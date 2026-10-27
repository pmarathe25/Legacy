//! Independent drafts merge around ordered effect barriers.
use super::*;
use std::sync::atomic::Ordering::Relaxed;
impl<'a> Transaction<'a> {
    pub(super) fn external_effect(&mut self) -> Result<()> {
        if self.parent.is_some() {
            self.blocked = true;
            return Err(Error::Rejected("isolated effect".into()));
        }
        Ok(())
    }
    fn fork<'b, T: Resource>(&'b self, value: T) -> Transaction<'b> {
        let data = HashMap::from_iter([(TypeId::of::<T>(), Arc::new(value) as Stored)]);
        let mut child = Transaction::new(self.host, self.at);
        child.data = Some(data);
        child.scratch = Some(TypeId::of::<T>());
        let parent = self.parent.filter(|_| self.scratch == child.scratch);
        child.parent = parent.or(Some(self));
        child.owner = self.owner;
        child.calls = self.calls;
        child.depth = self.depth;
        child.reads = self.reads.clone();
        child.versions = None;
        child
    }
    /// Execute one registered callback under its original owner and dispatch budget.
    pub fn listener<E: Event>(&mut self, event: &E, name: &str) -> Result<()> {
        self.budget("notification limit exceeded")?;
        let listener = self
            .host
            .registry
            .listeners
            .get(E::KEY)
            .and_then(|m| m.get(name))
            .ok_or_else(|| Error::Missing(name.into()))?;
        self.depth += 1;
        let result = listener(self, event);
        self.depth -= 1;
        require(!self.blocked, "isolated effect")?;
        result
    }
    /// Private scratch may be reused across callbacks; world effects remain blocked.
    pub fn draft<T: Resource>(
        &self,
        scratch: T,
        run: impl FnOnce(&mut Transaction<'_>) -> Result<()>,
    ) -> game_contracts::Draft<T> {
        let mut child = self.fork(scratch);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut child)));
        let accepted = !child.blocked && matches!(result, Ok(Ok(())));
        game_contracts::Draft {
            state: game_contracts::take(&mut child.finish(accepted).data),
            accepted,
        }
    }
    pub fn notify_prepared<E: Event + Sync, T: Resource>(
        &mut self,
        event: E,
        scratch: impl Fn() -> T + Sync,
        mut merge: impl FnMut(&mut Self, T) -> Result<()>,
    ) -> Result<()> {
        if !self.host.parallel_listeners || self.compute().current_num_threads() == 1 {
            return self.notify(event);
        }
        self.budget("notification limit exceeded")?;
        let host = self.host;
        let Some(listeners) = host.registry.listeners.get(E::KEY) else {
            return Ok(());
        };
        let metadata = host.registry.isolated.get(E::KEY);
        let entries: Vec<_> = listeners.iter().collect();
        let independent = |name: &String| {
            metadata
                .and_then(|m| m.get(name))
                .is_some_and(|m| m.ready::<T>())
        };
        if self.parent.is_none() {
            self.versions.get_or_insert_with(Default::default);
        }
        self.depth += 1;
        let base = self.calls;
        let at = self.versions.as_ref().map_or(0, |v| v.clock());
        let independent: Vec<_> = entries.iter().map(|(name, _)| independent(name)).collect();
        let result = game_contracts::drafting::ordered(
            self,
            &host.pool,
            &independent,
            |tx, i| {
                let mut child = tx.fork(scratch());
                let reads = Arc::new(game_contracts::drafting::Reads::default());
                child.reads = Some(reads.clone());
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    entries[i].1(&mut child, &event)
                }))
                .map_err(|_| Error::WorkerFailure)
                .and_then(|r| r);
                let blocked = child.blocked;
                let calls = child.calls - base;
                let mut changes = child.finish(result.is_ok());
                let state = game_contracts::take::<T>(&mut changes.data);
                host.drafts.drafted.fetch_add(1, Relaxed);
                (result, blocked, calls, state, reads)
            },
            |tx, i, draft| {
                let Some((result, blocked, calls, state, reads)) = draft else {
                    return entries[i].1(tx, &event);
                };
                let valid = tx.versions.as_ref().is_none_or(|v| v.valid(&reads, at));
                if blocked || !valid {
                    if tx.parent.is_some() {
                        tx.blocked = true;
                        return Err(Error::Rejected("isolated fallback".into()));
                    }
                    host.drafts.retry(blocked);
                    if blocked {
                        metadata.unwrap()[entries[i].0].block();
                    }
                    return entries[i].1(tx, &event);
                }
                result?;
                require(
                    tx.calls.checked_add(calls).is_some_and(|n| n <= MAX_CALLS),
                    "request dispatch limit exceeded",
                )?;
                let before = tx.calls;
                merge(tx, state)?;
                tx.calls = before + calls;
                host.drafts.accepted.fetch_add(1, Relaxed);
                if let Some(outer) = &tx.reads {
                    outer.include(&reads);
                }
                Ok(())
            },
        );
        self.depth -= 1;
        result
    }
}

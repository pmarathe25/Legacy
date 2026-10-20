//! Scratch-only proposal preparation. Execution and effects retain the serial path.
use super::*;
use rayon::prelude::*;
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
        child
    }
    /// Run a scratch request alongside a read-only query. Any attempted external
    /// effect or retained scratch change discards both drafts and retries serially.
    pub fn prepare<R: Request, T: Resource + Eq, Q: Send>(
        &mut self,
        request: R,
        query: impl Fn(&Transaction<'_>) -> Result<Q> + Sync,
    ) -> Result<(R::Response, Q)>
    where
        R::Response: Send,
    {
        if self.compute().current_num_threads() > 1 {
            let initial = self.get::<T>()?.clone();
            let mut draft = self.fork(initial.clone());
            let (result, extra) = self.compute().join(
                || {
                    let result = draft.call(request.clone());
                    let accept = !draft.blocked && draft.get::<T>().is_ok_and(|v| *v == initial);
                    (result, accept, draft.calls)
                },
                || query(self),
            );
            let (result, accept, calls) = result;
            drop(draft);
            if accept {
                self.calls = calls;
                return Ok((result?, extra?));
            }
        }
        Ok((self.call(request)?, query(self)?))
    }
    /// Independent subscribers draft private receiver state, then merge in the
    /// ordinary subscriber order. Ordinary subscribers remain serial barriers.
    pub fn notify_prepared<E: Event + Sync, T: Resource>(
        &mut self,
        event: E,
        scratch: impl Fn() -> T + Sync,
        mut merge: impl FnMut(&mut Self, T) -> Result<()>,
    ) -> Result<()> {
        if self.parent.is_none() || self.compute().current_num_threads() == 1 {
            return self.notify(event);
        }
        self.notification_budget()?;
        let host = self.host;
        let Some(listeners) = host.registry.listeners.get(E::KEY) else {
            return Ok(());
        };
        let metadata = host.registry.isolated.get(E::KEY);
        let entries: Vec<_> = listeners.iter().collect();
        let independent =
            |name: &String| metadata.and_then(|m| m.get(name)) == Some(&TypeId::of::<T>());
        self.depth += 1;
        let result = (|| {
            let mut i = 0;
            while i < entries.len() {
                if !independent(entries[i].0) {
                    entries[i].1(self, &event)?;
                    i += 1;
                    continue;
                }
                let end = i + entries[i..]
                    .iter()
                    .take_while(|(name, _)| independent(name))
                    .count();
                let base = self.calls;
                let rows: Vec<_> = self.compute().install(|| {
                    entries[i..end]
                        .par_iter()
                        .map(|(_, listener)| {
                            let mut child = self.fork(scratch());
                            let result = listener(&mut child, &event);
                            let blocked = child.blocked;
                            let calls = child.calls - base;
                            let mut changes = child.finish(result.is_ok());
                            let state = changes
                                .data
                                .remove(&TypeId::of::<T>())
                                .unwrap()
                                .downcast::<T>()
                                .ok()
                                .unwrap();
                            (result, blocked, calls, Arc::try_unwrap(state).ok().unwrap())
                        })
                        .collect()
                });
                if rows.iter().any(|(_, blocked, _, _)| *blocked)
                    || rows
                        .iter()
                        .try_fold(base, |n, (_, _, calls, _)| n.checked_add(*calls))
                        .is_none_or(|n| n > MAX_CALLS)
                {
                    self.blocked = true;
                    return Err(Error::Rejected("isolated fallback".into()));
                }
                for (result, _, calls, state) in rows {
                    result?;
                    let before = self.calls;
                    merge(self, state)?;
                    self.calls = before + calls;
                }
                i = end;
            }
            Ok(())
        })();
        self.depth -= 1;
        result
    }
}

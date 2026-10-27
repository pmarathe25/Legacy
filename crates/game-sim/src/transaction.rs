use crate::*;
/// Select whether a nested owner operation commits or rolls back its effects.
pub enum AttemptOutcome<T> {
    Commit(T),
    Rollback(T),
}
// Independent events in a 20,000-person phase must fit; recursion remains depth-limited.
pub(super) const MAX_CALLS: usize = 1_000_000;
pub(crate) struct Changes {
    pub data: HashMap<TypeId, Stored>,
    pub jobs: Vec<Job>,
    pub cancelled: Vec<ScheduleId>,
    pub notices: Vec<Notice>,
    pub next_id: u64,
    pub next_notice: u64,
}
pub struct Transaction<'a> {
    parent: Option<&'a Transaction<'a>>,
    blocked: bool,
    scratch: Option<TypeId>,
    host: &'a Host,
    owner: usize,
    at: SimTime,
    data: Option<HashMap<TypeId, Stored>>,
    frames: Vec<HashMap<TypeId, Checkpoint>>,
    jobs: Vec<Job>,
    cancelled: Vec<ScheduleId>,
    cancelled_jobs: Vec<usize>,
    notices: Vec<Notice>,
    next_id: u64,
    next_notice: u64,
    calls: usize,
    depth: usize,
    reads: Option<Arc<game_contracts::drafting::Reads>>,
    versions: Option<Box<game_contracts::drafting::Versions>>,
}
impl<'a> Transaction<'a> {
    pub(crate) fn new(host: &'a Host, at: SimTime) -> Self {
        Self {
            parent: None,
            blocked: false,
            scratch: None,
            host,
            owner: 0,
            at,
            data: None,
            frames: vec![],
            jobs: vec![],
            cancelled: vec![],
            cancelled_jobs: vec![],
            notices: vec![],
            next_id: host.next_id,
            next_notice: host.next_notice,
            calls: 0,
            depth: 0,
            reads: None,
            versions: None,
        }
    }
    pub(crate) fn owned(host: &'a Host, at: SimTime, data: HashMap<TypeId, Stored>) -> Self {
        let mut tx = Self::new(host, at);
        tx.data = Some(data);
        tx.frames.push(HashMap::default());
        tx
    }
    pub(crate) fn with_owner<T>(
        &mut self,
        owner: usize,
        operation: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let previous = std::mem::replace(&mut self.owner, owner);
        let result = operation(self);
        self.owner = previous;
        result
    }
    pub fn revision_after(&self) -> u64 {
        self.host.revision + 1
    }
    pub(crate) fn read_only(&self) -> bool {
        self.frames.iter().all(|f| f.is_empty())
            && self.jobs.is_empty()
            && self.cancelled.is_empty()
            && self.notices.is_empty()
            && self.next_id == self.host.next_id
            && self.next_notice == self.host.next_notice
    }
    /// Bulk owner operations retain the ordinary request path when hooks exist.
    pub fn has_before<R: Request>(&self) -> bool {
        self.host
            .registry
            .before
            .get(R::KEY)
            .is_some_and(|checks| !checks.is_empty())
    }
    pub fn registered<R: Request>(&self) -> bool {
        self.host.registered::<R>()
    }
    pub fn now(&self) -> SimTime {
        self.at
    }
    pub fn seed(&self) -> u64 {
        self.host.seed
    }
    pub fn compute(&self) -> &rayon::ThreadPool {
        &self.host.pool
    }
    #[inline]
    pub fn get<T: Resource>(&self) -> Result<&T> {
        game_contracts::drafting::read::<T>(&self.reads, self.scratch);
        if let Some(parent) = self.parent
            && self.scratch != Some(TypeId::of::<T>())
        {
            return parent.get();
        }
        registry::read(self.data.as_ref().unwrap_or(&self.host.data))
    }
    pub fn edit<T: Resource>(&mut self) -> Result<&mut T> {
        let owner = self
            .host
            .registry
            .resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| Error::Missing(T::KEY.into()))?
            .owner;
        require(
            owner == self.owner,
            "only the registered resource owner may mutate it",
        )?;
        self.edit_owned::<T>()
    }
    pub fn edit_with_compute<T: Resource, R>(
        &mut self,
        f: impl FnOnce(&mut T, &rayon::ThreadPool) -> R,
    ) -> Result<R> {
        let host = self.host;
        let state = self.edit::<T>()?;
        Ok(f(state, &host.pool))
    }
    fn edit_owned<T: Resource>(&mut self) -> Result<&mut T> {
        if let Some(versions) = &mut self.versions {
            versions.write::<T>();
        }
        let id = TypeId::of::<T>();
        if self.parent.is_some() && self.scratch != Some(id) {
            self.blocked = true;
            return Err(Error::Rejected("isolated write".into()));
        }
        let data = self
            .data
            .as_mut()
            .ok_or_else(|| Error::Rejected("query attempted a state change".into()))?;
        let state = data
            .get_mut(&id)
            .ok_or_else(|| Error::Missing(T::KEY.into()))?;
        let codec = &self.host.registry.resources[&id];
        for frame in &mut self.frames {
            frame.entry(id).or_insert_with(|| (codec.checkpoint)(state));
        }
        Ok(registry::mutable(state))
    }
    fn resolve(&mut self, frame: HashMap<TypeId, Checkpoint>, commit: bool) {
        for (id, checkpoint) in frame {
            let codec = &self.host.registry.resources[&id];
            let data = self.data.as_mut().expect("writable transaction");
            let state = data.get_mut(&id).unwrap();
            if commit {
                (codec.commit)(state, checkpoint);
            } else {
                (codec.restore)(state, checkpoint);
            }
        }
    }
    pub fn allocate(&mut self) -> Result<u64> {
        self.external_effect()?;
        require(
            self.data.is_some(),
            "query attempted to allocate an identity",
        )?;
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        Ok(id)
    }
    pub fn invoke(&mut self, mut pending: Pending) -> Result<Value> {
        pending.bind(&self.host.registry)?;
        pending.invoke(self)
    }
    pub fn call<R: Request>(&mut self, request: R) -> Result<R::Response> {
        self.dispatch_request(request, |tx, request, handler| (handler.run)(tx, request))
    }
    fn dispatch_request<R: Request, T>(
        &mut self,
        request: R,
        run: impl FnOnce(&mut Transaction<'_>, R, &registry::Typed<R>) -> Result<T>,
    ) -> Result<T> {
        let _cause = game_contracts::causal::scope("request", R::KEY);
        self.budget("request dispatch limit exceeded")?;
        let host = self.host;
        let handler = host
            .registry
            .typed
            .get(&TypeId::of::<R>())
            .ok_or_else(|| Error::Missing(R::KEY.into()))?
            .downcast_ref::<registry::Typed<R>>()
            .expect("registered request type");
        self.depth += 1;
        let result = (|| {
            let host = self.host;
            if let Some(checks) = host.registry.before.get(R::KEY) {
                for check in checks.values() {
                    check(self, &request)?;
                }
            }
            self.with_owner(handler.owner, |tx| run(tx, request, handler))
        })();
        self.depth -= 1;
        result
    }
    pub fn attempt<R: Request>(&mut self, request: R) -> Result<R::Response> {
        self.attempt_controlled(|tx| tx.call(request).map(AttemptOutcome::Commit))
    }
    /// Run an owner's alternative handler with the target request's normal
    /// hooks and a savepoint. Foreign owners may not replace its handler.
    pub fn attempt_owned_request<R: Request, T>(
        &mut self,
        request: R,
        body: impl FnOnce(&mut Transaction<'_>, R) -> Result<AttemptOutcome<T>>,
    ) -> Result<T> {
        require(
            self.host.registry.request_owners.get(R::KEY) == Some(&self.owner),
            "request belongs to another owner",
        )?;
        self.attempt_controlled(|tx| tx.dispatch_request(request, |tx, r, _| body(tx, r)))
    }
    fn attempt_controlled<T>(
        &mut self,
        body: impl FnOnce(&mut Transaction<'_>) -> Result<AttemptOutcome<T>>,
    ) -> Result<T> {
        let checkpoint = (
            self.jobs.len(),
            self.cancelled.len(),
            self.notices.len(),
            self.cancelled_jobs.len(),
            self.next_id,
            self.next_notice,
        );
        self.frames.push(HashMap::default());
        let result = body(self);
        let frame = self.frames.pop().expect("attempt frame");
        let commit = matches!(&result, Ok(AttemptOutcome::Commit(_)));
        self.resolve(frame, commit);
        if !commit {
            self.jobs.truncate(checkpoint.0);
            self.cancelled.truncate(checkpoint.1);
            self.notices.truncate(checkpoint.2);
            self.cancelled_jobs.truncate(checkpoint.3);
            self.next_id = checkpoint.4;
            self.next_notice = checkpoint.5;
        }
        match result {
            Ok(AttemptOutcome::Commit(value) | AttemptOutcome::Rollback(value)) => Ok(value),
            Err(error) => Err(error),
        }
    }
    pub fn schedule<R: Request>(
        &mut self,
        at: SimTime,
        priority: u8,
        request: R,
    ) -> Result<ScheduleId> {
        require(at >= self.at, "cannot schedule in the past")?;
        require(self.registered::<R>(), "unregistered scheduled request")?;
        let id = ScheduleId(self.allocate()?);
        self.jobs.push(Job {
            id,
            at,
            priority,
            request: Pending::new(request)?,
        });
        Ok(id)
    }
    pub(crate) fn cancelled(&self, id: ScheduleId) -> bool {
        self.cancelled.contains(&id)
    }
    pub(crate) fn precedes(&self, key: (SimTime, u8, ScheduleId)) -> bool {
        self.jobs.iter().enumerate().any(|(i, job)| {
            (job.at, job.priority, job.id) < key && !self.cancelled_jobs.contains(&i)
        })
    }
    /// Cancel this owner's callbacks of one type, including staged work.
    pub fn cancel_scheduled<R: Request>(&mut self) -> Result<()> {
        self.external_effect()?;
        let owner = self.host.registry.request_owners.get(R::KEY);
        require(
            owner == Some(&self.owner),
            "scheduled work belongs to owner",
        )?;
        let jobs = self.host.jobs.values().chain(&self.jobs);
        let owned = jobs.filter(|job| job.request.key() == R::KEY);
        let ids: Vec<_> = owned.map(|job| job.id).collect();
        for id in ids {
            self.unschedule(id);
        }
        Ok(())
    }
    pub fn unschedule(&mut self, id: ScheduleId) {
        if self.external_effect().is_err() {
            return;
        }
        self.cancelled.push(id);
        if let Ok(index) = self.jobs.binary_search_by_key(&id, |job| job.id) {
            self.cancelled_jobs.push(index);
        }
    }
    pub fn emit<E: Event>(&mut self, event: E, witnesses: Vec<PersonId>) -> Result<()> {
        self.external_effect()?;
        self.budget("notification limit exceeded")?;
        let data = serde_json::to_value(&event).map_err(registry::json_error)?;
        let sequence = self.next_notice;
        self.next_notice = sequence.checked_add(1).ok_or(Error::Overflow)?;
        self.notices.push(Notice {
            sequence,
            at: self.at,
            kind: E::KEY.into(),
            data,
            witnesses,
        });
        self.notify_event(&event)
    }
    pub fn notify<E: Event>(&mut self, event: E) -> Result<()> {
        self.budget("notification limit exceeded")?;
        self.notify_event(&event)
    }
    fn budget(&mut self, message: &str) -> Result<()> {
        self.calls += 1;
        require(self.calls <= MAX_CALLS && self.depth < 32, message)
    }
    fn notify_event<E: Event>(&mut self, event: &E) -> Result<()> {
        let _cause = game_contracts::causal::scope("event", E::KEY);
        let host = self.host;
        let Some(listeners) = host.registry.listeners.get(E::KEY) else {
            return Ok(());
        };
        self.depth += 1;
        let result = (|| {
            for listener in listeners.values() {
                listener(self, event)?;
            }
            Ok(())
        })();
        self.depth -= 1;
        result
    }
    pub fn start_action<R: Request>(
        &mut self,
        person: PersonId,
        kind: &str,
        until: Option<SimTime>,
        cancel: R,
    ) -> Result<()> {
        require(
            !self.get::<Actions>()?.0.contains_key(&person),
            "actor is busy",
        )?;
        require(
            self.registered::<R>()
                && self.host.registry.request_owners.get(R::KEY) == Some(&self.owner),
            "cancellation must be registered to the action owner",
        )?;
        let action = Action {
            id: self.allocate()?,
            owner: self.host.registry.owners[self.owner].clone(),
            kind: kind.into(),
            until,
            cancel: Pending::new(cancel)?,
        };
        self.edit_owned::<Actions>()?.0.insert(person, action);
        Ok(())
    }
    pub fn finish_action(&mut self, person: PersonId, kind: &str) -> Result<()> {
        let event = self
            .get::<Actions>()?
            .0
            .get(&person)
            .filter(|a| a.kind == kind && a.owner == self.host.registry.owners[self.owner])
            .map(|a| ActionFinished::new(person, a.id, true));
        require(event.is_some(), "action has changed")?;
        self.edit_owned::<Actions>()?.0.erase(&person);
        self.notify(event.unwrap())
    }
    pub fn cancel_action(&mut self, person: PersonId) -> Result<()> {
        if let Some(action) = self.get::<Actions>()?.0.get(&person).cloned() {
            action.cancel.invoke(self)?;
            if self.edit_owned::<Actions>()?.0.erase(&person) {
                self.notify(ActionFinished::new(person, action.id, false))?;
            }
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<()> {
        for (id, checkpoint) in &self.frames[0] {
            let codec = &self.host.registry.resources[id];
            (codec.validate_checkpoint)(&self.data.as_ref().unwrap()[id], checkpoint.as_ref())?;
        }
        self.host.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn finish(mut self, commit: bool) -> Changes {
        while let Some(frame) = self.frames.pop() {
            self.resolve(frame, commit);
        }
        game_contracts::remove_cancelled(&mut self.jobs, &mut self.cancelled_jobs);
        Changes {
            data: self.data.take().expect("writable transaction"),
            jobs: self.jobs,
            cancelled: self.cancelled,
            notices: self.notices,
            next_id: self.next_id,
            next_notice: self.next_notice,
        }
    }
}

#[path = "preparation.rs"]
mod preparation;

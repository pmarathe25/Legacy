use crate::*;
// Independent events in a 20,000-person phase must fit; recursion remains depth-limited.
const MAX_CALLS: usize = 1_000_000;
pub(crate) struct Changes {
    pub data: HashMap<TypeId, Stored>,
    pub jobs: Vec<Job>,
    pub cancelled: Vec<ScheduleId>,
    pub notices: Vec<Notice>,
    pub next_id: u64,
    pub next_notice: u64,
}
pub struct Transaction<'a> {
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
}
impl<'a> Transaction<'a> {
    pub(crate) fn new(host: &'a Host, at: SimTime) -> Self {
        Self {
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
    pub fn get<T: Resource>(&self) -> Result<&T> {
        self.data
            .as_ref()
            .unwrap_or(&self.host.data)
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref())
            .ok_or_else(|| Error::Missing(T::KEY.into()))
    }
    /// Stage a resource snapshot. Persistent tables copy only changed paths and records.
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
        let id = TypeId::of::<T>();
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
        Ok(Arc::get_mut(state)
            .expect("exclusive transaction resource")
            .downcast_mut()
            .expect("registered resource type"))
    }
    fn resolve(&mut self, frame: HashMap<TypeId, Checkpoint>, commit: bool) {
        for (id, checkpoint) in frame {
            let codec = &self.host.registry.resources[&id];
            let state = self
                .data
                .as_mut()
                .expect("writable transaction")
                .get_mut(&id)
                .unwrap();
            if commit {
                (codec.commit)(state, checkpoint);
            } else {
                (codec.restore)(state, checkpoint);
            }
        }
    }

    pub fn allocate(&mut self) -> Result<u64> {
        require(
            self.data.is_some(),
            "query attempted to allocate an identity",
        )?;
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        Ok(id)
    }
    pub fn call<R: Request>(&mut self, request: R) -> Result<R::Response> {
        self.calls += 1;
        require(
            self.calls <= MAX_CALLS && self.depth < 32,
            "request dispatch limit exceeded",
        )?;
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
            self.with_owner(handler.owner, |tx| (handler.run)(tx, request))
        })();
        self.depth -= 1;
        result
    }
    /// A failed nested attempt restores its own changes while retaining its caller's work.
    pub fn attempt<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let checkpoint = (
            self.jobs.len(),
            self.cancelled.len(),
            self.notices.len(),
            self.cancelled_jobs.len(),
            self.next_id,
            self.next_notice,
        );
        self.frames.push(HashMap::default());
        let result = self.call(request);
        let frame = self.frames.pop().expect("attempt frame");
        self.resolve(frame, result.is_ok());
        if result.is_err() {
            self.jobs.truncate(checkpoint.0);
            self.cancelled.truncate(checkpoint.1);
            self.notices.truncate(checkpoint.2);
            self.cancelled_jobs.truncate(checkpoint.3);
            self.next_id = checkpoint.4;
            self.next_notice = checkpoint.5;
        }
        result
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
    pub fn unschedule(&mut self, id: ScheduleId) {
        self.cancelled.push(id);
        // Allocated IDs increase in append order. Only work already staged at
        // this call is cancelled; an unknown future ID cannot cancel a later job.
        if let Ok(index) = self.jobs.binary_search_by_key(&id, |job| job.id) {
            self.cancelled_jobs.push(index);
        }
    }
    pub fn emit<E: Event>(&mut self, event: E, witnesses: Vec<PersonId>) -> Result<()> {
        self.notification_budget()?;
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
    /// Notify feature subscribers without publishing a notice.
    pub fn notify<E: Event>(&mut self, event: E) -> Result<()> {
        self.notification_budget()?;
        self.notify_event(&event)
    }
    fn notification_budget(&mut self) -> Result<()> {
        self.calls += 1;
        require(
            self.calls <= MAX_CALLS && self.depth < 32,
            "notification limit exceeded",
        )?;
        Ok(())
    }
    fn notify_event<E: Event>(&mut self, event: &E) -> Result<()> {
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
            self.host.registry.request_owners.get(R::KEY) == Some(&self.owner),
            "cancellation must belong to the action owner",
        )?;
        let action = Action {
            owner: self.host.registry.owners[self.owner].clone(),
            kind: kind.into(),
            until,
            cancel: Pending::new(cancel)?,
        };
        require(
            self.registered::<R>(),
            "missing action cancellation handler",
        )?;
        self.edit_owned::<Actions>()?.0.insert(person, action);
        Ok(())
    }
    pub fn finish_action(&mut self, person: PersonId, kind: &str) -> Result<()> {
        require(
            self.get::<Actions>()?.0.get(&person).is_some_and(|a| {
                a.kind == kind && a.owner == self.host.registry.owners[self.owner]
            }),
            "action has changed",
        )?;
        self.edit_owned::<Actions>()?.0.erase(&person);
        self.notify(ActionFinished { person })
    }
    pub fn cancel_action(&mut self, person: PersonId) -> Result<()> {
        if let Some(action) = self.get::<Actions>()?.0.get(&person).cloned() {
            action.cancel.invoke(self)?;
            if self.edit_owned::<Actions>()?.0.erase(&person) {
                self.notify(ActionFinished { person })?;
            }
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<()> {
        // Each feature owns validation of its state. Cross-feature checks can be registered as listeners.
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

        if !self.cancelled_jobs.is_empty() {
            self.cancelled_jobs.sort_unstable();
            self.cancelled_jobs.dedup();
            let mut i = 0;
            self.jobs.retain(|_| {
                let keep = self.cancelled_jobs.binary_search(&i).is_err();
                i += 1;
                keep
            });
        }
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

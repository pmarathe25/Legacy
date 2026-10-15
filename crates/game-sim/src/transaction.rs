use crate::*;
// Independent events in a 20,000-person phase must fit; recursion remains depth-limited.
const MAX_CALLS: usize = 1_000_000;
use game_storage::Table;
pub(crate) struct Changes {
    pub writes: Table<TypeId, Stored>,
    pub jobs: Vec<Job>,
    pub cancelled: Vec<ScheduleId>,
    pub notices: Vec<Notice>,
    pub next_id: u64,
    pub next_notice: u64,
}
pub struct Transaction<'a> {
    host: &'a Host,
    owner: Arc<str>,
    at: SimTime,
    writes: Table<TypeId, Stored>,
    jobs: Records<ScheduleId, Job>,
    cancelled: Vec<ScheduleId>,
    notices: Records<u64, Notice>,
    next_id: u64,
    next_notice: u64,
    calls: usize,
    depth: usize,
}
impl<'a> Transaction<'a> {
    pub(crate) fn new(host: &'a Host, at: SimTime) -> Self {
        Self {
            host,
            owner: "sim".into(),
            at,
            writes: Table::new(),
            jobs: Records::new(),
            cancelled: vec![],
            notices: Records::new(),
            next_id: host.next_id,
            next_notice: host.next_notice,
            calls: 0,
            depth: 0,
        }
    }
    pub(crate) fn with_owner<T>(
        &mut self,
        owner: &Arc<str>,
        operation: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let previous = std::mem::replace(&mut self.owner, owner.clone());
        let result = operation(self);
        self.owner = previous;
        result
    }
    pub fn revision_after(&self) -> u64 {
        self.host.revision + 1
    }
    pub(crate) fn read_only(&self) -> bool {
        self.writes.is_empty()
            && self.jobs.is_empty()
            && self.cancelled.is_empty()
            && self.notices.is_empty()
            && self.next_id == self.host.next_id
            && self.next_notice == self.host.next_notice
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
        if let Some(s) = self.writes.get(&TypeId::of::<T>()) {
            return s
                .downcast_ref()
                .ok_or_else(|| Error::Missing(T::KEY.into()));
        }
        self.host.get::<T>()
    }
    /// Stage a resource snapshot. Persistent tables copy only changed paths and records.
    pub fn edit<T: Resource>(&mut self) -> Result<&mut T> {
        let owner = &self
            .host
            .registry
            .resources
            .get(&TypeId::of::<T>())
            .ok_or_else(|| Error::Missing(T::KEY.into()))?
            .owner;
        require(
            owner == &self.owner,
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
        let state = self
            .writes
            .entry(id)
            .or_insert_with(|| self.host.data[&id].clone());
        if Arc::get_mut(state).is_none() {
            *state = Arc::new(state.downcast_ref::<T>().expect("registered type").clone());
        }
        Ok(Arc::get_mut(state)
            .expect("unique staged resource")
            .downcast_mut()
            .expect("registered type"))
    }

    pub fn allocate(&mut self) -> Result<u64> {
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
        let handler = self
            .host
            .registry
            .typed
            .get(&TypeId::of::<R>())
            .cloned()
            .ok_or_else(|| Error::Missing(R::KEY.into()))?
            .downcast::<registry::Typed<R>>()
            .expect("registered request type");
        self.depth += 1;
        let result = (|| {
            if let Some(checks) = self.host.registry.before.get(R::KEY) {
                let data = serde_json::to_value(&request).map_err(registry::json_error)?;
                let checks: Vec<_> = checks.values().cloned().collect();
                for check in checks {
                    check(self, data.clone())?;
                }
            }
            self.with_owner(&handler.owner, |tx| (handler.run)(tx, request))
        })();
        self.depth -= 1;
        result
    }
    /// A failed nested attempt restores its own changes while retaining its caller's work.
    pub fn attempt<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let checkpoint = (
            self.writes.clone(),
            self.jobs.clone(),
            self.cancelled.clone(),
            self.notices.clone(),
            self.next_id,
            self.next_notice,
        );
        let result = self.call(request);
        if result.is_err() {
            (
                self.writes,
                self.jobs,
                self.cancelled,
                self.notices,
                self.next_id,
                self.next_notice,
            ) = checkpoint;
        }
        result
    }
    pub(crate) fn call_envelope(&mut self, request: Envelope) -> Result<Value> {
        let handler = self
            .host
            .registry
            .handlers
            .get(&request.key)
            .ok_or_else(|| Error::Missing(request.key.clone()))?
            .clone();
        handler(self, request.payload)
    }
    pub fn schedule<R: Request>(
        &mut self,
        at: SimTime,
        priority: u8,
        request: R,
    ) -> Result<ScheduleId> {
        require(at >= self.at, "cannot schedule in the past")?;
        require(
            self.host.registry.handlers.contains_key(R::KEY),
            "unregistered scheduled request",
        )?;
        let id = ScheduleId(self.allocate()?);
        self.jobs.insert(
            id,
            Job {
                id,
                at,
                priority,
                request: Envelope {
                    key: R::KEY.into(),
                    payload: serde_json::to_value(request).map_err(registry::json_error)?,
                },
            },
        );
        Ok(id)
    }
    pub(crate) fn cancelled(&self, id: ScheduleId) -> bool {
        self.cancelled.contains(&id)
    }
    pub(crate) fn precedes(&self, key: (SimTime, u8, ScheduleId)) -> bool {
        self.jobs
            .values()
            .any(|job| (job.at, job.priority, job.id) < key)
    }
    pub fn unschedule(&mut self, id: ScheduleId) {
        self.cancelled.push(id);
        self.jobs.remove(&id);
    }
    pub fn emit<E: Event>(&mut self, event: E, witnesses: Vec<PersonId>) -> Result<()> {
        self.notification_budget()?;
        let data = serde_json::to_value(event).map_err(registry::json_error)?;
        let sequence = self.next_notice;
        self.next_notice = sequence.checked_add(1).ok_or(Error::Overflow)?;
        self.notices.insert(
            sequence,
            Notice {
                sequence,
                at: self.at,
                kind: E::KEY.into(),
                data: data.clone(),
                witnesses,
            },
        );
        self.notify_data(E::KEY, data)
    }
    /// Notify feature subscribers without publishing a notice.
    pub fn notify<E: Event>(&mut self, event: E) -> Result<()> {
        self.notification_budget()?;
        if !self.host.registry.listeners.contains_key(E::KEY) {
            return Ok(());
        }
        let data = serde_json::to_value(event).map_err(registry::json_error)?;
        self.notify_data(E::KEY, data)
    }
    fn notification_budget(&mut self) -> Result<()> {
        self.calls += 1;
        require(
            self.calls <= MAX_CALLS && self.depth < 32,
            "notification limit exceeded",
        )?;
        Ok(())
    }
    fn notify_data(&mut self, key: &str, data: Value) -> Result<()> {
        let listeners: Vec<_> = self
            .host
            .registry
            .listeners
            .get(key)
            .into_iter()
            .flat_map(|m| m.values().cloned())
            .collect();
        self.depth += 1;
        let result = (|| {
            for listener in listeners {
                listener(self, data.clone())?;
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
            owner: self.owner.clone(),
            kind: kind.into(),
            until,
            cancel: Envelope {
                key: R::KEY.into(),
                payload: serde_json::to_value(cancel).map_err(registry::json_error)?,
            },
        };
        require(
            self.host.registry.handlers.contains_key(&action.cancel.key),
            "missing action cancellation handler",
        )?;
        self.edit_owned::<Actions>()?.0.insert(person, action);
        Ok(())
    }
    pub fn finish_action(&mut self, person: PersonId, kind: &str) -> Result<()> {
        require(
            self.get::<Actions>()?
                .0
                .get(&person)
                .is_some_and(|a| a.kind == kind && a.owner == self.owner),
            "action has changed",
        )?;
        self.edit_owned::<Actions>()?.0.remove(&person);
        self.notify(ActionFinished { person })
    }
    pub fn cancel_action(&mut self, person: PersonId) -> Result<()> {
        if let Some(action) = self.get::<Actions>()?.0.get(&person).cloned() {
            self.call_envelope(action.cancel)?;
            if self.edit_owned::<Actions>()?.0.remove(&person).is_some() {
                self.notify(ActionFinished { person })?;
            }
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<()> {
        // Each feature owns validation of its state. Cross-feature checks can be registered as listeners.
        for (id, write) in &self.writes {
            let codec = &self.host.registry.resources[id];
            (codec.validate_changes)(write, &self.host.data[id])?;
        }
        self.host.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn finish(self) -> Changes {
        Changes {
            writes: self.writes,
            jobs: self.jobs.values().cloned().collect(),
            cancelled: self.cancelled,
            notices: self.notices.values().cloned().collect(),
            next_id: self.next_id,
            next_notice: self.next_notice,
        }
    }
}

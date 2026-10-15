use crate::*;
use std::any::Any;
pub(crate) struct Changes {
    pub writes: BTreeMap<TypeId, Stored>,
    pub jobs: Vec<Job>,
    pub cancelled: Vec<ScheduleId>,
    pub notices: Vec<Notice>,
    pub next_id: u64,
    pub next_notice: u64,
}
pub struct Transaction<'a> {
    host: &'a Host,
    owner: String,
    at: SimTime,
    writes: BTreeMap<TypeId, Box<dyn Any + Send + Sync>>,
    jobs: Vec<Job>,
    cancelled: Vec<ScheduleId>,
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
            owner: "sim".into(),
            at,
            writes: BTreeMap::new(),
            jobs: vec![],
            cancelled: vec![],
            notices: vec![],
            next_id: host.next_id,
            next_notice: host.next_notice,
            calls: 0,
            depth: 0,
        }
    }
    pub(crate) fn with_owner<T>(
        &mut self,
        owner: &str,
        operation: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let previous = std::mem::replace(&mut self.owner, owner.to_owned());
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
    /// Copy only a touched resource; errors discard all writes, events, jobs, and allocated IDs.
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
    fn edit_owned<T: Resource>(&mut self) -> Result<&mut T> {
        let id = TypeId::of::<T>();
        if !self.writes.contains_key(&id) {
            let codec = self
                .host
                .registry
                .resources
                .get(&id)
                .ok_or_else(|| Error::Missing(T::KEY.into()))?;
            self.writes.insert(id, (codec.copy)(&self.host.data[&id]));
        }
        Ok(self
            .writes
            .get_mut(&id)
            .expect("inserted")
            .downcast_mut()
            .expect("registered type"))
    }
    pub fn allocate(&mut self) -> Result<u64> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        Ok(id)
    }
    pub fn call<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let value = self.call_envelope(Envelope {
            key: R::KEY.into(),
            payload: serde_json::to_value(request).map_err(registry::json_error)?,
        })?;
        serde_json::from_value(value).map_err(registry::json_error)
    }
    /// A failed nested attempt restores its own changes while retaining its caller's work.
    pub fn attempt<R: Request>(&mut self, request: R) -> Result<R::Response> {
        let writes = self
            .writes
            .iter()
            .map(|(id, value)| {
                (
                    *id,
                    (self.host.registry.resources[id].copy_borrowed)(value.as_ref()),
                )
            })
            .collect();
        let checkpoint = (
            writes,
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
        self.calls += 1;
        require(
            self.calls <= 10_000 && self.depth < 32,
            "request dispatch limit exceeded",
        )?;
        let handler = self
            .host
            .registry
            .handlers
            .get(&request.key)
            .ok_or_else(|| Error::Missing(request.key.clone()))?
            .clone();
        self.depth += 1;
        let checks: Vec<_> = self
            .host
            .registry
            .before
            .get(&request.key)
            .into_iter()
            .flat_map(|m| m.values().cloned())
            .collect();
        let result = (|| {
            for check in checks {
                check(self, request.payload.clone())?;
            }
            handler(self, request.payload)
        })();
        self.depth -= 1;
        result
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
        self.jobs.push(Job {
            id,
            at,
            priority,
            request: Envelope {
                key: R::KEY.into(),
                payload: serde_json::to_value(request).map_err(registry::json_error)?,
            },
        });
        Ok(id)
    }
    pub fn unschedule(&mut self, id: ScheduleId) {
        self.cancelled.push(id);
        self.jobs.retain(|j| j.id != id);
    }
    pub fn emit<E: Event>(&mut self, event: E, witnesses: Vec<PersonId>) -> Result<()> {
        self.calls += 1;
        require(
            self.calls <= 10_000 && self.depth < 32,
            "notification limit exceeded",
        )?;
        let data = serde_json::to_value(event).map_err(registry::json_error)?;
        let sequence = self.next_notice;
        self.next_notice = sequence.checked_add(1).ok_or(Error::Overflow)?;
        self.notices.push(Notice {
            sequence,
            at: self.at,
            kind: E::KEY.into(),
            data: data.clone(),
            witnesses,
        });
        let listeners: Vec<_> = self
            .host
            .registry
            .listeners
            .get(E::KEY)
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
        result?;
        Ok(())
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
        Ok(())
    }
    pub fn cancel_action(&mut self, person: PersonId) -> Result<()> {
        if let Some(action) = self.get::<Actions>()?.0.get(&person).cloned() {
            self.call_envelope(action.cancel)?;
            self.edit_owned::<Actions>()?.0.remove(&person);
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<()> {
        // Each feature owns validation of its state. Cross-feature checks can be registered as listeners.
        for (id, write) in &self.writes {
            let codec = &self.host.registry.resources[id];
            (codec.validate_borrowed)(write.as_ref())?;
        }
        self.host.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn finish(self) -> Changes {
        Changes {
            writes: self
                .writes
                .into_iter()
                .map(|(k, v)| (k, Arc::from(v)))
                .collect(),
            jobs: self.jobs,
            cancelled: self.cancelled,
            notices: self.notices,
            next_id: self.next_id,
            next_notice: self.next_notice,
        }
    }
}

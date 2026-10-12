//! Owner-local sparse row changes and physical read guards.
use crate::*;
use game_ecs::{Batch, Component, StableId, Store};
#[derive(Default)]
struct Rows<C> {
    edits: BTreeMap<PersonId, (Option<C>, Option<C>)>,
}
impl<C: Component> Rows<C> {
    fn new() -> Self {
        Self {
            edits: BTreeMap::new(),
        }
    }
    fn guard(&mut self, store: &Store, actor: PersonId) {
        self.edits.entry(actor).or_insert_with(|| {
            let row = store.get::<C>(StableId(actor.0)).cloned();
            (row.clone(), row)
        });
    }
    fn set(&mut self, store: &Store, actor: PersonId, after: Option<C>) {
        self.guard(store, actor);
        self.edits.get_mut(&actor).unwrap().1 = after;
    }
    fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .edits
            .iter()
            .any(|(actor, (before, _))| store.get::<C>(StableId(actor.0)) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    fn stage(self, batch: &mut Batch) {
        for (actor, (before, after)) in self.edits {
            if before == after {
                continue;
            }
            if let Some(row) = after {
                batch.insert(StableId(actor.0), row);
            } else {
                batch.remove::<C>(StableId(actor.0));
            }
        }
    }
}
#[derive(Clone)]
struct TopologyGuard {
    seed: BTreeSet<PersonId>,
    groups: Vec<(PersonId, CarryGroup)>,
    lifts: Vec<(PersonId, Lift)>,
}
impl TopologyGuard {
    fn new(store: &Store, seed: BTreeSet<PersonId>) -> Self {
        let linked = connected(store, &seed);
        Self {
            seed,
            groups: groups(store)
                .filter(|(_, row)| {
                    linked.contains(&row.passenger) || !row.bearers.is_disjoint(&linked)
                })
                .map(|(id, row)| (id, row.clone()))
                .collect(),
            lifts: lifts(store)
                .filter(|(_, row)| {
                    linked.contains(&row.passenger) || !row.bearers.is_disjoint(&linked)
                })
                .map(|(id, row)| (id, row.clone()))
                .collect(),
        }
    }
    fn valid(&self, store: &Store) -> bool {
        let after = Self::new(store, self.seed.clone());
        after.groups == self.groups && after.lifts == self.lifts
    }
}
pub(crate) fn connected(store: &Store, seed: &BTreeSet<PersonId>) -> BTreeSet<PersonId> {
    let mut result = seed.clone();
    loop {
        let before = result.len();
        for (_, row) in groups(store) {
            if result.contains(&row.passenger) || !row.bearers.is_disjoint(&result) {
                result.insert(row.passenger);
                result.extend(row.bearers.iter().copied());
            }
        }
        if before == result.len() {
            return result;
        }
    }
}
/// The result is typed owner data; the prepared value owns only touched rows.
pub struct PreparedTransport<T> {
    rules: Rules,
    at: Option<SimTime>,
    groups: Rows<CarryGroup>,
    offers: Rows<OfferInbox>,
    lifts: Rows<Lift>,
    approaches: Rows<Approach>,
    resume: Rows<ResumeTarget>,
    paused: Rows<PausedApproach>,
    knowledge: Rows<SupportKnowledge>,
    body_reads: BTreeMap<PersonId, Body>,
    topology: Vec<TopologyGuard>,
    journal: Option<(Vec<Response>, Vec<Response>)>,
    expired_inboxes: Option<(SimTime, Vec<(PersonId, OfferInbox)>)>,
    result: T,
}
impl<T> PreparedTransport<T> {
    fn new(runtime: &Runtime, result: T) -> Self {
        Self {
            rules: runtime.rules.clone(),
            at: None,
            groups: Rows::new(),
            offers: Rows::new(),
            lifts: Rows::new(),
            approaches: Rows::new(),
            resume: Rows::new(),
            paused: Rows::new(),
            knowledge: Rows::new(),
            body_reads: BTreeMap::new(),
            topology: Vec::new(),
            journal: None,
            expired_inboxes: None,
            result,
        }
    }
    pub fn result(&self) -> &T {
        &self.result
    }
    fn guard_bodies(
        &mut self,
        bodies: &impl BodyFacts,
        actors: impl IntoIterator<Item = PersonId>,
    ) -> Result<(), Error> {
        for actor in actors {
            self.body_reads
                .insert(actor, bodies.body(actor).ok_or(Error::Unavailable)?.clone());
        }
        Ok(())
    }
    fn guard_topology(
        &mut self,
        store: &Store,
        actors: impl IntoIterator<Item = PersonId>,
    ) -> BTreeSet<PersonId> {
        let seed = actors.into_iter().collect::<BTreeSet<_>>();
        let linked = connected(store, &seed);
        self.topology.push(TopologyGuard::new(store, seed));
        linked
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        now: SimTime,
    ) -> Result<(), Error> {
        if self.at.is_some_and(|at| at != now)
            || runtime.rules != self.rules
            || self
                .journal
                .as_ref()
                .is_some_and(|(before, _)| runtime.responses != *before)
            || self
                .body_reads
                .iter()
                .any(|(actor, before)| bodies.body(*actor).as_ref() != Some(before))
            || self.topology.iter().any(|guard| !guard.valid(store))
        {
            return Err(Error::Unavailable);
        }
        if self.expired_inboxes.as_ref().is_some_and(|(at, before)| {
            offer_inboxes(store)
                .filter(|(_, inbox)| inbox.by_bearer.values().any(|offer| offer.expires <= *at))
                .map(|(actor, row)| (actor, row.clone()))
                .collect::<Vec<_>>()
                != *before
        }) {
            return Err(Error::Unavailable);
        }
        self.groups.validate(store)?;
        self.offers.validate(store)?;
        self.lifts.validate(store)?;
        self.approaches.validate(store)?;
        self.resume.validate(store)?;
        self.paused.validate(store)?;
        self.knowledge.validate(store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        bodies: &impl BodyFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<TransportFinalization<T>, Error> {
        self.validate(runtime, store, bodies, now)?;
        self.groups.stage(batch);
        self.offers.stage(batch);
        self.lifts.stage(batch);
        self.approaches.stage(batch);
        self.resume.stage(batch);
        self.paused.stage(batch);
        self.knowledge.stage(batch);
        Ok(TransportFinalization {
            journal: self.journal.map(|(_, after)| after),
            result: self.result,
        })
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
        bodies: &impl BodyFacts,
        now: SimTime,
    ) -> Result<T, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, bodies, now, &mut batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
pub struct TransportFinalization<T> {
    journal: Option<Vec<Response>>,
    result: T,
}
impl<T> TransportFinalization<T> {
    pub fn finish(self, runtime: &mut Runtime) -> T {
        if let Some(journal) = self.journal {
            runtime.responses = journal;
        }
        self.result
    }
}
impl Runtime {
    pub fn component(&self, store: &Store, actor: PersonId) -> BTreeSet<PersonId> {
        connected(store, &BTreeSet::from([actor]))
    }
    pub fn busy(&self, store: &Store, actor: PersonId) -> bool {
        lifts(store).any(|(_, lift)| lift.passenger == actor || lift.bearers.contains(&actor))
    }
    pub fn prepare_offer(
        &self,
        store: &Store,
        bearer: PersonId,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<PreparedTransport<()>, Error> {
        let a = bodies.body(bearer).ok_or(Error::Unavailable)?;
        let b = bodies.body(passenger).ok_or(Error::Unavailable)?;
        if bearer == passenger
            || !a.can_bear
            || !a.position.same_point(b.position)
            || group(store, bearer).is_some()
            || self.busy(store, bearer)
            || self.busy(store, passenger)
        {
            return Err(Error::Unavailable);
        }
        let expires = SimTime(
            now.0
                .checked_add(self.rules.offer_lifetime.0)
                .ok_or(Error::Overflow)?,
        );
        let mut offers = offer_inbox(store, passenger).cloned().unwrap_or_default();
        offers.by_bearer.retain(|_, offer| offer.expires > now);
        if offers.by_bearer.len() >= self.rules.max_bearers
            && !offers.by_bearer.contains_key(&bearer)
        {
            return Err(Error::Unavailable);
        }
        offers.by_bearer.insert(
            bearer,
            Offer {
                bearer,
                passenger,
                offered_at: now,
                expires,
            },
        );
        let mut prepared = PreparedTransport::new(self, ());
        prepared.at = Some(now);
        prepared.offers.set(store, passenger, Some(offers));
        prepared.guard_topology(store, [bearer, passenger]);
        prepared.guard_bodies(bodies, [bearer, passenger])?;
        Ok(prepared)
    }
    pub fn offer(
        &mut self,
        store: &mut Store,
        bearer: PersonId,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<(), Error> {
        self.prepare_offer(store, bearer, passenger, now, bodies)?
            .commit(self, store, bodies, now)
    }
    pub fn prepare_begin_lift(
        &self,
        store: &Store,
        passenger: PersonId,
        bearers: BTreeSet<PersonId>,
        permission: Permission,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<PreparedTransport<Lift>, Error> {
        let authority = permission.authority()?;
        let completes = SimTime(
            now.0
                .checked_add(self.rules.lift_time.0)
                .ok_or(Error::Overflow)?,
        );
        let group = CarryGroup {
            passenger,
            anchor: *bearers.first().ok_or(Error::Unavailable)?,
            bearers: bearers.clone(),
            authority,
            accepted_at: completes,
        };
        self.check_group(store, &group, bodies, now, true)?;
        let lift = Lift {
            passenger,
            bearers,
            authority,
            started: now,
            completes,
        };
        let mut prepared = PreparedTransport::new(self, lift.clone());
        prepared.at = Some(now);
        prepared.lifts.set(store, passenger, Some(lift));
        prepared.groups.guard(store, passenger);
        prepared.offers.guard(store, passenger);
        let linked =
            prepared.guard_topology(store, group.bearers.iter().copied().chain([passenger]));
        prepared.guard_bodies(bodies, linked)?;
        Ok(prepared)
    }
    pub fn begin_lift(
        &mut self,
        store: &mut Store,
        passenger: PersonId,
        bearers: BTreeSet<PersonId>,
        permission: Permission,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<Lift, Error> {
        self.prepare_begin_lift(store, passenger, bearers, permission, now, bodies)?
            .commit(self, store, bodies, now)
    }
    pub fn prepare_complete_lift(
        &self,
        store: &Store,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<PreparedTransport<CarryGroup>, Error> {
        let lift = lift(store, passenger).ok_or(Error::Unavailable)?;
        if now != lift.completes {
            return Err(Error::Unavailable);
        }
        let group = CarryGroup {
            passenger,
            anchor: *lift.bearers.first().ok_or(Error::Unavailable)?,
            bearers: lift.bearers.clone(),
            authority: lift.authority,
            accepted_at: now,
        };
        self.check_group(store, &group, bodies, now, false)?;
        let mut prepared = PreparedTransport::new(self, group.clone());
        prepared.at = Some(now);
        prepared.lifts.set(store, passenger, None);
        prepared.offers.set(store, passenger, None);
        prepared.groups.set(store, passenger, Some(group.clone()));
        let linked =
            prepared.guard_topology(store, group.bearers.iter().copied().chain([passenger]));
        prepared.guard_bodies(bodies, linked)?;
        Ok(prepared)
    }
    pub fn complete_lift(
        &mut self,
        store: &mut Store,
        passenger: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<CarryGroup, Error> {
        self.prepare_complete_lift(store, passenger, now, bodies)?
            .commit(self, store, bodies, now)
    }
    pub(crate) fn check_group(
        &self,
        store: &Store,
        candidate: &CarryGroup,
        bodies: &impl BodyFacts,
        now: SimTime,
        require_offers: bool,
    ) -> Result<(), Error> {
        let passenger = bodies.body(candidate.passenger).ok_or(Error::Unavailable)?;
        if candidate.bearers.is_empty()
            || candidate.bearers.len() > self.rules.max_bearers
            || candidate.bearers.contains(&candidate.passenger)
            || candidate
                .bearers
                .iter()
                .any(|id| group(store, *id).is_some())
            || require_offers
                && (self.busy(store, candidate.passenger)
                    || candidate.bearers.iter().any(|id| self.busy(store, *id)))
        {
            return Err(Error::Unavailable);
        }
        for bearer in &candidate.bearers {
            let body = bodies.body(*bearer).ok_or(Error::Unavailable)?;
            if !body.can_bear || !body.position.same_point(passenger.position) {
                return Err(Error::Unavailable);
            }
            if require_offers
                && !group(store, candidate.passenger)
                    .is_some_and(|row| row.bearers.contains(bearer))
                && !offer_inbox(store, candidate.passenger)
                    .and_then(|row| row.by_bearer.get(bearer))
                    .is_some_and(|offer| offer.expires > now)
            {
                return Err(Error::Unavailable);
            }
        }
        if let Some(existing) = group(store, candidate.passenger) {
            for bearer in existing.bearers.difference(&candidate.bearers) {
                if bodies.body(*bearer).is_some_and(|body| body.can_hold) {
                    return Err(Error::Unavailable);
                }
            }
        }
        let mut rows = groups(store)
            .filter(|(id, _)| *id != candidate.passenger)
            .map(|(_, row)| row)
            .collect::<Vec<_>>();
        rows.push(candidate);
        rows.sort_by_key(|row| row.passenger);
        let mut linked = candidate.bearers.clone();
        linked.insert(candidate.passenger);
        loop {
            let size = linked.len();
            for row in &rows {
                if linked.contains(&row.passenger) || !row.bearers.is_disjoint(&linked) {
                    linked.insert(row.passenger);
                    linked.extend(row.bearers.iter().copied());
                }
            }
            if size == linked.len() {
                break;
            }
        }
        rows.retain(|row| linked.contains(&row.passenger) || !row.bearers.is_disjoint(&linked));
        let plan = capacity_plan_rows(rows, bodies, None)?;
        if !plan.overloaded.is_disjoint(&linked) || !plan.unable.is_disjoint(&linked) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn prepare_newborn(
        &self,
        store: &Store,
        child: PersonId,
        gestator: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<PreparedTransport<CarryGroup>, Error> {
        let child_body = bodies.body(child).ok_or(Error::Unavailable)?;
        let mother = bodies.body(gestator).ok_or(Error::Unavailable)?;
        if child == gestator
            || !child_body.position.same_point(mother.position)
            || group(store, child).is_some()
        {
            return Err(Error::Unavailable);
        }
        let bearers = group(store, gestator)
            .map_or_else(|| BTreeSet::from([gestator]), |row| row.bearers.clone());
        let group = CarryGroup {
            passenger: child,
            anchor: *bearers.first().ok_or(Error::Unavailable)?,
            bearers,
            authority: Authority::Newborn,
            accepted_at: now,
        };
        let mut prepared = PreparedTransport::new(self, group.clone());
        prepared.at = Some(now);
        prepared.groups.set(store, child, Some(group));
        prepared.groups.guard(store, gestator);
        prepared.guard_topology(store, [child, gestator]);
        prepared.guard_bodies(bodies, [child, gestator])?;
        Ok(prepared)
    }
    pub fn attach_newborn(
        &mut self,
        store: &mut Store,
        child: PersonId,
        gestator: PersonId,
        now: SimTime,
        bodies: &impl BodyFacts,
    ) -> Result<CarryGroup, Error> {
        self.prepare_newborn(store, child, gestator, now, bodies)?
            .commit(self, store, bodies, now)
    }
    pub fn prepare_remove_group(
        &self,
        store: &Store,
        passenger: PersonId,
    ) -> PreparedTransport<Option<CarryGroup>> {
        let before = group(store, passenger).cloned();
        let mut prepared = PreparedTransport::new(self, before);
        prepared.groups.set(store, passenger, None);
        prepared.guard_topology(store, [passenger]);
        prepared
    }
    pub fn remove_group(
        &mut self,
        store: &mut Store,
        passenger: PersonId,
    ) -> Result<Option<CarryGroup>, Error> {
        self.prepare_remove_group(store, passenger).commit(
            self,
            store,
            &BTreeMap::<PersonId, Body>::new(),
            SimTime(0),
        )
    }
    pub fn prepare_cancel_lifts(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedTransport<Vec<Lift>> {
        let ids = actors.iter().copied().collect::<BTreeSet<_>>();
        let rows = lifts(store)
            .filter(|(_, row)| ids.contains(&row.passenger) || !ids.is_disjoint(&row.bearers))
            .map(|(_, row)| row.clone())
            .collect::<Vec<_>>();
        let mut prepared = PreparedTransport::new(self, rows.clone());
        for row in rows {
            prepared.lifts.set(store, row.passenger, None);
        }
        prepared.guard_topology(store, ids);
        prepared
    }
    pub fn cancel_lifts(&mut self, store: &mut Store, actor: PersonId) -> Result<Vec<Lift>, Error> {
        self.prepare_cancel_lifts(store, &[actor]).commit(
            self,
            store,
            &BTreeMap::<PersonId, Body>::new(),
            SimTime(0),
        )
    }
    pub fn prepare_prune_offers(&self, store: &Store, now: SimTime) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        prepared.at = Some(now);
        prepared.expired_inboxes = Some((
            now,
            offer_inboxes(store)
                .filter(|(_, inbox)| inbox.by_bearer.values().any(|offer| offer.expires <= now))
                .map(|(actor, row)| (actor, row.clone()))
                .collect(),
        ));
        for (passenger, before) in offer_inboxes(store) {
            let mut after = before.clone();
            after.by_bearer.retain(|_, offer| offer.expires > now);
            if after != *before {
                let after = (!after.by_bearer.is_empty()).then_some(after);
                prepared.offers.set(store, passenger, after);
            }
        }
        prepared
    }
    pub fn prune_offers(&mut self, store: &mut Store, now: SimTime) -> Result<(), Error> {
        self.prepare_prune_offers(store, now).commit(
            self,
            store,
            &BTreeMap::<PersonId, Body>::new(),
            now,
        )
    }
    pub fn prepare_observe_support(
        &self,
        store: &Store,
        request: &SupportRequest,
        present: impl IntoIterator<Item = PersonId>,
    ) -> Result<PreparedTransport<()>, Error> {
        request.validate()?;
        let mut prepared = PreparedTransport::new(self, ());
        for observer in support_observers(request.passenger, present) {
            let mut known = prepared
                .knowledge
                .edits
                .get(&observer)
                .and_then(|(_, row)| row.clone())
                .or_else(|| support_knowledge(store, observer).cloned())
                .unwrap_or_default();
            if known.by_passenger.len() < 64 || known.by_passenger.contains_key(&request.passenger)
            {
                known
                    .by_passenger
                    .insert(request.passenger, request.clone());
                prepared.knowledge.set(store, observer, Some(known));
            }
        }
        Ok(prepared)
    }
    pub fn observe_support(
        &mut self,
        store: &mut Store,
        request: &SupportRequest,
        present: impl IntoIterator<Item = PersonId>,
    ) -> Result<(), Error> {
        self.prepare_observe_support(store, request, present)?
            .commit(self, store, &BTreeMap::<PersonId, Body>::new(), SimTime(0))
    }
    pub fn prepare_review(
        &self,
        store: &Store,
        response: Response,
        support: Option<&SupportRequest>,
        present: impl IntoIterator<Item = PersonId>,
    ) -> Result<PreparedTransport<()>, Error> {
        response.validate()?;
        let mut prepared = if let Some(support) = support {
            self.prepare_observe_support(store, support, present)?
        } else {
            PreparedTransport::new(self, ())
        };
        let mut after = self.responses.clone();
        if !after.iter().rev().take(64).any(|known| {
            known.passenger == response.passenger
                && known.bearer == response.bearer
                && known.kind == response.kind
        }) {
            after.push(response);
            if after.len() > 64 {
                after.remove(0);
            }
        }
        prepared.journal = Some((self.responses.clone(), after));
        Ok(prepared)
    }
    pub fn record_response(&mut self, store: &mut Store, response: Response) -> Result<(), Error> {
        self.prepare_review(store, response, None, [])?.commit(
            self,
            store,
            &BTreeMap::<PersonId, Body>::new(),
            SimTime(0),
        )
    }
    pub fn prepare_approach(
        &self,
        store: &Store,
        actor: PersonId,
        after: Option<Approach>,
    ) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        prepared.approaches.set(store, actor, after);
        prepared
    }
    pub fn prepare_resume_target(
        &self,
        store: &Store,
        actor: PersonId,
        after: Option<AreaId>,
    ) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        prepared
            .resume
            .set(store, actor, after.map(|area| ResumeTarget { area }));
        prepared
    }
    pub fn prepare_paused_approach(
        &self,
        store: &Store,
        actor: PersonId,
        after: Option<RoadPosition>,
    ) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        prepared
            .paused
            .set(store, actor, after.map(|target| PausedApproach { target }));
        prepared
    }
}
impl Runtime {
    /// Pause the selected actors' existing directional intent and approach target.
    pub fn prepare_pause_routes(
        &self,
        store: &Store,
        actors: &[PersonId],
        destination: AreaId,
    ) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        for actor in actors {
            prepared
                .resume
                .set(store, *actor, Some(ResumeTarget { area: destination }));
            prepared.approaches.guard(store, *actor);
            if let Some(approach) = approach_job(store, *actor) {
                prepared.approaches.set(store, *actor, None);
                prepared.paused.set(
                    store,
                    *actor,
                    Some(PausedApproach {
                        target: approach.target,
                    }),
                );
            }
        }
        prepared
    }
    /// Departure consumes paused intent only after the physical owner admits it.
    pub fn prepare_departure(&self, store: &Store, actors: &[PersonId]) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        for actor in actors {
            prepared.resume.set(store, *actor, None);
            prepared.paused.set(store, *actor, None);
            prepared.approaches.set(store, *actor, None);
        }
        prepared
    }
    pub fn prepare_finish_approach(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedTransport<Approach>, Error> {
        let row = approach_job(store, actor).ok_or(Error::Unavailable)?;
        if row.completes != now {
            return Err(Error::Unavailable);
        }
        let mut prepared = PreparedTransport::new(self, row.clone());
        prepared.at = Some(now);
        prepared.approaches.set(store, actor, None);
        Ok(prepared)
    }
}
impl Runtime {
    /// Load budgets are independent across disconnected carrying components.
    pub fn component_plan(
        &self,
        store: &Store,
        bodies: &impl BodyFacts,
        actor: PersonId,
    ) -> Result<CapacityPlan, Error> {
        let linked = self.component(store, actor);
        capacity_plan_rows(
            groups(store)
                .filter(|(_, row)| {
                    linked.contains(&row.passenger) || !row.bearers.is_disjoint(&linked)
                })
                .map(|(_, row)| row),
            bodies,
            None,
        )
    }
}
impl Runtime {
    pub fn prepare_forget_routes(
        &self,
        store: &Store,
        stopped: &BTreeSet<PersonId>,
    ) -> PreparedTransport<()> {
        let mut prepared = PreparedTransport::new(self, ());
        for (actor, _) in resume_targets(store).filter(|(actor, _)| !stopped.contains(actor)) {
            prepared.resume.set(store, actor, None);
        }
        for (actor, _) in paused_approaches(store).filter(|(actor, _)| !stopped.contains(actor)) {
            prepared.paused.set(store, actor, None);
        }
        prepared
    }
    pub fn prepare_refusal(
        &self,
        store: &Store,
        passenger: PersonId,
        bearer: PersonId,
    ) -> Result<PreparedTransport<()>, Error> {
        let mut after = offer_inbox(store, passenger)
            .ok_or(Error::Unavailable)?
            .clone();
        if after.by_bearer.remove(&bearer).is_none() {
            return Err(Error::Unavailable);
        }
        let mut prepared = PreparedTransport::new(self, ());
        prepared.offers.set(
            store,
            passenger,
            (!after.by_bearer.is_empty()).then_some(after),
        );
        Ok(prepared)
    }
}

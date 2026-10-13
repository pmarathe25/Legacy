//! Sparse owner transitions over canonical History and Guardian components.
use crate::canonical::{Projection, configuration};
use crate::*;
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Succession {
    Unchanged,
    Died {
        predecessor: PersonId,
    },
    Bound {
        predecessor: PersonId,
        successor: PersonId,
    },
}
impl Succession {
    /// Control-role reactions leave possessions and political offices untouched.
    pub fn role_changes(self) -> Vec<(PersonId, Role)> {
        match self {
            Self::Bound {
                predecessor,
                successor,
            } => vec![(predecessor, Role::Citizen), (successor, Role::Ruler)],
            _ => Vec::new(),
        }
    }
}

pub struct PreparedLineage<Output> {
    runtime: Runtime,
    reads: ReadSet,
    history: Option<History>,
    guardians: Vec<(PersonId, Guardian)>,
    learning: Vec<(PersonId, Option<InvocationLearning>)>,
    reserved_successors: Vec<StableId>,
    output: Output,
}
pub struct StagedLineage<Output> {
    runtime: Runtime,
    guard: ReadGuard,
    reserved_successors: Vec<StableId>,
    output: Output,
}
impl<Output> PreparedLineage<Output> {
    pub fn output(&self) -> &Output {
        &self.output
    }
    /// Factual adapters can retain the other owners' borrowed input witnesses.
    /// Their reads join this effect; selection and updates remain with Lineage.
    pub fn with_factual_reads(mut self, reads: ReadSet) -> Self {
        self.reads.extend(reads);
        self
    }
    /// Fold conduct evidence into this owner's pending care, learning, or history
    /// effects before either effect is staged. Only Lineage's own projection is
    /// copied; canonical rows and the original result remain unpublished.
    #[allow(
        clippy::too_many_arguments,
        reason = "pending owner effect and observation facts are distinct inputs"
    )]
    pub fn with_witness(
        self,
        runtime: &Runtime,
        store: &Store,
        event: WitnessEvent,
        current_location: &Location,
        stage: LifeStage,
        now: SimTime,
        observers: impl IntoIterator<Item = WitnessFacts>,
    ) -> Result<Self, Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let current = self
            .history
            .as_ref()
            .map_or_else(|| runtime.current(store), History::current);
        let area = if current == Some(event.actor) {
            event.departure.or(match current_location {
                Location::AtArea(area) => Some(*area),
                _ => None,
            })
        } else {
            None
        };
        let observers = observers.into_iter().collect::<Vec<_>>();
        let mut actors = self
            .guardians
            .iter()
            .filter_map(|(actor, _)| guardian(store, *actor).is_some().then_some(*actor))
            .collect::<std::collections::BTreeSet<_>>();
        actors.extend(
            observers
                .iter()
                .filter(|facts| {
                    facts.health > 0
                        && area.is_some_and(|area| facts.location == Location::AtArea(area))
                        && guardian(store, facts.actor).is_some()
                })
                .map(|facts| facts.actor),
        );
        let mut before = runtime.project(store, &actors.into_iter().collect::<Vec<_>>())?;
        if area.is_some() {
            before.reads.membership::<Guardian>(store, |_, _| true);
        }
        let mut after = before.world.clone();
        if let Some(history) = self.history {
            after.waiting = history.waiting();
            after.incarnations = history.incarnations;
        }
        for (actor, guardian) in self.guardians {
            after.guardians.insert(actor, guardian);
        }
        crate::gameplay::apply_witness(&mut after, event, current_location, stage, now, observers)?;
        let mut combined = runtime.change(before, after, self.output)?;
        combined.reads.extend(self.reads);
        combined.learning = self.learning;
        combined.reserved_successors = self.reserved_successors;
        Ok(combined)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedLineage<Output>, Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        if let Some(history) = self.history {
            if batch.touches::<History>(runtime.history) {
                return Err(Error::Unavailable);
            }
            batch.insert(runtime.history, history);
            admitted.protect::<History>(runtime.history);
        }
        for (actor, guardian) in self.guardians {
            let id = StableId(actor.0);
            if batch.touches::<Guardian>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, guardian);
            admitted.protect::<Guardian>(id);
        }
        for (actor, learning) in self.learning {
            let id = StableId(actor.0);
            if batch.touches::<InvocationLearning>(id) {
                return Err(Error::Unavailable);
            }
            if let Some(learning) = learning {
                batch.insert(id, learning);
            } else {
                batch.remove::<InvocationLearning>(id);
            }
            admitted.protect::<InvocationLearning>(id);
        }
        Ok(StagedLineage {
            runtime: self.runtime,
            guard: self.reads.bind(store, batch, &admitted)?,
            reserved_successors: self.reserved_successors,
            output: self.output,
        })
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<Output, Error> {
        let mut batch = Batch::new();
        let staged = self.stage_into(runtime, store, &mut batch)?;
        staged.validate(runtime, store, &batch)?;
        store.apply_batch(batch)?;
        Ok(staged.finish())
    }
}
impl<Output> StagedLineage<Output> {
    pub fn guard(&self) -> &ReadGuard {
        &self.guard
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        // Birth creators own the new person/body rows and their exact guards.
        // A reserved binding cannot publish on its own or accept stale occupancy.
        if self.reserved_successors.iter().any(|id| {
            store.contains(*id) || !batch.inserted_entities().any(|created| created == *id)
        }) {
            return Err(Error::Unavailable);
        }
        self.guard.validate(store, batch)
    }
    /// Returns already prepared effects after canonical admission; no state is
    /// retained in Runtime and no further rule evaluation or mutation occurs.
    pub fn finish(self) -> Output {
        self.output
    }
}

impl Runtime {
    pub fn prepare_request_success(
        &self,
        store: &Store,
        actor: PersonId,
        request: GuardianRequest,
        now: SimTime,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        guardian.requested_delivery = None;
        guardian.assignment = Some(Assignment {
            request,
            accepted_at: now,
        });
        guardian.explanation =
            "I accept this physical task while retaining my judgment and lineage duty".into();
        self.change(before, after, ())
    }
    pub(crate) fn change<Output>(
        &self,
        before: Projection,
        after: World,
        output: Output,
    ) -> Result<PreparedLineage<Output>, Error> {
        let history = History {
            incarnations: after.incarnations,
        };
        history.validate()?;
        let changed_history =
            (history.incarnations != before.world.incarnations).then_some(history);
        let mut changed_guardians = Vec::new();
        for (actor, guardian) in after.guardians {
            guardian.validate()?;
            if before.world.guardians.get(&actor) != Some(&guardian) {
                changed_guardians.push((actor, guardian));
            }
        }
        Ok(PreparedLineage {
            runtime: self.clone(),
            reads: before.reads,
            history: changed_history,
            guardians: changed_guardians,
            learning: Vec::new(),
            reserved_successors: Vec::new(),
            output,
        })
    }
    pub fn prepare_death(
        &self,
        store: &Store,
        current_alive: bool,
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
    ) -> Result<PreparedLineage<Succession>, Error> {
        let before = self.project_all(store)?;
        let after = observe_death(&before.world, current_alive, now, constraints);
        let output = match (before.world.current(), after.current()) {
            (Some(predecessor), None) => Succession::Died { predecessor },
            _ => Succession::Unchanged,
        };
        self.change(before, after, output)
    }
    pub fn prepare_birth(
        &self,
        store: &Store,
        births: &[ActualBirth],
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
    ) -> Result<PreparedLineage<Succession>, Error> {
        self.prepare_birth_binding(store, births, now, constraints, false)
    }
    /// Bind an actual prepared newborn before its creators publish their rows.
    /// Stage the person/body creators in the same reserved host transaction,
    /// then validate this typed receipt before admission.
    pub fn prepare_reserved_birth(
        &self,
        store: &Store,
        births: &[ActualBirth],
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
    ) -> Result<PreparedLineage<Succession>, Error> {
        self.prepare_birth_binding(store, births, now, constraints, true)
    }
    fn prepare_birth_binding(
        &self,
        store: &Store,
        births: &[ActualBirth],
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
        reserved: bool,
    ) -> Result<PreparedLineage<Succession>, Error> {
        let history_only = self.project(store, &[])?;
        if history_only.world.waiting.is_none() {
            let after = history_only.world.clone();
            return self.change(history_only, after, Succession::Unchanged);
        }
        let before = self.project_all(store)?;
        if let Some(binding) = bind_actual_birth(&before.world, births, now, constraints) {
            let successor = StableId(binding.successor.0);
            let exists = store.contains(successor);
            if !exists && !reserved {
                return Err(Error::Unavailable);
            }
            let mut before = before;
            if exists {
                before.reads.entity(store, successor);
            }
            let mut prepared = self.change(
                before,
                binding.lineage,
                Succession::Bound {
                    predecessor: binding.predecessor,
                    successor: binding.successor,
                },
            )?;
            if !exists {
                prepared.reserved_successors.push(successor);
            }
            Ok(prepared)
        } else {
            let after = before.world.clone();
            self.change(before, after, Succession::Unchanged)
        }
    }
    pub fn prepare_conduct(
        &self,
        store: &Store,
        actor: PersonId,
        traits: GuardianTraits,
        observation: Observation,
    ) -> Result<PreparedLineage<()>, Error> {
        valid_traits(traits)?;
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        if observation.witnessed
            && let Some(target) = after.current()
        {
            let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
            let opening = guardian
                .assessments
                .get(&target)
                .cloned()
                .unwrap_or_default();
            let observed = observe_conduct(&opening, traits, observation);
            if observed != opening {
                guardian.assessments.insert(target, observed);
            }
        }
        self.change(before, after, ())
    }
    pub fn prepare_stage_observation(
        &self,
        store: &Store,
        actor: PersonId,
        traits: GuardianTraits,
        stage: LifeStage,
        now: SimTime,
        witnessed: bool,
    ) -> Result<PreparedLineage<()>, Error> {
        valid_traits(traits)?;
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        if witnessed && let Some(target) = after.current() {
            let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
            let opening = guardian
                .assessments
                .get(&target)
                .cloned()
                .unwrap_or_default();
            guardian
                .assessments
                .insert(target, observe_stage(&opening, traits, stage, now, true));
        }
        self.change(before, after, ())
    }
    pub fn prepare_failed_attempt(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        after
            .guardians
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .observe_failed_attempt();
        self.change(before, after, ())
    }
    pub fn prepare_care_provision(
        &self,
        store: &Store,
        actor: PersonId,
        facts: ProvisionFacts<'_>,
    ) -> Result<PreparedLineage<Option<CareProvision>>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let target = after.current().ok_or(Error::Unavailable)?;
        if facts.applicable == Some(false) {
            after.observe_care_unavailable(actor);
        }
        let guardian = after.guardians.get_mut(&actor).ok_or(Error::Unavailable)?;
        let provision = review_care_provision(
            guardian.provision.as_ref(),
            target,
            facts.now,
            facts.together,
            facts.need.filter(|_| facts.applicable != Some(false)),
            facts.doses,
        );
        guardian.provision = provision.clone();
        self.change(before, after, provision)
    }
    pub fn prepare_care_receipt(
        &self,
        store: &Store,
        actor: PersonId,
        target: PersonId,
        good: Good,
        quantity: Quantity,
        sequence: u64,
    ) -> Result<PreparedLineage<bool>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let credited = after.record_care_receipt(actor, target, good, quantity, sequence)?;
        self.change(before, after, credited)
    }
    pub fn prepare_supply_receipts(
        &self,
        store: &Store,
        actor: PersonId,
        receipts: &[(EventId, SupplyReceipt)],
        through: u64,
    ) -> Result<PreparedLineage<Quantity>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        let mut credited = 0_u64;
        if let Some(request) = after
            .guardians
            .get_mut(&actor)
            .and_then(|guardian| guardian.requested_delivery.as_mut())
        {
            if through < request.last_event || receipts.iter().any(|(id, _)| id.0 > through) {
                return Err(Error::Unavailable);
            }
            let mut ordered: Vec<_> = receipts.iter().collect();
            ordered.sort_by_key(|(id, _)| *id);
            for (id, receipt) in ordered {
                credited = credited
                    .checked_add(request.receive(actor, *id, receipt)?.0)
                    .ok_or(Error::Overflow)?;
            }
            request.last_event = through;
        }
        self.change(before, after, Quantity(credited))
    }
    pub fn prepare_cancel_delivery(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Result<PreparedLineage<()>, Error> {
        let before = self.project(store, &[actor])?;
        let mut after = before.world.clone();
        after
            .guardians
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .requested_delivery = None;
        self.change(before, after, ())
    }
    pub fn prepare_learning(
        &self,
        store: &Store,
        person: PersonId,
        amount: u64,
    ) -> Result<PreparedLineage<u64>, Error> {
        let before = self.project(store, &[])?;
        let after = before.world.clone();
        let mut change = self.change(before, after, 0)?;
        if !change.reads.entity(store, StableId(person.0)) {
            return Err(Error::Unavailable);
        }
        let opening = change
            .reads
            .read::<InvocationLearning>(store, StableId(person.0));
        let learned = opening.map_or(0, |row| row.learned);
        if learned > INVOCATION_LEARNING_LIMIT {
            return Err(configuration("invalid invocation learning"));
        }
        let credited = amount.min(INVOCATION_LEARNING_LIMIT - learned);
        if credited > 0 {
            change.learning.push((
                person,
                Some(InvocationLearning {
                    learned: learned + credited,
                }),
            ));
        }
        change.output = credited;
        Ok(change)
    }
}
pub(crate) fn valid_traits(traits: GuardianTraits) -> Result<(), Error> {
    if [traits.discipline, traits.patience, traits.risk_tolerance]
        .into_iter()
        .any(|value| value > 1_000)
    {
        return Err(configuration("invalid guardian traits"));
    }
    Ok(())
}
pub struct ProvisionFacts<'a> {
    pub now: SimTime,
    pub together: bool,
    pub applicable: Option<bool>,
    pub need: Option<&'a NeedObservation>,
    pub doses: CareDoses,
}

impl<Output> PreparedLineage<Output> {
    /// Apply mortality after this owner's pending evidence, before publishing
    /// either effect. The existing death policy receives the projected rows.
    pub fn with_death(
        self,
        runtime: &Runtime,
        store: &Store,
        current_alive: bool,
        now: SimTime,
        constraints: &BTreeMap<PersonId, Option<GuardianGoal>>,
    ) -> Result<PreparedLineage<Succession>, Error> {
        if runtime != &self.runtime {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let before = runtime.project_all(store)?;
        let mut pending = before.world.clone();
        if let Some(history) = self.history {
            pending.waiting = history.waiting();
            pending.incarnations = history.incarnations;
        }
        for (actor, guardian) in self.guardians {
            pending.guardians.insert(actor, guardian);
        }
        let after = observe_death(&pending, current_alive, now, constraints);
        let output = match (pending.current(), after.current()) {
            (Some(predecessor), None) => Succession::Died { predecessor },
            _ => Succession::Unchanged,
        };
        let mut combined = runtime.change(before, after, output)?;
        combined.reads.extend(self.reads);
        combined.learning = self.learning;
        combined.reserved_successors = self.reserved_successors;
        Ok(combined)
    }
}

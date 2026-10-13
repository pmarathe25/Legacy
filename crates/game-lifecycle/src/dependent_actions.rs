//! Dependent action admission and receiving preferences and caregiving effects.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependentAction {
    Signal(ChildhoodInterest),
    Play(ChildhoodInterest),
    Comfort(PersonId),
    Care(PersonId),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DependentStartFacts {
    pub fatigue_available: bool,
    pub child_alive: bool,
    pub same_place: bool,
    pub child_receiving_care: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependentStart {
    pub action: DependentAction,
    pub completes: SimTime,
}
pub struct PreparedDependentStart {
    rules: Rules,
    start: DependentStart,
    reads: ReadSet,
}
impl Runtime {
    pub fn prepare_dependent_start(
        &self,
        store: &Store,
        actor: PersonId,
        action: DependentAction,
        now: SimTime,
        facts: DependentStartFacts,
        care_duration: SimDuration,
    ) -> Result<PreparedDependentStart, Error> {
        if !self.rules.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid dependent action biology".into(),
            ));
        }
        let mut reads = ReadSet::new();
        let duration = match action {
            DependentAction::Signal(preference) => {
                let life = reads
                    .read::<Life>(store, StableId(actor.0))
                    .ok_or(Error::Unavailable)?;
                if !dependent(Some(life.stage))
                    || !matches!(
                        preference,
                        ChildhoodInterest::Food | ChildhoodInterest::Company
                    )
                {
                    return Err(Error::Unavailable);
                }
                6_000
            }
            DependentAction::Play(_) => {
                if reads
                    .read::<Life>(store, StableId(actor.0))
                    .is_none_or(|life| life.stage != LifeStage::Child)
                    || !facts.fatigue_available
                {
                    return Err(Error::Unavailable);
                }
                MINUTE.0
            }
            DependentAction::Comfort(child) => {
                if reads
                    .read::<Life>(store, StableId(child.0))
                    .is_none_or(|life| !dependent(Some(life.stage)))
                    || child == actor
                    || !facts.child_alive
                    || !facts.same_place
                    || facts.child_receiving_care
                {
                    return Err(Error::Unavailable);
                }
                MINUTE.0
            }
            DependentAction::Care(child) => {
                if reads
                    .read::<Life>(store, StableId(child.0))
                    .is_none_or(|life| !dependent(Some(life.stage)))
                    || child == actor
                    || care_duration.0 == 0
                {
                    return Err(Error::Unavailable);
                }
                care_duration.0
            }
        };
        let completes = SimTime(now.0.checked_add(duration).ok_or(Error::Overflow)?);
        if matches!(action, DependentAction::Signal(_)) {
            completes.0.checked_add(300_000).ok_or(Error::Overflow)?;
        }
        Ok(PreparedDependentStart {
            rules: self.rules.clone(),
            start: DependentStart { action, completes },
            reads,
        })
    }
    pub fn prepare_dependent_reaction(
        &self,
        store: &Store,
        actor: PersonId,
        action: DependentAction,
        now: SimTime,
        child_alive: bool,
        same_place: bool,
    ) -> Result<Option<PreparedDependentReaction>, Error> {
        if !self.rules.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid dependent action biology".into(),
            ));
        }
        let receiver = match action {
            DependentAction::Comfort(child) | DependentAction::Care(child) => child,
            _ => actor,
        };
        if matches!(
            action,
            DependentAction::Comfort(_) | DependentAction::Care(_)
        ) && (!child_alive || !same_place)
        {
            return Ok(None);
        }
        let mut reads = ReadSet::new();
        let before = reads
            .read::<Life>(store, StableId(receiver.0))
            .ok_or(Error::Unavailable)?
            .clone();
        let mut after = before;
        match action {
            DependentAction::Signal(preference) => {
                after.preference = Some((
                    preference,
                    SimTime(now.0.checked_add(300_000).ok_or(Error::Overflow)?),
                ))
            }
            DependentAction::Comfort(_) => {
                if after
                    .preference
                    .is_some_and(|(preference, _)| preference == ChildhoodInterest::Company)
                {
                    after.preference = None;
                }
            }
            DependentAction::Care(_) => {
                reads
                    .read::<Life>(store, StableId(actor.0))
                    .ok_or(Error::Unavailable)?;
                after.caregivers.insert(actor);
                if after
                    .preference
                    .is_some_and(|(preference, _)| preference == ChildhoodInterest::Food)
                {
                    after.preference = None;
                }
            }
            DependentAction::Play(_) => {}
        }
        after.validate()?;
        Ok(Some(PreparedDependentReaction {
            rules: self.rules.clone(),
            receiver,
            after,
            reads,
        }))
    }
}
impl PreparedDependentStart {
    pub fn start(&self) -> DependentStart {
        self.start
    }
    pub fn with_factual_reads(mut self, reads: ReadSet) -> Self {
        self.reads.extend(reads);
        self
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<ReadGuard, Error> {
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        self.reads.bind(store, batch, &ReadFootprint::new())
    }
}
pub struct PreparedDependentReaction {
    rules: Rules,
    receiver: PersonId,
    after: Life,
    reads: ReadSet,
}
impl PreparedDependentReaction {
    pub fn with_factual_reads(mut self, reads: ReadSet) -> Self {
        self.reads.extend(reads);
        self
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let id = StableId(self.receiver.0);
        if batch.touches::<Life>(id) {
            return Err(Error::Unavailable);
        }
        batch.insert(id, self.after);
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Life>(id);
        self.reads.bind(store, batch, &admitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Runtime, Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let runtime = Runtime {
            rules: Rules::human(),
        };
        for actor in [PersonId(1), PersonId(2)] {
            insert_life(
                &mut store,
                Life {
                    id: actor,
                    born: SimTime(0),
                    initial_age: runtime.rules.infancy,
                    stage: LifeStage::Child,
                    reproduction: Reproduction::Neither,
                    parents: vec![],
                    caregivers: BTreeSet::new(),
                    preference: None,
                },
            )
            .unwrap();
        }
        (runtime, store)
    }
    fn facts() -> DependentStartFacts {
        DependentStartFacts {
            fatigue_available: true,
            child_alive: true,
            same_place: true,
            child_receiving_care: false,
        }
    }
    #[test]
    fn production_admission_selects_dependent_ages_preferences_and_authored_durations() {
        let (runtime, store) = fixture();
        let signal = runtime
            .prepare_dependent_start(
                &store,
                PersonId(1),
                DependentAction::Signal(ChildhoodInterest::Food),
                SimTime(10),
                facts(),
                SimDuration(123),
            )
            .unwrap();
        assert_eq!(signal.start().completes, SimTime(6_010));
        assert!(
            runtime
                .prepare_dependent_start(
                    &store,
                    PersonId(1),
                    DependentAction::Signal(ChildhoodInterest::Craft),
                    SimTime(10),
                    facts(),
                    SimDuration(123)
                )
                .is_err()
        );
        let play = runtime
            .prepare_dependent_start(
                &store,
                PersonId(1),
                DependentAction::Play(ChildhoodInterest::Craft),
                SimTime(10),
                facts(),
                SimDuration(123),
            )
            .unwrap();
        assert_eq!(play.start().completes, SimTime(MINUTE.0 + 10));
        assert!(
            runtime
                .prepare_dependent_start(
                    &store,
                    PersonId(1),
                    DependentAction::Play(ChildhoodInterest::Craft),
                    SimTime(10),
                    DependentStartFacts {
                        fatigue_available: false,
                        ..facts()
                    },
                    SimDuration(123)
                )
                .is_err()
        );
        let care = runtime
            .prepare_dependent_start(
                &store,
                PersonId(2),
                DependentAction::Care(PersonId(1)),
                SimTime(10),
                facts(),
                SimDuration(123),
            )
            .unwrap();
        assert_eq!(care.start().completes, SimTime(133));
        assert!(
            runtime
                .prepare_dependent_start(
                    &store,
                    PersonId(2),
                    DependentAction::Comfort(PersonId(1)),
                    SimTime(10),
                    DependentStartFacts {
                        child_receiving_care: true,
                        ..facts()
                    },
                    SimDuration(123)
                )
                .is_err()
        );
        assert!(
            runtime
                .prepare_dependent_start(
                    &store,
                    PersonId(1),
                    DependentAction::Signal(ChildhoodInterest::Company),
                    SimTime(u64::MAX - 6_000),
                    facts(),
                    SimDuration(123)
                )
                .is_err()
        );
    }
    #[test]
    fn preference_and_care_receivers_guard_their_exact_lifecycle_rows_before_publication() {
        let (runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_dependent_reaction(
                &store,
                PersonId(1),
                DependentAction::Signal(ChildhoodInterest::Food),
                SimTime(10),
                true,
                true,
            )
            .unwrap()
            .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(
            life(&store, PersonId(1)).unwrap().preference,
            Some((ChildhoodInterest::Food, SimTime(300_010)))
        );
        assert!(
            runtime
                .prepare_dependent_reaction(
                    &store,
                    PersonId(2),
                    DependentAction::Care(PersonId(1)),
                    SimTime(20),
                    false,
                    true
                )
                .unwrap()
                .is_none()
        );
        let care = runtime
            .prepare_dependent_reaction(
                &store,
                PersonId(2),
                DependentAction::Care(PersonId(1)),
                SimTime(20),
                true,
                true,
            )
            .unwrap()
            .unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let guard = care.stage_into(&runtime, &store, &mut batch).unwrap();
        batch.remove::<Life>(StableId(1));
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
    }
}

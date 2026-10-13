//! Owner selection and authored prenatal history admission.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId};

pub fn independent(stage: Option<LifeStage>, lifecycle_enabled: bool) -> bool {
    !lifecycle_enabled || stage.is_some_and(|stage| stage != LifeStage::Infant)
}
pub fn dependent(stage: Option<LifeStage>) -> bool {
    matches!(stage, Some(LifeStage::Infant | LifeStage::Child))
}
pub fn adequate_food(
    facts: impl IntoIterator<Item = (game_core::Good, bool, game_core::Quantity)>,
) -> bool {
    facts.into_iter().any(|(good, accessible, quantity)| {
        good == game_core::Good::Food && accessible && quantity.0 >= 3_000
    })
}
#[derive(Clone, Copy, Debug)]
pub struct ParentCandidateFacts {
    pub choice: ParentChoiceInput,
    pub family_intent: bool,
}
pub fn chooses_parenthood(facts: ParentCandidateFacts) -> bool {
    facts.family_intent && willing_parent(facts.choice)
}
/// Re-evaluate both people's current choice when a selected pair is reached.
/// Earlier accepted attempts may have changed their availability since selection.
pub fn current_conception_candidate(
    pair: (PersonId, PersonId),
    mut facts: impl FnMut(PersonId, PersonId) -> Option<ParentCandidateFacts>,
) -> Option<(PersonId, PersonId)> {
    let (actor, partner) = pair;
    (actor != partner
        && facts(actor, partner).is_some_and(chooses_parenthood)
        && facts(partner, actor).is_some_and(chooses_parenthood))
    .then_some(pair)
}
/// The owner canonicalizes candidate pairs and applies current bilateral choice.
pub fn conception_candidates(
    relationships: impl IntoIterator<Item = (PersonId, PersonId)>,
    excluded: PersonId,
    mut facts: impl FnMut(PersonId, PersonId) -> Option<ParentCandidateFacts>,
) -> Vec<(PersonId, PersonId)> {
    let mut pairs = BTreeSet::new();
    for (actor, partner) in relationships {
        if actor == excluded || partner == excluded || actor == partner {
            continue;
        }
        if current_conception_candidate((actor, partner), &mut facts).is_some() {
            pairs.insert((actor.min(partner), actor.max(partner)));
        }
    }
    pairs.into_iter().collect()
}
pub struct PreparedPrenatalHistory {
    rules: Rules,
    rows: Vec<(Life, Life)>,
}
impl Runtime {
    /// Admit an authored unobserved interval as a lifecycle snapshot, preserving actual age.
    pub fn prepare_prenatal_history(
        &self,
        store: &Store,
        now: SimTime,
    ) -> Result<PreparedPrenatalHistory, Error> {
        if !self.rules.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid prenatal biology".into(),
            ));
        }
        let mut rows = Vec::new();
        for (_, before) in all_lives(store) {
            let age = before.age_at(now)?;
            if age >= self.rules.lifespan.0 {
                return Err(Error::InvalidConfiguration(
                    "biology cannot support the authored prenatal history".into(),
                ));
            }
            let mut after = before.clone();
            after.stage = self.rules.stage_at(age);
            rows.push((before.clone(), after));
        }
        rows.sort_unstable_by_key(|(before, _)| before.id);
        Ok(PreparedPrenatalHistory {
            rules: self.rules.clone(),
            rows,
        })
    }
}
impl PreparedPrenatalHistory {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        let actual = reads.membership::<Life>(store, |_, _| true);
        if actual
            != self
                .rows
                .iter()
                .map(|(row, _)| StableId(row.id.0))
                .collect()
        {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        for (before, after) in self.rows {
            let id = StableId(before.id.0);
            reads.expect_row(id, Some(before));
            admitted.protect::<Life>(id);
            batch.insert(id, after);
        }
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(runtime, store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
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
            rules: Rules {
                conception_time: SimDuration(1),
                gestation: SimDuration(10),
                infancy: SimDuration(20),
                adulthood: SimDuration(30),
                elderhood: SimDuration(40),
                lifespan: SimDuration(50),
            },
        };
        insert_life(
            &mut store,
            Life {
                id: PersonId(1),
                born: SimTime(0),
                initial_age: SimDuration(30),
                stage: LifeStage::Adult,
                reproduction: Reproduction::Gestates,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: None,
            },
        )
        .unwrap();
        (runtime, store)
    }
    #[test]
    fn prenatal_history_updates_actual_stage_atomically_and_rejects_lifespan_or_foreign_writes() {
        let (runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_prenatal_history(&store, SimTime(10))
            .unwrap();
        prepared.commit(&runtime, &mut store).unwrap();
        assert_eq!(life(&store, PersonId(1)).unwrap().stage, LifeStage::Elder);
        let before = store.snapshot().unwrap();
        assert!(
            runtime
                .prepare_prenatal_history(&store, SimTime(20))
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let prepared = runtime
            .prepare_prenatal_history(&store, SimTime(11))
            .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        batch.remove::<Life>(StableId(1));
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
    }
    fn facts() -> ParentCandidateFacts {
        ParentCandidateFacts {
            family_intent: true,
            choice: ParentChoiceInput {
                stage: LifeStage::Adult,
                alive: true,
                health: 100,
                hunger: 0,
                meal_relief: 100,
                available: true,
                wants_parenthood: true,
                partner_affection: 100,
                partner_trust: 0,
                knows_adequate_food: true,
            },
        }
    }
    #[test]
    fn conception_selection_is_bilateral_canonical_and_independent_of_input_order() {
        let relationships = [
            (PersonId(2), PersonId(1)),
            (PersonId(1), PersonId(2)),
            (PersonId(3), PersonId(1)),
        ];
        let choice = |actor, _| {
            Some(ParentCandidateFacts {
                family_intent: actor != PersonId(3),
                ..facts()
            })
        };
        let expected = vec![(PersonId(1), PersonId(2))];
        assert_eq!(
            conception_candidates(relationships, PersonId(9), choice),
            expected
        );
        assert_eq!(
            conception_candidates(relationships.into_iter().rev(), PersonId(9), choice),
            expected
        );
        assert!(conception_candidates(relationships, PersonId(1), choice).is_empty());
        assert!(adequate_food([(
            game_core::Good::Food,
            true,
            game_core::Quantity(3_000)
        )]));
        assert!(!adequate_food([(
            game_core::Good::Food,
            true,
            game_core::Quantity(2_999)
        )]));
        assert!(dependent(Some(LifeStage::Child)));
        assert!(!independent(Some(LifeStage::Infant), true));
        assert!(independent(None, false));
    }
    #[test]
    fn selected_conception_pairs_recheck_bilateral_choice_after_prior_attempts() {
        let relationships = [
            (PersonId(1), PersonId(2)),
            (PersonId(1), PersonId(3)),
            (PersonId(4), PersonId(5)),
        ];
        let selected = conception_candidates(relationships, PersonId(9), |_, _| Some(facts()));
        let mut occupied = BTreeSet::new();
        let mut accepted = Vec::new();
        for pair in selected {
            if let Some(pair) = current_conception_candidate(pair, |actor, _| {
                let mut current = facts();
                current.choice.available = !occupied.contains(&actor);
                Some(current)
            }) {
                occupied.extend([pair.0, pair.1]);
                accepted.push(pair);
            }
        }
        assert_eq!(accepted, vec![relationships[0], relationships[2]]);
        for unwilling in [PersonId(1), PersonId(2)] {
            assert!(
                current_conception_candidate(relationships[0], |actor, _| {
                    let mut current = facts();
                    current.family_intent = actor != unwilling;
                    Some(current)
                })
                .is_none()
            );
        }
        assert!(current_conception_candidate(relationships[0], |_, _| None).is_none());
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CareSourceFacts {
    pub actor: PersonId,
    pub child: PersonId,
    pub stage: LifeStage,
    pub alive: bool,
    pub available: bool,
    pub assigned_caregiver: Option<PersonId>,
    pub same_place: bool,
    pub authorized_source: bool,
    pub available_quantity: u64,
    pub dose: game_core::Quantity,
}
/// Care is an actual available action toward a living dependent with a usable dose.
pub fn prepare_care_source(facts: CareSourceFacts) -> Result<game_core::Quantity, Error> {
    if !dependent(Some(facts.stage))
        || facts.actor == facts.child
        || !facts.alive
        || !facts.available
        || facts
            .assigned_caregiver
            .is_some_and(|actor| actor != facts.actor)
        || !facts.same_place
        || !facts.authorized_source
        || facts.available_quantity < facts.dose.0
    {
        return Err(Error::Unavailable);
    }
    Ok(facts.dose)
}

#[cfg(test)]
mod care_tests {
    use super::*;
    #[test]
    fn care_requires_current_dependent_availability_permission_and_actual_supply() {
        let facts = CareSourceFacts {
            actor: PersonId(1),
            child: PersonId(2),
            stage: LifeStage::Child,
            alive: true,
            available: true,
            assigned_caregiver: None,
            same_place: true,
            authorized_source: true,
            available_quantity: 1_000,
            dose: game_core::Quantity(1_000),
        };
        assert_eq!(
            prepare_care_source(facts).unwrap(),
            game_core::Quantity(1_000)
        );
        assert!(
            prepare_care_source(CareSourceFacts {
                assigned_caregiver: Some(PersonId(3)),
                ..facts
            })
            .is_err()
        );
        assert!(
            prepare_care_source(CareSourceFacts {
                available_quantity: 999,
                ..facts
            })
            .is_err()
        );
        assert!(
            prepare_care_source(CareSourceFacts {
                stage: LifeStage::Adult,
                ..facts
            })
            .is_err()
        );
        assert!(
            prepare_care_source(CareSourceFacts {
                authorized_source: false,
                ..facts
            })
            .is_err()
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependentActivity {
    Express,
    Play,
}
impl Runtime {
    pub fn validate_dependent_activity(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        activity: DependentActivity,
        completes: SimTime,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("invalid dependent activity timing".into());
        if !self.rules.valid() {
            return Err(fail());
        }
        let life = life(store, actor).ok_or_else(fail)?;
        let duration = match activity {
            DependentActivity::Express => 6_000,
            DependentActivity::Play => MINUTE.0,
        };
        let started = SimTime(completes.0.checked_sub(duration).ok_or_else(fail)?);
        let age = life.age_at(started).map_err(|_| fail())?;
        if started > now
            || age >= self.rules.adulthood.0
            || activity == DependentActivity::Play && age < self.rules.infancy.0
        {
            return Err(fail());
        }
        Ok(())
    }
}

#[cfg(test)]
mod dependent_timing_tests {
    use super::*;
    #[test]
    fn child_activity_validates_start_age_and_fixed_duration_without_assembly() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let runtime = Runtime {
            rules: Rules {
                conception_time: SimDuration(1),
                gestation: SimDuration(10),
                infancy: SimDuration(20),
                adulthood: SimDuration(30),
                elderhood: SimDuration(40),
                lifespan: SimDuration(50),
            },
        };
        insert_life(
            &mut store,
            Life {
                id: PersonId(1),
                born: SimTime(0),
                initial_age: SimDuration(0),
                stage: LifeStage::Infant,
                reproduction: Reproduction::Gestates,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: None,
            },
        )
        .unwrap();
        runtime
            .validate_dependent_activity(
                &store,
                PersonId(1),
                SimTime(0),
                DependentActivity::Express,
                SimTime(6_000),
            )
            .unwrap();
        runtime
            .validate_dependent_activity(
                &store,
                PersonId(1),
                SimTime(20),
                DependentActivity::Play,
                SimTime(MINUTE.0 + 20),
            )
            .unwrap();
        assert!(
            runtime
                .validate_dependent_activity(
                    &store,
                    PersonId(1),
                    SimTime(19),
                    DependentActivity::Play,
                    SimTime(MINUTE.0 + 19)
                )
                .is_err()
        );
        assert!(
            runtime
                .validate_dependent_activity(
                    &store,
                    PersonId(1),
                    SimTime(29),
                    DependentActivity::Play,
                    SimTime(MINUTE.0 + 30)
                )
                .is_err()
        );
        assert!(
            runtime
                .validate_dependent_activity(
                    &store,
                    PersonId(1),
                    SimTime(30),
                    DependentActivity::Express,
                    SimTime(6_000 + 30)
                )
                .is_err()
        );
    }
}

pub fn adult_for_work(stage: Option<LifeStage>, lifecycle_enabled: bool) -> bool {
    !lifecycle_enabled || matches!(stage, Some(LifeStage::Adult | LifeStage::Elder))
}

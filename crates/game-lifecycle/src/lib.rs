//! Biological lifecycle state and deterministic transitions. Care and family
//! institutions remain separate from physical birth and age.
mod storage;
mod transitions;
use game_ecs::Store;
pub use storage::*;
pub use transitions::*;

pub mod heredity;

use game_core::{
    ChildhoodInterest, Error, LifeStage, MINUTE, PersonId, PregnancyId, SimDuration, SimTime,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub conception_time: SimDuration,
    pub gestation: SimDuration,
    pub infancy: SimDuration,
    pub adulthood: SimDuration,
    pub elderhood: SimDuration,
    pub lifespan: SimDuration,
}
impl Rules {
    pub fn human() -> Self {
        let day = 86_400_000;
        let year = 365 * day;
        Self {
            conception_time: MINUTE,
            gestation: SimDuration(280 * day),
            infancy: SimDuration(2 * year),
            adulthood: SimDuration(18 * year),
            elderhood: SimDuration(65 * year),
            lifespan: SimDuration(90 * year),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reproduction {
    Gestates,
    SuppliesSeed,
    Neither,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Life {
    pub id: PersonId,
    pub born: SimTime,
    pub initial_age: SimDuration,
    pub stage: LifeStage,
    pub reproduction: Reproduction,
    pub parents: Vec<PersonId>,
    pub caregivers: BTreeSet<PersonId>,
    pub preference: Option<(ChildhoodInterest, SimTime)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PregnancyState {
    Conceiving { completes: SimTime },
    Gestating { due: SimTime },
    Born(PersonId),
    Ended,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pregnancy {
    pub id: PregnancyId,
    pub gestator: PersonId,
    pub partner: PersonId,
    pub state: PregnancyState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
}

impl Rules {
    pub fn valid(&self) -> bool {
        self.conception_time.0 > 0
            && self.gestation.0 > 0
            && SimDuration(0) < self.infancy
            && self.infancy < self.adulthood
            && self.adulthood < self.elderhood
            && self.elderhood < self.lifespan
    }
    pub fn stage_at(&self, age: u64) -> LifeStage {
        if age < self.infancy.0 {
            LifeStage::Infant
        } else if age < self.adulthood.0 {
            LifeStage::Child
        } else if age < self.elderhood.0 {
            LifeStage::Adult
        } else {
            LifeStage::Elder
        }
    }
    pub fn effective_lifespan(&self, lifespan_loss: u64) -> u64 {
        self.lifespan.0.saturating_sub(lifespan_loss)
    }
}
impl Life {
    pub fn age_at(&self, now: SimTime) -> Result<u64, Error> {
        now.0
            .checked_sub(self.born.0)
            .and_then(|elapsed| elapsed.checked_add(self.initial_age.0))
            .ok_or(Error::Overflow)
    }
    pub fn maturity_milli(&self, rules: &Rules, now: SimTime) -> Result<u16, Error> {
        if rules.adulthood.0 == 0 {
            return Err(Error::InvalidConfiguration("zero adulthood age".into()));
        }
        Ok(
            (u128::from(self.age_at(now)?) * 1_000 / u128::from(rules.adulthood.0)).min(1_000)
                as u16,
        )
    }
    pub fn next_age_deadline(
        &self,
        rules: &Rules,
        now: SimTime,
        lifespan_loss: u64,
        aging_enabled: bool,
    ) -> Result<Option<SimTime>, Error> {
        if !aging_enabled {
            return Ok(None);
        }
        let age = self.age_at(now)?;
        let threshold = [
            rules.infancy.0,
            rules.adulthood.0,
            rules.elderhood.0,
            rules.effective_lifespan(lifespan_loss),
        ]
        .into_iter()
        .find(|threshold| *threshold > age);
        threshold
            .map(|threshold| {
                let at = u128::from(self.born.0) + u128::from(threshold);
                let at = at
                    .checked_sub(u128::from(self.initial_age.0))
                    .ok_or(Error::Overflow)?;
                Ok(SimTime(u64::try_from(at).map_err(|_| Error::Overflow)?))
            })
            .transpose()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgeEffect {
    pub actor: PersonId,
    pub stage: LifeStage,
    pub died: bool,
    pub next_deadline: Option<SimTime>,
}
impl Runtime {
    /// Pure evaluation; the host publishes body death or stage changes atomically
    /// with the returned deadline. Lifespan adjustment is an explicit input.
    pub fn evaluate_age(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        lifespan_loss: u64,
        aging_enabled: bool,
    ) -> Result<AgeEffect, Error> {
        let life = life(store, actor).ok_or(Error::Unavailable)?;
        if !aging_enabled {
            return Ok(AgeEffect {
                actor,
                stage: life.stage,
                died: false,
                next_deadline: None,
            });
        }
        let age = life.age_at(now)?;
        let died = age >= self.rules.effective_lifespan(lifespan_loss);
        Ok(AgeEffect {
            actor,
            stage: self.rules.stage_at(age),
            died,
            next_deadline: if died {
                None
            } else {
                life.next_age_deadline(&self.rules, now, lifespan_loss, true)?
            },
        })
    }
}
/// The deciding person's current body, aims and remembered resources. Physical
/// proximity and compatible reproductive bodies are separate execution gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParentChoiceInput {
    pub stage: LifeStage,
    pub alive: bool,
    pub health: u64,
    pub hunger: u64,
    pub meal_relief: u64,
    pub available: bool,
    pub wants_parenthood: bool,
    pub partner_affection: i32,
    pub partner_trust: i32,
    pub knows_adequate_food: bool,
}
pub fn willing_parent(input: ParentChoiceInput) -> bool {
    matches!(input.stage, LifeStage::Adult | LifeStage::Elder)
        && input.alive
        && input.health > 0
        && input.hunger < input.meal_relief
        && input.available
        && input.wants_parenthood
        && input.partner_affection >= 100
        && input.partner_trust >= 0
        && input.knows_adequate_food
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PregnancyEndedEffect {
    pub pregnancy: PregnancyId,
    pub parents: [PersonId; 2],
    pub was_conceiving: bool,
}
/// Pure selection of compatible reproductive bodies. It grants no social duty,
/// family structure, permission, or conception consent.
pub fn reproductive_pair(
    first: (PersonId, Reproduction),
    second: (PersonId, Reproduction),
) -> Option<(PersonId, PersonId)> {
    if first.0 == second.0 {
        return None;
    }
    match (first.1, second.1) {
        (Reproduction::Gestates, Reproduction::SuppliesSeed) => Some((first.0, second.0)),
        (Reproduction::SuppliesSeed, Reproduction::Gestates) => Some((second.0, first.0)),
        _ => None,
    }
}

pub fn validate_runtime(world: &Runtime, store: &Store, now: SimTime) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent development state".into());
    if !world.rules.valid() {
        return Err(fail());
    }
    for (actor, life) in all_lives(store) {
        if life.validate().is_err()
            || actor != life.id
            || life.born > now
            || life.age_at(now).is_err()
            || life
                .parents
                .iter()
                .any(|parent| *parent == actor || crate::life(store, *parent).is_none())
            || life
                .caregivers
                .iter()
                .any(|carer| crate::life(store, *carer).is_none())
        {
            return Err(fail());
        }
    }
    let mut pregnant = BTreeSet::new();
    for (id, pregnancy) in all_pregnancies(store) {
        if pregnancy.validate().is_err()
            || id != pregnancy.id
            || reproductive_pair(
                (
                    pregnancy.gestator,
                    life(store, pregnancy.gestator)
                        .ok_or_else(fail)?
                        .reproduction,
                ),
                (
                    pregnancy.partner,
                    life(store, pregnancy.partner)
                        .ok_or_else(fail)?
                        .reproduction,
                ),
            ) != Some((pregnancy.gestator, pregnancy.partner))
        {
            return Err(fail());
        }
        match pregnancy.state {
            PregnancyState::Conceiving { completes }
            | PregnancyState::Gestating { due: completes } => {
                if completes <= now || !pregnant.insert(pregnancy.gestator) {
                    return Err(fail());
                }
            }
            PregnancyState::Born(child) => {
                if !life(store, child).is_some_and(|life| {
                    life.initial_age.0 == 0
                        && life.parents == [pregnancy.gestator, pregnancy.partner]
                }) {
                    return Err(fail());
                }
            }
            PregnancyState::Ended => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules() -> Rules {
        Rules {
            conception_time: MINUTE,
            gestation: SimDuration(180_000),
            infancy: SimDuration(300_000),
            adulthood: SimDuration(600_000),
            elderhood: SimDuration(1_200_000),
            lifespan: SimDuration(1_500_000),
        }
    }
    fn life_row(id: u64, reproduction: Reproduction) -> Life {
        Life {
            id: PersonId(id),
            born: SimTime(1_000),
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction,
            parents: Vec::new(),
            caregivers: BTreeSet::new(),
            preference: None,
        }
    }
    fn fixture_world() -> (Runtime, Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for row in [
            life_row(1, Reproduction::Gestates),
            life_row(2, Reproduction::SuppliesSeed),
        ] {
            insert_life(&mut store, row).unwrap();
        }
        (Runtime { rules: rules() }, store)
    }
    #[test]
    fn conception_willingness_uses_current_personal_choice_not_a_permanent_family_promise() {
        let input = ParentChoiceInput {
            stage: LifeStage::Adult,
            alive: true,
            health: 100,
            hunger: 10,
            meal_relief: 300,
            available: true,
            wants_parenthood: true,
            partner_affection: 200,
            partner_trust: 300,
            knows_adequate_food: true,
        };
        assert!(willing_parent(input));
        assert!(!willing_parent(ParentChoiceInput {
            stage: LifeStage::Child,
            ..input
        }));
        assert!(!willing_parent(ParentChoiceInput {
            wants_parenthood: false,
            ..input
        }));
        assert!(!willing_parent(ParentChoiceInput {
            hunger: 300,
            ..input
        }));
        assert!(!willing_parent(ParentChoiceInput {
            available: false,
            ..input
        }));
        assert!(!willing_parent(ParentChoiceInput {
            knows_adequate_food: false,
            ..input
        }));
    }
    #[test]
    fn actual_age_boundaries_and_next_deadlines_need_only_life_and_rules() {
        let (world, store) = fixture_world();
        for (now, stage, deadline) in [
            (300_999, LifeStage::Infant, 301_000),
            (301_000, LifeStage::Child, 601_000),
            (601_000, LifeStage::Adult, 1_201_000),
            (1_201_000, LifeStage::Elder, 1_501_000),
        ] {
            let result = world
                .evaluate_age(&store, PersonId(1), SimTime(now), 0, true)
                .unwrap();
            assert_eq!(result.stage, stage);
            assert_eq!(result.next_deadline, Some(SimTime(deadline)));
            assert!(!result.died);
        }
        let result = world
            .evaluate_age(&store, PersonId(1), SimTime(1_501_000), 0, true)
            .unwrap();
        assert!(result.died);
        assert_eq!(result.next_deadline, None);
    }
    #[test]
    fn initial_age_explicit_lifespan_adjustment_and_nonaging_are_distinct_inputs() {
        let (world, mut store) = fixture_world();
        life_mut(&mut store, PersonId(1)).unwrap().initial_age = world.rules.adulthood;
        assert_eq!(
            life(&store, PersonId(1))
                .unwrap()
                .next_age_deadline(&world.rules, SimTime(1_000), 0, true)
                .unwrap(),
            Some(SimTime(601_000))
        );
        let result = world
            .evaluate_age(&store, PersonId(1), SimTime(801_000), 100_000, true)
            .unwrap();
        assert!(result.died);
        assert_eq!(
            world
                .evaluate_age(&store, PersonId(1), SimTime(801_000), 100_000, false)
                .unwrap()
                .next_deadline,
            None
        );
        assert!(
            life(&store, PersonId(1))
                .unwrap()
                .age_at(SimTime(999))
                .is_err()
        );
    }
    #[test]
    fn life_state_serialization_and_local_validation_do_not_require_a_simulation() {
        let (world, store) = fixture_world();
        validate_runtime(&world, &store, SimTime(1_000)).unwrap();
        let encoded = serde_json::to_vec(&world).unwrap();
        assert_eq!(world, serde_json::from_slice::<Runtime>(&encoded).unwrap());
        let restored = Store::restore(
            store.schema().clone(),
            serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(restored, store);
        validate_runtime(&world, &restored, SimTime(1_000)).unwrap();
        let mut invalid = world;
        invalid.rules.adulthood = invalid.rules.infancy;
        assert!(validate_runtime(&invalid, &store, SimTime(1_000)).is_err());
    }
    #[test]
    fn lifespan_response_guards_age_rules_clock_and_survival_before_publication() {
        let (runtime, mut store) = fixture_world();
        let facts = LifespanFacts {
            actor: PersonId(1),
            now: SimTime(1_000),
            alive: true,
            lifespan_loss: 100_000,
        };
        let prepared = runtime.prepare_lifespan_change(&store, facts).unwrap();
        let batch = game_ecs::Batch::new();
        assert!(matches!(
            prepared.check(&runtime, &store, facts, &batch).unwrap(),
            LifespanResponse::Deadline {
                actor: PersonId(1),
                ..
            }
        ));
        assert!(
            prepared
                .check(
                    &runtime,
                    &store,
                    LifespanFacts {
                        now: SimTime(1_001),
                        ..facts
                    },
                    &batch
                )
                .is_err()
        );
        assert!(
            prepared
                .check(
                    &runtime,
                    &store,
                    LifespanFacts {
                        alive: false,
                        ..facts
                    },
                    &batch
                )
                .is_err()
        );
        let mut changed = runtime.clone();
        changed.rules.lifespan.0 += 1;
        assert!(prepared.check(&changed, &store, facts, &batch).is_err());
        let mut foreign = game_ecs::Batch::new();
        foreign.remove::<Life>(game_ecs::StableId(1));
        assert!(prepared.check(&runtime, &store, facts, &foreign).is_err());
        life_mut(&mut store, PersonId(1)).unwrap().initial_age.0 += 1;
        assert!(prepared.check(&runtime, &store, facts, &batch).is_err());
        let dead = LifespanFacts {
            alive: false,
            lifespan_loss: u64::MAX,
            ..facts
        };
        assert_eq!(
            runtime
                .prepare_lifespan_change(&store, dead)
                .unwrap()
                .check(&runtime, &store, dead, &batch)
                .unwrap(),
            LifespanResponse::Unchanged
        );
        let expires = LifespanFacts {
            lifespan_loss: runtime.rules.lifespan.0,
            ..facts
        };
        assert_eq!(
            runtime
                .prepare_lifespan_change(&store, expires)
                .unwrap()
                .check(&runtime, &store, expires, &batch)
                .unwrap(),
            LifespanResponse::Died { actor: PersonId(1) }
        );
    }
}

/// A lifespan penalty changes death or the next age deadline; a dead body is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifespanResponse {
    Unchanged,
    Died {
        actor: PersonId,
    },
    Deadline {
        actor: PersonId,
        at: Option<SimTime>,
    },
}
/// The physical owner supplies survival. Invocation supplies the accepted penalty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifespanFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub alive: bool,
    pub lifespan_loss: u64,
}
#[derive(Clone, Debug)]
pub struct PreparedLifespanResponse {
    before: Life,
    rules: Rules,
    facts: LifespanFacts,
    response: LifespanResponse,
}
impl PreparedLifespanResponse {
    /// Validate the final composition before publishing any cost or session edits.
    /// A concurrent age-row edit is incompatible with this already-evaluated response.
    pub fn check(
        &self,
        runtime: &Runtime,
        store: &Store,
        facts: LifespanFacts,
        batch: &game_ecs::Batch,
    ) -> Result<LifespanResponse, Error> {
        if self.facts != facts
            || self.rules != runtime.rules
            || life(store, facts.actor) != Some(&self.before)
            || batch.touches::<Life>(game_ecs::StableId(facts.actor.0))
        {
            return Err(Error::Unavailable);
        }
        Ok(self.response)
    }
}
impl Runtime {
    pub fn prepare_lifespan_change(
        &self,
        store: &Store,
        facts: LifespanFacts,
    ) -> Result<PreparedLifespanResponse, Error> {
        if !self.rules.valid() {
            return Err(Error::Unavailable);
        }
        let before = life(store, facts.actor).ok_or(Error::Unavailable)?.clone();
        let response = if !facts.alive {
            LifespanResponse::Unchanged
        } else {
            let effect =
                self.evaluate_age(store, facts.actor, facts.now, facts.lifespan_loss, true)?;
            if effect.died {
                LifespanResponse::Died { actor: facts.actor }
            } else {
                LifespanResponse::Deadline {
                    actor: facts.actor,
                    at: effect.next_deadline,
                }
            }
        };
        Ok(PreparedLifespanResponse {
            before,
            rules: self.rules.clone(),
            facts,
            response,
        })
    }
}

mod history;
pub use history::*;

mod dependent_actions;
pub use dependent_actions::*;

mod menu;
pub use menu::*;

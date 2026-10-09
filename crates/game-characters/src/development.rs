//! Biological lifecycle state and deterministic transitions. Care and family
//! institutions remain separate from physical birth and age.
use game_core::{
    ChildhoodInterest, Error, LifeStage, MINUTE, PersonId, PregnancyId, SimDuration, SimTime,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct Pregnancy {
    pub id: PregnancyId,
    pub gestator: PersonId,
    pub partner: PersonId,
    pub state: PregnancyState,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub lives: BTreeMap<PersonId, Life>,
    pub pregnancies: BTreeMap<PregnancyId, Pregnancy>,
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
impl World {
    /// Pure evaluation; the host publishes body death or stage changes atomically
    /// with the returned deadline. Lifespan adjustment is an explicit input.
    pub fn evaluate_age(
        &self,
        actor: PersonId,
        now: SimTime,
        lifespan_loss: u64,
        aging_enabled: bool,
    ) -> Result<AgeEffect, Error> {
        let life = self.lives.get(&actor).ok_or(Error::Unavailable)?;
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
pub enum ConceptionEffect {
    Gestating { due: SimTime },
    Ended,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BirthEffect {
    Ready,
    Ended,
}
impl Pregnancy {
    /// Physical continuity/consent is evaluated by the participants and host.
    /// This transition knows only whether the coupled attempt remained valid.
    pub fn complete_conception(
        &mut self,
        now: SimTime,
        rules: &Rules,
        participants_still_valid: bool,
    ) -> Result<ConceptionEffect, Error> {
        if self.state != (PregnancyState::Conceiving { completes: now }) {
            return Err(Error::Unavailable);
        }
        let effect = if participants_still_valid {
            ConceptionEffect::Gestating {
                due: SimTime(
                    now.0
                        .checked_add(rules.gestation.0)
                        .ok_or(Error::Overflow)?,
                ),
            }
        } else {
            ConceptionEffect::Ended
        };
        self.state = match effect {
            ConceptionEffect::Gestating { due } => PregnancyState::Gestating { due },
            ConceptionEffect::Ended => PregnancyState::Ended,
        };
        Ok(effect)
    }
    pub fn birth_due(&mut self, now: SimTime, gestator_alive: bool) -> Result<BirthEffect, Error> {
        if self.state != (PregnancyState::Gestating { due: now }) {
            return Err(Error::Unavailable);
        }
        if gestator_alive {
            Ok(BirthEffect::Ready)
        } else {
            self.state = PregnancyState::Ended;
            Ok(BirthEffect::Ended)
        }
    }
    pub fn record_actual_birth(&mut self, child: PersonId, now: SimTime) -> Result<(), Error> {
        if self.state != (PregnancyState::Gestating { due: now })
            || child == self.gestator
            || child == self.partner
        {
            return Err(Error::Unavailable);
        }
        self.state = PregnancyState::Born(child);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PregnancyEndedEffect {
    pub pregnancy: PregnancyId,
    pub parents: [PersonId; 2],
    pub was_conceiving: bool,
}
impl World {
    /// A partner's later death does not cancel an already gestating pregnancy.
    /// Ending the coupled attempt or the gestator's life does.
    pub fn invalidate_pregnancy(
        &mut self,
        id: PregnancyId,
        actor: PersonId,
        died: bool,
    ) -> Option<PregnancyEndedEffect> {
        self.pregnancies.get_mut(&id)?.invalidate(actor, died)
    }
    pub fn invalidate_pregnancies(
        &mut self,
        actor: PersonId,
        died: bool,
    ) -> Vec<PregnancyEndedEffect> {
        self.pregnancies
            .values_mut()
            .filter_map(|pregnancy| pregnancy.invalidate(actor, died))
            .collect()
    }
}
impl Pregnancy {
    pub fn invalidate(&mut self, actor: PersonId, died: bool) -> Option<PregnancyEndedEffect> {
        let was_conceiving = matches!(self.state, PregnancyState::Conceiving { .. });
        let interrupted_attempt =
            was_conceiving && (self.gestator == actor || self.partner == actor);
        let gestator_died = died
            && self.gestator == actor
            && matches!(
                self.state,
                PregnancyState::Conceiving { .. } | PregnancyState::Gestating { .. }
            );
        if !(interrupted_attempt || gestator_died) {
            return None;
        }
        self.state = PregnancyState::Ended;
        Some(PregnancyEndedEffect {
            pregnancy: self.id,
            parents: [self.gestator, self.partner],
            was_conceiving,
        })
    }
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

pub fn validate_local(world: &World, now: SimTime) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent development state".into());
    if !world.rules.valid() {
        return Err(fail());
    }
    for (actor, life) in &world.lives {
        if *actor != life.id
            || life.born > now
            || life.age_at(now).is_err()
            || life
                .parents
                .iter()
                .any(|parent| parent == actor || !world.lives.contains_key(parent))
            || life
                .caregivers
                .iter()
                .any(|carer| !world.lives.contains_key(carer))
        {
            return Err(fail());
        }
    }
    let mut pregnant = BTreeSet::new();
    for (id, pregnancy) in &world.pregnancies {
        if *id != pregnancy.id
            || reproductive_pair(
                (
                    pregnancy.gestator,
                    world
                        .lives
                        .get(&pregnancy.gestator)
                        .ok_or_else(fail)?
                        .reproduction,
                ),
                (
                    pregnancy.partner,
                    world
                        .lives
                        .get(&pregnancy.partner)
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
                if !world.lives.get(&child).is_some_and(|life| {
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
    fn life(id: u64, reproduction: Reproduction) -> Life {
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
    fn fixture_world() -> World {
        World {
            rules: rules(),
            lives: BTreeMap::from([
                (PersonId(1), life(1, Reproduction::Gestates)),
                (PersonId(2), life(2, Reproduction::SuppliesSeed)),
            ]),
            pregnancies: BTreeMap::new(),
        }
    }
    fn fixture_pregnancy() -> Pregnancy {
        Pregnancy {
            id: PregnancyId(3),
            gestator: PersonId(1),
            partner: PersonId(2),
            state: PregnancyState::Conceiving {
                completes: SimTime(60_000),
            },
        }
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
        let world = fixture_world();
        for (now, stage, deadline) in [
            (300_999, LifeStage::Infant, 301_000),
            (301_000, LifeStage::Child, 601_000),
            (601_000, LifeStage::Adult, 1_201_000),
            (1_201_000, LifeStage::Elder, 1_501_000),
        ] {
            let result = world
                .evaluate_age(PersonId(1), SimTime(now), 0, true)
                .unwrap();
            assert_eq!(result.stage, stage);
            assert_eq!(result.next_deadline, Some(SimTime(deadline)));
            assert!(!result.died);
        }
        let result = world
            .evaluate_age(PersonId(1), SimTime(1_501_000), 0, true)
            .unwrap();
        assert!(result.died);
        assert_eq!(result.next_deadline, None);
    }
    #[test]
    fn initial_age_explicit_lifespan_adjustment_and_nonaging_are_distinct_inputs() {
        let mut world = fixture_world();
        world.lives.get_mut(&PersonId(1)).unwrap().initial_age = world.rules.adulthood;
        assert_eq!(
            world.lives[&PersonId(1)]
                .next_age_deadline(&world.rules, SimTime(1_000), 0, true)
                .unwrap(),
            Some(SimTime(601_000))
        );
        let result = world
            .evaluate_age(PersonId(1), SimTime(801_000), 100_000, true)
            .unwrap();
        assert!(result.died);
        assert_eq!(
            world
                .evaluate_age(PersonId(1), SimTime(801_000), 100_000, false)
                .unwrap()
                .next_deadline,
            None
        );
        assert!(world.lives[&PersonId(1)].age_at(SimTime(999)).is_err());
    }
    #[test]
    fn conception_transition_is_timed_and_invalid_or_overflowing_attempts_are_inert() {
        let mut pregnancy = fixture_pregnancy();
        let before = pregnancy.clone();
        assert_eq!(
            pregnancy.complete_conception(SimTime(59_999), &rules(), true),
            Err(Error::Unavailable)
        );
        assert_eq!(pregnancy, before);
        let result = pregnancy
            .complete_conception(SimTime(60_000), &rules(), true)
            .unwrap();
        assert_eq!(
            result,
            ConceptionEffect::Gestating {
                due: SimTime(240_000)
            }
        );
        assert_eq!(
            pregnancy.state,
            PregnancyState::Gestating {
                due: SimTime(240_000)
            }
        );
        let mut overflow = before;
        overflow.state = PregnancyState::Conceiving {
            completes: SimTime(u64::MAX),
        };
        let saved = overflow.clone();
        assert_eq!(
            overflow.complete_conception(SimTime(u64::MAX), &rules(), true),
            Err(Error::Overflow)
        );
        assert_eq!(overflow, saved);
    }
    #[test]
    fn pregnancy_survives_later_partner_death_but_gestator_death_ends_it() {
        let mut world = fixture_world();
        let mut pregnancy = fixture_pregnancy();
        pregnancy
            .complete_conception(SimTime(60_000), &world.rules, true)
            .unwrap();
        world.pregnancies.insert(pregnancy.id, pregnancy);
        assert!(world.invalidate_pregnancies(PersonId(2), true).is_empty());
        let ended = world.invalidate_pregnancies(PersonId(1), true);
        assert_eq!(ended.len(), 1);
        assert!(!ended[0].was_conceiving);
        assert_eq!(
            world.pregnancies[&PregnancyId(3)].state,
            PregnancyState::Ended
        );
        let mut world = fixture_world();
        world
            .pregnancies
            .insert(PregnancyId(3), fixture_pregnancy());
        let ended = world.invalidate_pregnancies(PersonId(2), false);
        assert!(ended[0].was_conceiving);
    }
    #[test]
    fn recording_birth_requires_a_real_due_transition_and_preserves_parent_identity() {
        let mut pregnancy = fixture_pregnancy();
        pregnancy
            .complete_conception(SimTime(60_000), &rules(), true)
            .unwrap();
        let before = pregnancy.clone();
        assert_eq!(
            pregnancy.birth_due(SimTime(239_999), true),
            Err(Error::Unavailable)
        );
        assert_eq!(pregnancy, before);
        assert_eq!(
            pregnancy.birth_due(SimTime(240_000), true),
            Ok(BirthEffect::Ready)
        );
        assert_eq!(pregnancy, before);
        pregnancy
            .record_actual_birth(PersonId(4), SimTime(240_000))
            .unwrap();
        assert_eq!(pregnancy.state, PregnancyState::Born(PersonId(4)));
        let mut dead = before;
        assert_eq!(
            dead.birth_due(SimTime(240_000), false),
            Ok(BirthEffect::Ended)
        );
        assert_eq!(dead.state, PregnancyState::Ended);
    }
    #[test]
    fn life_state_serialization_and_local_validation_do_not_require_a_simulation() {
        let world = fixture_world();
        validate_local(&world, SimTime(1_000)).unwrap();
        let encoded = serde_json::to_vec(&world).unwrap();
        assert_eq!(world, serde_json::from_slice::<World>(&encoded).unwrap());
        let mut invalid = world;
        invalid.rules.adulthood = invalid.rules.infancy;
        assert!(validate_local(&invalid, SimTime(1_000)).is_err());
    }
}

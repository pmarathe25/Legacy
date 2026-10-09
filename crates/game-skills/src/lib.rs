//! Personal learning and exact skill-adjusted work arithmetic.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub per_level: SimDuration,
    pub max_level: u16,
    pub work_bonus_per_level: u16,
    pub damage_bonus_per_level: u16,
    pub action_practice_ms: u64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            // Explicit accelerated fixture rates; full settings can replace them.
            per_level: MINUTE,
            max_level: 100,
            work_bonus_per_level: 10,
            damage_bonus_per_level: 5,
            action_practice_ms: 6_000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub practice_ms: u64,
    pub level: u16,
    pub progress_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub records: BTreeMap<PersonId, BTreeMap<Skill, Progress>>,
    pub semantic_ticks: BTreeMap<PersonId, BTreeMap<Skill, u64>>,
}

impl World {
    /// Time to a live mastery change during continuous practice. Guides cannot
    /// teach across a level their own accumulated mastery has not reached.
    pub fn until_next_level(
        &self,
        actor: PersonId,
        skill: Skill,
        mastery_cap: Option<u64>,
    ) -> Result<Option<u64>, Error> {
        if self.rules.per_level.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "zero skill level duration".into(),
            ));
        }
        let practice = self.practice(actor, skill);
        let level = practice / self.rules.per_level.0;
        if level >= u64::from(self.rules.max_level) {
            return Ok(None);
        }
        let threshold = (level + 1)
            .checked_mul(self.rules.per_level.0)
            .ok_or(Error::Overflow)?;
        if mastery_cap.is_some_and(|cap| cap < threshold) {
            return Ok(None);
        }
        Ok(Some(threshold - practice))
    }
    pub fn level(&self, actor: PersonId, skill: Skill) -> u16 {
        self.records
            .get(&actor)
            .and_then(|skills| skills.get(&skill))
            .map_or(0, |p| p.level)
    }

    pub fn practice(&self, actor: PersonId, skill: Skill) -> u64 {
        self.records
            .get(&actor)
            .and_then(|skills| skills.get(&skill))
            .map_or(0, |p| p.practice_ms)
    }

    pub fn credit(
        &mut self,
        actor: PersonId,
        skill: Skill,
        milliseconds: u64,
        mastery_cap: Option<u64>,
    ) {
        if milliseconds == 0 {
            return;
        }
        let cap = self.rules.per_level.0 * u64::from(self.rules.max_level);
        let existing = self.practice(actor, skill);
        // A guide cannot teach beyond their personal mastery. A pupil with more
        // experience retains it; an inadequate guide never subtracts learning.
        let cap = mastery_cap.map_or(cap, |teacher| cap.min(teacher.max(existing)));
        let credited =
            (u128::from(existing) + u128::from(milliseconds)).min(u128::from(cap)) as u64;
        if credited == existing {
            return;
        }
        self.records
            .entry(actor)
            .or_default()
            .insert(skill, progress(credited, &self.rules));
    }
}

pub fn progress(practice_ms: u64, rules: &Rules) -> Progress {
    let level = (practice_ms / rules.per_level.0).min(u64::from(rules.max_level)) as u16;
    Progress {
        practice_ms,
        level,
        progress_ms: if level == rules.max_level {
            0
        } else {
            practice_ms % rules.per_level.0
        },
    }
}

pub fn rate_for_level(level: u16, rules: &Rules) -> SkillRate {
    SkillRate {
        numerator: 1_000
            + u64::from(level.min(rules.max_level)) * u64::from(rules.work_bonus_per_level),
        denominator: 1_000,
    }
}

pub fn advance_work(elapsed: u64, rate: SkillRate, remainder: u64) -> Result<(u64, u64), Error> {
    if rate.numerator == 0 || rate.denominator == 0 || remainder >= rate.denominator {
        return Err(Error::InvalidConfiguration(
            "invalid frozen skill work rate".into(),
        ));
    }
    let numerator = u128::from(elapsed) * u128::from(rate.numerator) + u128::from(remainder);
    let work =
        u64::try_from(numerator / u128::from(rate.denominator)).map_err(|_| Error::Overflow)?;
    let remainder = (numerator % u128::from(rate.denominator)) as u64;
    Ok((work, remainder))
}

pub fn work_duration(remaining: u64, rate: SkillRate, remainder: u64) -> Result<u64, Error> {
    if rate.numerator == 0 || rate.denominator == 0 || remainder >= rate.denominator {
        return Err(Error::InvalidConfiguration(
            "invalid frozen skill work rate".into(),
        ));
    }
    if remaining == 0 {
        return Ok(0);
    }
    let needed = (u128::from(remaining) * u128::from(rate.denominator))
        .saturating_sub(u128::from(remainder));
    u64::try_from(needed.div_ceil(u128::from(rate.numerator))).map_err(|_| Error::Overflow)
}

pub fn damage_for_level(base: u64, level: u16, rules: &Rules) -> Result<u64, Error> {
    let numerator =
        1_000 + u64::from(level.min(rules.max_level)) * u64::from(rules.damage_bonus_per_level);
    u64::try_from(u128::from(base) * u128::from(numerator) / 1_000).map_err(|_| Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_training_is_fractional_bounded_and_slice_invariant() {
        let mut whole = World::default();
        let mut sliced = whole.clone();
        whole.credit(PersonId(1), Skill::Craft, 170_003, None);
        for elapsed in [1, 59_999, 10_003, 40_000, 60_000] {
            sliced.credit(PersonId(1), Skill::Craft, elapsed, None);
        }
        assert_eq!(whole, sliced);
        assert_eq!(whole.level(PersonId(1), Skill::Craft), 2);
        assert_eq!(
            whole.records[&PersonId(1)][&Skill::Craft].progress_ms,
            50_003
        );
        assert_eq!(whole.level(PersonId(1), Skill::Farming), 0);
        whole.credit(PersonId(1), Skill::Craft, u64::MAX, None);
        assert_eq!(whole.level(PersonId(1), Skill::Craft), 100);
        assert_eq!(whole.records[&PersonId(1)][&Skill::Craft].progress_ms, 0);
        assert_eq!(whole.level(PersonId(2), Skill::Craft), 0);
    }

    #[test]
    fn rational_work_progress_and_completion_share_the_exact_remainder() {
        let rate = SkillRate {
            numerator: 1_370,
            denominator: 1_000,
        };
        let (whole, remainder) = advance_work(70_013, rate, 19).unwrap();
        let (mut total, mut carry) = (0, 19);
        for elapsed in [1, 9, 60_003, 10_000] {
            let (work, next) = advance_work(elapsed, rate, carry).unwrap();
            total += work;
            carry = next;
        }
        assert_eq!((whole, remainder), (total, carry));
        let duration = work_duration(60_000, rate, 19).unwrap();
        assert!(advance_work(duration - 1, rate, 19).unwrap().0 < 60_000);
        assert!(advance_work(duration, rate, 19).unwrap().0 >= 60_000);
        assert_eq!(
            work_duration(60_000, SkillRate::default(), 0).unwrap(),
            60_000
        );
        assert!(advance_work(u64::MAX, rate, 0).is_err());
    }

    #[test]
    fn guidance_cannot_copy_a_teachers_full_mastery_and_borrowing_is_read_only() {
        let mut world = World::default();
        world.credit(PersonId(1), Skill::Craft, 300_000, None);
        let before = world.clone();
        let borrowed = rate_for_level(world.level(PersonId(1), Skill::Craft), &world.rules);
        assert!(borrowed.numerator > 1_000);
        assert_eq!(world, before);
        let teacher = world.practice(PersonId(1), Skill::Craft);
        world.credit(PersonId(2), Skill::Craft, 30_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 0);
        world.credit(PersonId(2), Skill::Craft, 600_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 5);
        world.credit(PersonId(2), Skill::Craft, 60_000, None);
        assert_eq!(world.level(PersonId(2), Skill::Craft), 6);
        world.credit(PersonId(2), Skill::Craft, 60_000, Some(teacher));
        assert_eq!(world.level(PersonId(2), Skill::Craft), 6);
        assert_eq!(damage_for_level(45_000, 100, &world.rules).unwrap(), 67_500);
    }
    #[test]
    fn live_mastery_changes_have_exact_boundaries_and_respect_a_guides_cap() {
        let mut world = World::default();
        let actor = PersonId(1);
        world.credit(actor, Skill::Medicine, 90_000, None);
        assert_eq!(
            world
                .until_next_level(actor, Skill::Medicine, None)
                .unwrap(),
            Some(30_000)
        );
        assert_eq!(
            world
                .until_next_level(actor, Skill::Medicine, Some(119_999))
                .unwrap(),
            None
        );
        assert_eq!(
            world
                .until_next_level(actor, Skill::Medicine, Some(120_000))
                .unwrap(),
            Some(30_000)
        );
        world.credit(actor, Skill::Medicine, u64::MAX, None);
        assert_eq!(
            world
                .until_next_level(actor, Skill::Medicine, None)
                .unwrap(),
            None
        );
        world.rules.per_level = SimDuration(0);
        assert!(
            world
                .until_next_level(actor, Skill::Medicine, None)
                .is_err()
        );
    }
}

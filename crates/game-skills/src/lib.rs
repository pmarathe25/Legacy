//! Personal learning and exact skill-adjusted work arithmetic.
use game_core::*;
use game_ecs::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod learning;
mod practice;
mod storage;
pub use learning::{
    ActivityFacts, ActorFacts, CareWork, EvaluatedLearning, InteractionFacts, LearningEvent,
    Lesson, PreparedLearningAcknowledgement, SocialFacts, scale_damage, validate_world,
};
pub use practice::{PreparedNewborn, PreparedPractice, StagedPractice};
#[cfg(test)]
mod learning_tests;
#[cfg(test)]
mod practice_tests;
pub use storage::{
    all_skills, insert_skills, register_components, remove_skills, skills, skills_mut,
};

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
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        let cap = self
            .per_level
            .0
            .checked_mul(u64::from(self.max_level))
            .ok_or(Error::Overflow)?;
        if self.per_level.0 == 0
            || !(1..=1_000).contains(&self.max_level)
            || u64::from(self.max_level) * u64::from(self.work_bonus_per_level) > 1_000
            || u64::from(self.max_level) * u64::from(self.damage_bonus_per_level) > 1_000
            || self.action_practice_ms == 0
            || self.action_practice_ms > cap
        {
            return Err(Error::InvalidConfiguration("invalid skill rules".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub practice_ms: u64,
    pub level: u16,
    pub progress_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skills {
    pub records: BTreeMap<Skill, Progress>,
    pub semantic_ticks: BTreeMap<Skill, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
}
/// Borrowed mastery consumes the owning feature's configured bonuses. A caller
/// without a Skills runtime receives these same owner defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BonusRates {
    pub work_bonus_per_level: u16,
    pub damage_bonus_per_level: u16,
}
pub fn bonus_rates(runtime: Option<&Runtime>) -> BonusRates {
    let defaults;
    let rules = match runtime {
        Some(runtime) => &runtime.rules,
        None => {
            defaults = Rules::default();
            &defaults
        }
    };
    BonusRates {
        work_bonus_per_level: rules.work_bonus_per_level,
        damage_bonus_per_level: rules.damage_bonus_per_level,
    }
}
/// Learning earned by an actual interval of activity, supplied by its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PracticeDelta {
    pub actor: PersonId,
    pub skill: Skill,
    pub milliseconds: u64,
    pub mastery_cap: Option<u64>,
}

impl Runtime {
    pub fn new(rules: Rules) -> Result<Self, Error> {
        rules.validate()?;
        Ok(Self { rules })
    }

    /// Validate this owner's rows against its rules and the supplied clock.
    /// Person membership is a host fact.
    pub fn validate(&self, store: &Store, now: SimTime) -> Result<(), Error> {
        self.rules.validate()?;
        for (_, row) in all_skills(store) {
            row.validate_progress(&self.rules)?;
            if row
                .semantic_ticks
                .values()
                .any(|tick| *tick > now.0 / MINUTE.0)
            {
                return Err(Error::InvalidSave("future semantic skill credit".into()));
            }
        }
        Ok(())
    }

    pub fn apply_practice(&self, store: &mut Store, practice: PracticeDelta) -> Result<(), Error> {
        self.credit(
            store,
            practice.actor,
            practice.skill,
            practice.milliseconds,
            practice.mastery_cap,
        )
    }

    /// Social actions have a bounded learning rate even when several people
    /// publish the same achievement. Physical strikes retain individual credit.
    pub fn credit_action(
        &self,
        store: &mut Store,
        actor: PersonId,
        skill: Skill,
        milliseconds: u64,
        now: SimTime,
    ) -> Result<(), Error> {
        if milliseconds == 0 {
            return Ok(());
        }
        self.rules.validate()?;
        let mut row = skills(store, actor).cloned().unwrap_or_default();
        row.validate_progress(&self.rules)?;
        let semantic = matches!(skill, Skill::Negotiation | Skill::Leadership);
        if semantic {
            let tick = now.0 / MINUTE.0;
            if row
                .semantic_ticks
                .get(&skill)
                .is_some_and(|previous| *previous >= tick)
            {
                return Ok(());
            }
            row.semantic_ticks.insert(skill, tick);
        }
        if !row.credit(skill, milliseconds, None, &self.rules) && !semantic {
            return Ok(());
        }
        insert_skills(store, actor, row)
    }
    /// Time to a live mastery change during continuous practice. Guides cannot
    /// teach across a level their own accumulated mastery has not reached.
    pub fn until_next_level(
        &self,
        store: &Store,
        actor: PersonId,
        skill: Skill,
        mastery_cap: Option<u64>,
    ) -> Result<Option<u64>, Error> {
        self.rules.validate()?;
        if let Some(row) = skills(store, actor) {
            row.validate_progress(&self.rules)?;
        }
        let practice = self.practice(store, actor, skill);
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
    pub fn level(&self, store: &Store, actor: PersonId, skill: Skill) -> u16 {
        skills(store, actor).map_or(0, |row| row.level(skill))
    }

    pub fn practice(&self, store: &Store, actor: PersonId, skill: Skill) -> u64 {
        skills(store, actor).map_or(0, |row| row.practice(skill))
    }

    pub fn credit(
        &self,
        store: &mut Store,
        actor: PersonId,
        skill: Skill,
        milliseconds: u64,
        mastery_cap: Option<u64>,
    ) -> Result<(), Error> {
        if milliseconds == 0 {
            return Ok(());
        }
        self.rules.validate()?;
        let mut row = skills(store, actor).cloned().unwrap_or_default();
        row.validate_progress(&self.rules)?;
        if !row.credit(skill, milliseconds, mastery_cap, &self.rules) {
            return Ok(());
        }
        insert_skills(store, actor, row)
    }
}

impl Skills {
    /// Intrinsic checks do not capture the current rules or simulation clock.
    pub fn validate(&self) -> Result<(), Error> {
        if self.records.values().any(|record| {
            if record.level == 0 {
                record.progress_ms != record.practice_ms
            } else {
                record.progress_ms >= record.practice_ms
            }
        }) || self
            .semantic_ticks
            .keys()
            .any(|skill| !matches!(skill, Skill::Negotiation | Skill::Leadership))
        {
            return Err(Error::InvalidSave(
                "invalid personal skill component".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_progress(&self, rules: &Rules) -> Result<(), Error> {
        rules.validate()?;
        self.validate()?;
        let cap = rules.per_level.0 * u64::from(rules.max_level);
        if self.records.values().any(|record| {
            record.practice_ms > cap || record != &calculated_progress(record.practice_ms, rules)
        }) {
            return Err(Error::InvalidSave(
                "inconsistent personal skill progress".into(),
            ));
        }
        Ok(())
    }

    pub fn level(&self, skill: Skill) -> u16 {
        self.records.get(&skill).map_or(0, |record| record.level)
    }

    pub fn practice(&self, skill: Skill) -> u64 {
        self.records
            .get(&skill)
            .map_or(0, |record| record.practice_ms)
    }

    // Runtime validates the opening row and rules before calculating changes.
    fn credit(
        &mut self,
        skill: Skill,
        milliseconds: u64,
        mastery_cap: Option<u64>,
        rules: &Rules,
    ) -> bool {
        let cap = rules.per_level.0 * u64::from(rules.max_level);
        let existing = self.practice(skill);
        // A guide cannot teach beyond their personal mastery. A pupil with more
        // experience retains it; an inadequate guide never subtracts learning.
        let cap = mastery_cap.map_or(cap, |teacher| cap.min(teacher.max(existing)));
        let credited =
            (u128::from(existing) + u128::from(milliseconds)).min(u128::from(cap)) as u64;
        if credited == existing {
            return false;
        }
        self.records
            .insert(skill, calculated_progress(credited, rules));
        true
    }
}

pub fn progress(practice_ms: u64, rules: &Rules) -> Result<Progress, Error> {
    rules.validate()?;
    Ok(calculated_progress(practice_ms, rules))
}

fn calculated_progress(practice_ms: u64, rules: &Rules) -> Progress {
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

/// Borrowed mastery supplies an effective ability without changing personal learning.
pub fn effective_level(personal: u16, borrowed: u16) -> u16 {
    personal.max(borrowed)
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
mod tests;

/// Receiving effects for interruption of a linked personal lesson.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonInterruption {
    pub deadlines: Vec<PersonId>,
    pub idle: Vec<PersonId>,
}
/// Partners are factual activity links. Skills owns variant-only clearing and
/// removal of both participants' physical deadlines even after one moves on.
pub fn lesson_interruption(
    partners: Option<(PersonId, PersonId)>,
    mut is_lesson: impl FnMut(PersonId) -> bool,
) -> LessonInterruption {
    let deadlines = partners
        .map(|(teacher, student)| vec![teacher, student])
        .unwrap_or_default();
    let idle = deadlines
        .iter()
        .copied()
        .filter(|actor| is_lesson(*actor))
        .collect();
    LessonInterruption { deadlines, idle }
}
#[cfg(test)]
mod interruption_tests {
    use super::*;
    #[test]
    fn linked_lesson_removes_both_deadlines_but_clears_only_current_lesson() {
        let effect = lesson_interruption(Some((PersonId(1), PersonId(2))), |actor| {
            actor == PersonId(2)
        });
        assert_eq!(effect.deadlines, vec![PersonId(1), PersonId(2)]);
        assert_eq!(effect.idle, vec![PersonId(2)]);
        assert_eq!(
            lesson_interruption(None, |_| panic!("absent lesson")),
            LessonInterruption {
                deadlines: vec![],
                idle: vec![]
            }
        );
    }
}

pub mod receipt_validation;

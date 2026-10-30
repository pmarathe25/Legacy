//! Explicit lessons complement work experience without copying another person's records.
use crate::{Event, Rule, Skills};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub teacher: u64,
    pub student: u64,
    pub recipe: u32,
    pub tick: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_life: Option<u64>,
}

pub(crate) fn counts(lessons: &[Lesson]) -> HashMap<(u64, u32), u64> {
    if lessons.is_empty() {
        return HashMap::new();
    }
    let mut keys: Vec<_> = lessons
        .par_iter()
        .map(|row| (row.student, row.recipe))
        .collect();
    keys.par_sort_unstable();
    keys.par_chunk_by(|a, b| a == b)
        .map(|group| (group[0], group.len() as u64))
        .collect()
}

pub(crate) fn validate(
    lessons: &[Lesson],
    people: &HashSet<u64>,
    rules: &HashMap<u32, &Rule>,
    now: u64,
) -> Result<(), String> {
    let mut keys: Vec<_> = lessons
        .par_iter()
        .map(|row| (row.student, row.recipe, row.tick, row.teacher))
        .collect();
    keys.par_sort_unstable();
    if keys.par_windows(2).any(|pair| pair[0] == pair[1])
        || lessons.par_iter().any(|lesson| {
            lesson.tick > now
                || lesson.teacher == lesson.student
                || !people.contains(&lesson.teacher)
                || !people.contains(&lesson.student)
                || lesson
                    .source_life
                    .is_some_and(|life| !people.contains(&life))
                || !rules.contains_key(&lesson.recipe)
        })
        || counts(lessons).par_iter().any(|(&(_, recipe), &count)| {
            count > rules[&recipe].threshold(rules[&recipe].max_level)
        })
    {
        return Err("invalid skill lesson".into());
    }
    Ok(())
}

impl Skills {
    /// Add one lesson after a consenting encounter. The host supplies the teacher's
    /// committed effective rate, including any actively invoked capability.
    pub fn teach(
        &mut self,
        teacher: u64,
        student: u64,
        recipe: u32,
        tick: u64,
        teacher_rate: u64,
        source_life: Option<u64>,
    ) -> Result<(), &'static str> {
        let recipe = self.canonical(recipe);
        let rule = self
            .rules
            .iter()
            .find(|rule| rule.recipe == recipe)
            .ok_or("skill is not teachable")?;
        if teacher == student || teacher_rate <= self.index().rate(student, recipe) {
            return Err("teacher must have a higher skill rate");
        }
        if self.lessons.iter().any(|lesson| {
            lesson.teacher == teacher
                && lesson.student == student
                && lesson.recipe == recipe
                && lesson.tick == tick
        }) {
            return Err("lesson already recorded");
        }
        let worked = self
            .records
            .iter()
            .find(|record| record.person == student && record.recipe == recipe)
            .map_or(0, |record| record.completions);
        let taught = self
            .lessons
            .iter()
            .filter(|lesson| lesson.student == student && lesson.recipe == recipe)
            .count() as u64;
        let before = rule.level(worked.saturating_add(taught));
        let after = rule.level(worked.saturating_add(taught).saturating_add(1));
        if before == rule.max_level {
            return Err("student has mastered this skill");
        }
        self.lessons.push(Lesson {
            teacher,
            student,
            recipe,
            tick,
            source_life,
        });
        if after > before {
            self.events.push(Event {
                tick,
                person: student,
                recipe,
                previous_level: before,
                level: after,
            });
        }
        Ok(())
    }
}

//! Optional teaching choices made during social encounters.
use crate::{Event, Lesson, Rule, Skills, teaching};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherPolicy {
    pub person: u64,
    pub recipe: u32,
    pub enabled: bool,
    #[serde(default)]
    pub eligible_at: u64,
}

pub struct TeachingDraft {
    lessons: Vec<Lesson>,
    events: Vec<Event>,
}
impl TeachingDraft {
    pub fn commit(self, skills: &mut Skills) {
        skills.lessons.par_extend(self.lessons);
        skills.events.par_extend(self.events);
    }
}

pub(crate) fn validate(
    policies: &[TeacherPolicy],
    people: &HashSet<u64>,
    rules: &HashMap<u32, &Rule>,
) -> Result<(), String> {
    let mut keys: Vec<_> = policies
        .par_iter()
        .map(|row| (row.person, row.recipe))
        .collect();
    keys.par_sort_unstable();
    if keys.par_windows(2).any(|pair| pair[0] == pair[1])
        || policies
            .par_iter()
            .any(|row| !people.contains(&row.person) || !rules.contains_key(&row.recipe))
    {
        return Err("invalid teaching policy".into());
    }
    Ok(())
}

impl Skills {
    /// Evaluate disjoint encounter pairs against tick-start skill.
    /// Each pair can transfer one lesson; the controlled ruler is excluded.
    pub fn plan_teaching(
        &self,
        pairs: &[(u64, u64)],
        controlled: Option<u64>,
        tick: u64,
    ) -> Option<TeachingDraft> {
        self.plan_teaching_with(pairs, controlled, tick, |_, _, enabled| enabled)
    }
    /// The host supplies each teacher's own response to their current partner.
    pub fn plan_teaching_with(
        &self,
        pairs: &[(u64, u64)],
        controlled: Option<u64>,
        tick: u64,
        willing: impl Fn(u64, u64, bool) -> bool + Sync,
    ) -> Option<TeachingDraft> {
        if (self.records.is_empty() && self.lessons.is_empty())
            || pairs.is_empty()
            || self
                .teachers
                .par_iter()
                .all(|policy| policy.eligible_at > tick)
        {
            return None;
        }
        let mut participants: Vec<_> = pairs.par_iter().flat_map_iter(|&(a, b)| [a, b]).collect();
        participants.par_sort_unstable();
        if participants.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        let mut policies: Vec<_> = self
            .teachers
            .par_iter()
            .filter(|policy| policy.eligible_at <= tick)
            .collect();
        policies.par_sort_unstable_by_key(|policy| (policy.person, policy.recipe));
        let rules: HashMap<_, _> = self.rules.iter().map(|rule| (rule.recipe, rule)).collect();
        let worked: HashMap<_, _> = self
            .records
            .par_iter()
            .map(|record| ((record.person, record.recipe), record.completions))
            .collect();
        let taught = teaching::counts(&self.lessons);
        let existing: HashSet<_> = self
            .lessons
            .par_iter()
            .filter(|lesson| lesson.tick == tick)
            .map(|lesson| (lesson.teacher, lesson.student, lesson.recipe))
            .collect();
        let index = self.index();
        let mut results: Vec<_> = pairs
            .par_iter()
            .filter_map(|&(a, b)| {
                if controlled == Some(a) || controlled == Some(b) {
                    return None;
                }
                let mut choice = None;
                for (teacher, student) in [(a, b), (b, a)] {
                    let start = policies.partition_point(|policy| policy.person < teacher);
                    let end = policies.partition_point(|policy| policy.person <= teacher);
                    for policy in &policies[start..end] {
                        if !willing(teacher, student, policy.enabled)
                            || existing.contains(&(teacher, student, policy.recipe))
                        {
                            continue;
                        }
                        let rate = index.rate(teacher, policy.recipe);
                        if rate <= index.rate(student, policy.recipe) {
                            continue;
                        }
                        let candidate = (std::cmp::Reverse(rate), teacher, policy.recipe, student);
                        if choice.is_none_or(|old| candidate < old) {
                            choice = Some(candidate);
                        }
                    }
                }
                let (_, teacher, recipe, student) = choice?;
                let rule = rules[&recipe];
                let credits = worked
                    .get(&(student, recipe))
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(taught.get(&(student, recipe)).copied().unwrap_or(0));
                let before = rule.level(credits);
                let after = rule.level(credits.saturating_add(1));
                if before >= rule.max_level {
                    return None;
                }
                let lesson = Lesson {
                    teacher,
                    student,
                    recipe,
                    tick,
                    source_life: None,
                };
                let event = (after > before).then_some(Event {
                    tick,
                    person: student,
                    recipe,
                    previous_level: before,
                    level: after,
                });
                Some((lesson, event))
            })
            .collect();
        if results.is_empty() {
            return None;
        }
        results.par_sort_unstable_by_key(|(lesson, _)| {
            (lesson.student, lesson.recipe, lesson.teacher)
        });
        let (lessons, events): (Vec<_>, Vec<_>) = results.into_par_iter().unzip();
        Some(TeachingDraft {
            lessons,
            events: events.into_par_iter().flatten().collect(),
        })
    }
}

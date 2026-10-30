//! Personal, work-attributed learning with bounded history and diminishing returns.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
mod automatic;
mod teaching;
pub use automatic::{TeacherPolicy, TeachingDraft};
pub use game_parallel_skill_model::{
    Alias, Attribution, CatalogTraining, Completion, Event, Record, Rule,
};
pub use teaching::Lesson;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skills {
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Alias>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_training: Option<CatalogTraining>,
    #[serde(default)]
    pub records: Vec<Record>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub lessons: Vec<Lesson>,
    #[serde(default)]
    pub teachers: Vec<TeacherPolicy>,
    #[serde(default)]
    pub inherit_teachers: bool,
}

/// Immutable committed rates. Unconfigured or untrained work advances one unit.
pub struct Index {
    rates: HashMap<(u64, u32), u64>,
    aliases: HashMap<u32, u32>,
}
impl Index {
    pub fn rate(&self, person: u64, recipe: u32) -> u64 {
        let recipe = self.aliases.get(&recipe).copied().unwrap_or(recipe);
        self.rates.get(&(person, recipe)).copied().unwrap_or(1)
    }
}

impl Skills {
    pub fn canonical(&self, recipe: u32) -> u32 {
        self.aliases
            .iter()
            .find(|alias| alias.recipe == recipe)
            .map_or(recipe, |alias| alias.canonical)
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        recipes: &HashSet<u32>,
        now: u64,
    ) -> Result<(), String> {
        let mut rules = HashMap::new();
        for rule in &self.rules {
            if !recipes.contains(&rule.recipe)
                || rule.completions_per_level == 0
                || rule.max_level == 0
                || rule.work_per_level == 0
                || rules.insert(rule.recipe, rule).is_some()
            {
                return Err("invalid skill rule".into());
            }
        }
        if self.catalog_training.as_ref().is_some_and(|config| {
            config.completions_per_level == 0 || config.max_level == 0 || config.work_per_level == 0
        }) {
            return Err("invalid catalog skill training rules".into());
        }
        let mut aliases = HashSet::new();
        if self.aliases.iter().any(|alias| {
            !recipes.contains(&alias.recipe)
                || !rules.contains_key(&alias.canonical)
                || rules.contains_key(&alias.recipe)
                || !aliases.insert(alias.recipe)
        }) {
            return Err("invalid skill category mapping".into());
        }
        let mut seen = HashSet::new();
        for record in &self.records {
            let Some(rule) = rules.get(&record.recipe) else {
                return Err("unknown learned skill".into());
            };
            let cap = rule.threshold(rule.max_level);
            if !people.contains(&record.person)
                || !seen.insert((record.person, record.recipe))
                || record.completions == 0
                || record.completions > cap
                || (record.completions == cap && !record.credited_jobs.is_empty())
                || (record.completions < cap
                    && record.completions != record.credited_jobs.len() as u64)
                || record.credited_jobs.windows(2).any(|ids| ids[0] >= ids[1])
                || record
                    .credited_jobs
                    .iter()
                    .any(|credit| credit.finished_at > now)
            {
                return Err("invalid personal skill attribution".into());
            }
        }
        if self.events.par_iter().any(|event| {
            event.tick > now
                || !people.contains(&event.person)
                || rules.get(&event.recipe).is_none_or(|rule| {
                    event.level > rule.max_level || event.level <= event.previous_level
                })
        }) {
            return Err("invalid skill event".into());
        }
        teaching::validate(&self.lessons, people, &rules, now)?;
        automatic::validate(&self.teachers, people, &rules)?;
        Ok(())
    }

    /// Build once per tick; readers can query this index concurrently.
    pub fn index(&self) -> Index {
        let rules: HashMap<_, _> = self.rules.iter().map(|rule| (rule.recipe, rule)).collect();
        let aliases = self
            .aliases
            .iter()
            .map(|alias| (alias.recipe, alias.canonical))
            .collect();
        if self.lessons.is_empty() {
            let rates = self
                .records
                .par_iter()
                .filter_map(|record| {
                    let rule = rules[&record.recipe];
                    let level = rule.level(record.completions);
                    (level > 0).then_some((
                        (record.person, record.recipe),
                        1 + u64::from(level) * u64::from(rule.work_per_level),
                    ))
                })
                .collect();
            return Index { rates, aliases };
        }
        let lessons = teaching::counts(&self.lessons);
        let present: HashSet<_> = self
            .records
            .par_iter()
            .map(|record| (record.person, record.recipe))
            .collect();
        let rates = self
            .records
            .par_iter()
            .filter_map(|record| {
                let rule = rules[&record.recipe];
                let level = rule.level(
                    record.completions.saturating_add(
                        lessons
                            .get(&(record.person, record.recipe))
                            .copied()
                            .unwrap_or(0),
                    ),
                );
                (level > 0).then_some((
                    (record.person, record.recipe),
                    1 + u64::from(level) * u64::from(rule.work_per_level),
                ))
            })
            .chain(
                lessons
                    .par_iter()
                    .filter_map(|(&(person, recipe), &count)| {
                        if present.contains(&(person, recipe)) {
                            return None;
                        }
                        let rule = rules[&recipe];
                        let level = rule.level(count);
                        (level > 0).then_some((
                            (person, recipe),
                            1 + u64::from(level) * u64::from(rule.work_per_level),
                        ))
                    }),
            )
            .collect();
        Index { rates, aliases }
    }

    /// Stage credits from newly finished jobs, never repeated storage-delivery attempts.
    /// The host supplies validated identities and publishes the returned state only
    /// after every owner prepares successfully. Repeated attribution is idempotent.
    pub fn prepare(&self, completions: &[Completion], tick: u64) -> Self {
        let rules: HashMap<_, _> = self.rules.iter().map(|rule| (rule.recipe, rule)).collect();
        let aliases: HashMap<_, _> = self
            .aliases
            .iter()
            .map(|alias| (alias.recipe, alias.canonical))
            .collect();
        let lessons = teaching::counts(&self.lessons);
        let old: HashMap<_, _> = self
            .records
            .par_iter()
            .map(|record| ((record.person, record.recipe), record))
            .collect();
        let mut credits: Vec<_> = completions
            .par_iter()
            .filter_map(|credit| {
                let mut credit = *credit;
                credit.recipe = aliases
                    .get(&credit.recipe)
                    .copied()
                    .unwrap_or(credit.recipe);
                rules.contains_key(&credit.recipe).then_some(credit)
            })
            .collect();
        credits.par_sort_unstable_by_key(|credit| {
            (credit.worker, credit.recipe, credit.finished_at, credit.job)
        });
        let updated: Vec<_> = credits
            .par_chunk_by(|a, b| (a.worker, a.recipe) == (b.worker, b.recipe))
            .map(|group| {
                let key = (group[0].worker, group[0].recipe);
                let rule = rules[&key.1];
                let prior = old.get(&key).copied();
                let cap = rule.threshold(rule.max_level);
                let before = prior.map_or(0, |record| record.completions);
                let mut record = Record {
                    person: key.0,
                    recipe: key.1,
                    completions: before,
                    credited_jobs: Vec::new(),
                };
                if before < cap {
                    let previous = prior.map_or(&[][..], |record| record.credited_jobs.as_slice());
                    let (mut i, mut j) = (0, 0);
                    while (i < previous.len() || j < group.len()) && record.completions < cap {
                        let incoming = group.get(j).map(|credit| Attribution {
                            finished_at: credit.finished_at,
                            job: credit.job,
                        });
                        let next = match (previous.get(i), incoming) {
                            (Some(&a), Some(b)) if a <= b => {
                                i += 1;
                                a
                            }
                            (_, Some(b)) => {
                                j += 1;
                                b
                            }
                            (Some(&a), None) => {
                                i += 1;
                                a
                            }
                            (None, None) => break,
                        };
                        if record.credited_jobs.last() != Some(&next) {
                            record.credited_jobs.push(next);
                            record.completions = record.credited_jobs.len() as u64;
                        }
                    }
                    if record.completions == cap {
                        record.credited_jobs.clear();
                    }
                }
                let taught = lessons.get(&key).copied().unwrap_or(0);
                let previous_level = rule.level(before.saturating_add(taught));
                let level = rule.level(record.completions.saturating_add(taught));
                let event = (level > previous_level).then_some(Event {
                    tick,
                    person: key.0,
                    recipe: key.1,
                    previous_level,
                    level,
                });
                (record, event)
            })
            .collect();
        let changed: HashSet<_> = updated
            .par_iter()
            .map(|(r, _)| (r.person, r.recipe))
            .collect();
        let mut records: Vec<_> = self
            .records
            .par_iter()
            .filter(|r| !changed.contains(&(r.person, r.recipe)))
            .cloned()
            .collect();
        let (learned, events): (Vec<_>, Vec<_>) = updated.into_par_iter().unzip();
        records.par_extend(learned);
        let events = events.into_par_iter().flatten().collect();
        records.par_sort_unstable_by_key(|record| (record.person, record.recipe));
        Self {
            rules: self.rules.clone(),
            aliases: self.aliases.clone(),
            catalog_training: self.catalog_training.clone(),
            records,
            events,
            lessons: self.lessons.par_iter().cloned().collect(),
            teachers: self.teachers.par_iter().cloned().collect(),
            inherit_teachers: self.inherit_teachers,
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;

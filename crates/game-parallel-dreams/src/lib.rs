//! Enduring aspirations periodically create concrete, physically achieved goals.
use game_parallel_goals::{Goal, Goals};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dream {
    pub person: u64,
    pub recipe: u32,
    pub priority: i32,
    pub batch: u64,
    pub review_interval: u64,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
/// A person creates or revises their own enduring aspiration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetDream {
    pub person: u64,
    pub recipe: u32,
    pub priority: i32,
    pub batch: u64,
    pub review_interval: u64,
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub recipe: u32,
    pub target: u64,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dreams {
    pub people: Vec<Dream>,
    /// Allow the composed host to enroll actual newborns with a parent's general inclination.
    #[serde(default)]
    pub inherit_births: bool,
    /// Individual inclinations to develop aspirations from personally witnessed work.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub learning: Vec<Learning>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Dreams {
    fn clone(&self) -> Self {
        Self {
            people: self.people.par_iter().cloned().collect(),
            inherit_births: self.inherit_births,
            learning: self.learning.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Learning {
    pub person: u64,
    pub max_aspirations: usize,
    pub priority: i32,
    pub batch: u64,
    pub review_interval: u64,
}
pub struct Draft {
    pub state: Dreams,
    pub goals: Option<Goals>,
}
struct Review {
    next_review: u64,
    change: Option<(Option<usize>, Goal)>,
}
impl Dreams {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        recipes: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), &'static str> {
        let mut keys: Vec<_> = self
            .people
            .par_iter()
            .map(|dream| (dream.person, dream.recipe))
            .collect();
        keys.par_sort_unstable();
        if keys.par_windows(2).any(|pair| pair[0] == pair[1])
            || {
                let mut learners: Vec<_> = self.learning.par_iter().map(|row| row.person).collect();
                learners.par_sort_unstable();
                learners.par_windows(2).any(|pair| pair[0] == pair[1])
                    || self.learning.par_iter().any(|row| {
                        !people.contains(&row.person)
                            || row.max_aspirations == 0
                            || row.max_aspirations > 32
                            || row.batch == 0
                            || row.review_interval == 0
                    })
            }
            || self.people.par_iter().any(|dream| {
                !people.contains(&dream.person)
                    || !recipes.contains(&dream.recipe)
                    || dream.batch == 0
                    || dream.review_interval == 0
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || !people.contains(&event.person)
                    || !recipes.contains(&event.recipe)
                    || event.target == 0
            })
        {
            return Err("invalid personal aspiration");
        }
        Ok(())
    }
    /// Learn only from observations supplied by the host, with a bounded personal repertoire.
    pub fn learn(&mut self, observations: &[(u64, u32)], tick: u64) {
        if self.learning.is_empty() || observations.is_empty() {
            return;
        }
        let mut known: Vec<_> = self
            .people
            .par_iter()
            .map(|d| (d.person, d.recipe))
            .collect();
        known.par_sort_unstable();
        let mut observed: Vec<_> = observations.par_iter().copied().collect();
        observed.par_sort_unstable();
        let mut learners: Vec<_> = self.learning.par_iter().collect();
        if learners
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            learners.par_sort_unstable_by_key(|rule| rule.person);
        }
        let learned: Vec<_> = observed
            .par_chunk_by(|a, b| a.0 == b.0)
            .filter_map(|group| {
                let index = learners
                    .binary_search_by_key(&group[0].0, |rule| rule.person)
                    .ok()?;
                let rule = learners[index];
                let start = known.partition_point(|row| row.0 < rule.person);
                let end = known.partition_point(|row| row.0 <= rule.person);
                let capacity = rule.max_aspirations.saturating_sub(end - start);
                Some((group, rule, &known[start..end], capacity))
            })
            .flat_map_iter(|(group, rule, known, capacity)| {
                group
                    .iter()
                    .enumerate()
                    .filter(move |(index, row)| {
                        (*index == 0 || group[*index - 1].1 != row.1)
                            && known.binary_search(row).is_err()
                    })
                    .take(capacity)
                    .map(move |(_, &(_, recipe))| Dream {
                        person: rule.person,
                        recipe,
                        priority: rule.priority,
                        batch: rule.batch,
                        review_interval: rule.review_interval,
                        next_review: tick,
                        enabled: true,
                    })
            })
            .collect();
        self.people.par_extend(learned);
    }
    /// Stage reviews against goals from this boundary and committed personal agency.
    pub fn prepare(
        &self,
        goals: &Goals,
        tick: u64,
        alive: impl Fn(u64) -> bool + Sync,
    ) -> Result<Option<Draft>, &'static str> {
        if !self
            .people
            .par_iter()
            .any(|dream| dream.enabled && dream.next_review <= tick && alive(dream.person))
        {
            return Ok(None);
        }
        let mut keys: Vec<_> = goals
            .people
            .par_iter()
            .enumerate()
            .map(|(i, goal)| ((goal.person, goal.recipe), i))
            .collect();
        if keys.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            keys.par_sort_unstable_by_key(|row| row.0);
        }
        let reviews: Vec<_> = self
            .people
            .par_iter()
            .map(|dream| -> Result<Option<Review>, &'static str> {
                if !dream.enabled || dream.next_review > tick || !alive(dream.person) {
                    return Ok(None);
                }
                let next_review = tick
                    .checked_add(dream.review_interval)
                    .ok_or("aspiration review overflow")?;
                let existing = keys
                    .binary_search_by_key(&(dream.person, dream.recipe), |row| row.0)
                    .ok()
                    .map(|i| keys[i].1);
                let change = match existing {
                    Some(i)
                        if !goals.people[i].enabled
                            || goals.people[i].progress < goals.people[i].target =>
                    {
                        None
                    }
                    Some(i) => {
                        let mut goal = goals.people[i].clone();
                        goal.target = goal
                            .progress
                            .checked_add(dream.batch)
                            .ok_or("aspiration target overflow")?;
                        goal.priority = dream.priority;
                        goal.next_review = 0;
                        Some((Some(i), goal))
                    }
                    None => Some((
                        None,
                        Goal {
                            person: dream.person,
                            recipe: dream.recipe,
                            priority: dream.priority,
                            target: dream.batch,
                            progress: 0,
                            next_review: 0,
                            enabled: true,
                        },
                    )),
                };
                Ok(Some(Review {
                    next_review,
                    change,
                }))
            })
            .collect::<Result<_, _>>()?;
        let people = self
            .people
            .par_iter()
            .zip(reviews.par_iter())
            .map(|(dream, review)| {
                let mut next = dream.clone();
                if let Some(review) = review {
                    next.next_review = review.next_review;
                }
                next
            })
            .collect();
        let events = reviews
            .par_iter()
            .filter_map(|review| review.as_ref()?.change.as_ref())
            .map(|(_, goal)| Event {
                tick,
                person: goal.person,
                recipe: goal.recipe,
                target: goal.target,
            })
            .collect();
        let mut patches: Vec<_> = reviews
            .par_iter()
            .filter_map(|review| {
                let (index, goal) = review.as_ref()?.change.as_ref()?;
                index.map(|index| (index, goal.clone()))
            })
            .collect();
        patches.par_sort_unstable_by_key(|patch| patch.0);
        let appended: Vec<_> = reviews
            .par_iter()
            .filter_map(|review| {
                let (index, goal) = review.as_ref()?.change.as_ref()?;
                index.is_none().then(|| goal.clone())
            })
            .collect();
        let next_goals = if patches.is_empty() && appended.is_empty() {
            None
        } else {
            let mut updated = goals.clone();
            updated
                .people
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, goal)| {
                    if let Ok(i) = patches.binary_search_by_key(&index, |patch| patch.0) {
                        *goal = patches[i].1.clone();
                    }
                });
            updated.people.extend(appended);
            Some(updated)
        };
        Ok(Some(Draft {
            state: Dreams {
                people,
                inherit_births: self.inherit_births,
                learning: self.learning.par_iter().cloned().collect(),
                events,
            },
            goals: next_goals,
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learning_respects_personal_boundaries_and_existing_choices() {
        let mut rejected = dream(1, 2);
        rejected.enabled = false;
        let mut state = Dreams {
            inherit_births: false,
            people: vec![rejected],
            learning: vec![Learning {
                person: 1,
                max_aspirations: 2,
                priority: 4,
                batch: 1,
                review_interval: 3,
            }],
            events: vec![],
        };
        state.learn(&[(2, 1), (1, 3), (1, 2), (1, 3), (1, 4)], 7);
        assert_eq!(state.people.len(), 2);
        assert!(!state.people[0].enabled);
        assert_eq!(
            (
                state.people[1].person,
                state.people[1].recipe,
                state.people[1].next_review
            ),
            (1, 3, 7)
        );
        state.learn(&[(1, 4)], 8);
        assert_eq!(state.people.len(), 2);
    }
    #[test]
    fn parallel_learning_produces_the_same_personal_aspirations() {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut state = Dreams {
                        inherit_births: false,
                        people: vec![],
                        learning: (1..=4096)
                            .map(|person| Learning {
                                person,
                                max_aspirations: 2,
                                priority: 1,
                                batch: 1,
                                review_interval: 3,
                            })
                            .collect(),
                        events: vec![],
                    };
                    let observations: Vec<_> = (1..=4096)
                        .rev()
                        .flat_map(|person| [(person, 3), (person, 1), (person, 2), (person, 1)])
                        .collect();
                    state.learn(&observations, 4);
                    assert_eq!(state.people.len(), 8192);
                    assert!(state.people.iter().all(|dream| dream.recipe <= 2));
                    state
                })
        };
        assert_eq!(run(1), run(4));
    }
    #[test]
    #[ignore = "performance measurement"]
    fn concentrated_learning_benchmark() {
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000u64);
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let initial = Dreams {
                    inherit_births: false,
                    people: vec![],
                    learning: (1..=count)
                        .map(|person| Learning {
                            person,
                            max_aspirations: 2,
                            priority: 1,
                            batch: 1,
                            review_interval: 3,
                        })
                        .collect(),
                    events: vec![],
                };
                let observations: Vec<_> = (1..=count).rev().map(|person| (person, 1)).collect();
                let mut samples = Vec::new();
                for round in 0..4 {
                    let mut state = initial.clone();
                    let start = std::time::Instant::now();
                    state.learn(&observations, 1);
                    let elapsed = start.elapsed().as_secs_f64();
                    assert_eq!(state.people.len(), count as usize);
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} learners={count} elapsed={:.6}",
                    samples[1]
                );
            });
    }
    fn dream(person: u64, recipe: u32) -> Dream {
        Dream {
            person,
            recipe,
            priority: 3,
            batch: 2,
            review_interval: 3,
            next_review: 0,
            enabled: true,
        }
    }
    fn goals() -> Goals {
        Goals {
            retry_interval: 2,
            plan_inputs: false,
            people: vec![],
            events: vec![],
        }
    }
    #[test]
    fn aspirations_create_and_renew_goals_without_overriding_refusal_or_progress() {
        let dreams = Dreams {
            inherit_births: false,
            learning: vec![],
            people: vec![dream(1, 4), dream(2, 4)],
            events: vec![],
        };
        let first = dreams.prepare(&goals(), 1, |id| id == 1).unwrap().unwrap();
        assert_eq!(first.state.events.len(), 1);
        let mut goals = first.goals.unwrap();
        assert_eq!((goals.people[0].target, goals.people[0].progress), (2, 0));
        assert!(first.state.prepare(&goals, 2, |_| true).unwrap().is_some()); // second person's review
        goals.people[0].progress = 1;
        let unfinished = first
            .state
            .prepare(&goals, 4, |id| id == 1)
            .unwrap()
            .unwrap();
        assert!(unfinished.goals.is_none());
        goals.people[0].progress = 2;
        let renewed = unfinished
            .state
            .prepare(&goals, 7, |id| id == 1)
            .unwrap()
            .unwrap();
        assert_eq!(renewed.goals.unwrap().people[0].target, 4);
        goals.people[0].enabled = false;
        let refused = renewed
            .state
            .prepare(&goals, 10, |id| id == 1)
            .unwrap()
            .unwrap();
        assert!(refused.goals.is_none());
    }
    #[test]
    fn aspiration_reviews_are_deterministic_and_overflow_is_rejected() {
        let dreams = Dreams {
            inherit_births: false,
            learning: vec![],
            people: (1..=10_000).rev().map(|person| dream(person, 1)).collect(),
            events: vec![],
        };
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let draft = dreams.prepare(&goals(), 1, |_| true).unwrap().unwrap();
                    (draft.state, draft.goals.unwrap())
                })
        };
        assert_eq!(run(1), run(4));
        assert!(dreams.prepare(&goals(), u64::MAX, |_| true).is_err());
    }
}

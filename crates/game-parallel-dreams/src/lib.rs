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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dreams {
    pub people: Vec<Dream>,
    #[serde(default)]
    pub events: Vec<Event>,
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
            state: Dreams { people, events },
            goals: next_goals,
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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

//! Personal craft goals, resolved through real production work.
pub mod planning;
use game_parallel_autowork::{Autowork, Policy};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goals {
    #[serde(default = "retry_interval")]
    pub retry_interval: u64,
    /// Plan available recipes for missing local inputs before the final job.
    #[serde(default)]
    pub plan_inputs: bool,
    pub people: Vec<Goal>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub person: u64,
    pub recipe: u32,
    pub priority: i32,
    pub target: u64,
    #[serde(default)]
    pub progress: u64,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
fn retry_interval() -> u64 {
    3
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub recipe: u32,
    pub progress: u64,
    pub completed: bool,
    #[serde(default)]
    pub blocked: bool,
}
/// An individual changes their own work aspiration without resetting earned progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetGoal {
    pub person: u64,
    pub recipe: u32,
    pub priority: i32,
    pub target: u64,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub person: u64,
    pub recipe: u32,
}
impl Goals {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        recipes: &HashSet<u32>,
        now: u64,
    ) -> Result<(), &'static str> {
        let mut keys: Vec<_> = self
            .people
            .par_iter()
            .map(|g| (g.person, g.recipe))
            .collect();
        keys.par_sort_unstable();
        if self.retry_interval == 0 {
            return Err("invalid goal retry interval");
        }
        if keys.par_windows(2).any(|p| p[0] == p[1])
            || self.people.par_iter().any(|g| {
                !people.contains(&g.person)
                    || !recipes.contains(&g.recipe)
                    || g.target == 0
                    || g.progress > g.target
            })
            || self.events.par_iter().any(|e| {
                e.tick > now || !people.contains(&e.person) || !recipes.contains(&e.recipe)
            })
        {
            return Err("invalid work goal state");
        }
        Ok(())
    }
    /// Pick at most one active goal per person; authored direct policies win.
    pub fn policies(&self, base: &Autowork, now: u64) -> Autowork {
        let mut occupied: Vec<_> = base.policies.par_iter().map(|p| p.person).collect();
        if occupied.par_windows(2).any(|pair| pair[0] > pair[1]) {
            occupied.par_sort_unstable();
        }
        let mut goals: Vec<_> = self
            .people
            .par_iter()
            .filter(|g| {
                g.enabled
                    && g.progress < g.target
                    && g.next_review <= now
                    && occupied.binary_search(&g.person).is_err()
            })
            .collect();
        if goals.par_windows(2).any(|pair| {
            (
                pair[0].person,
                std::cmp::Reverse(pair[0].priority),
                pair[0].recipe,
            ) > (
                pair[1].person,
                std::cmp::Reverse(pair[1].priority),
                pair[1].recipe,
            )
        }) {
            goals.par_sort_unstable_by_key(|g| (g.person, std::cmp::Reverse(g.priority), g.recipe));
        }
        let policy = |goal: &Goal| Policy {
            person: goal.person,
            recipe: goal.recipe,
            enabled: true,
            disregard_claims: false,
        };
        let picked: Vec<_> = if goals
            .par_windows(2)
            .any(|pair| pair[0].person == pair[1].person)
        {
            goals
                .par_chunk_by(|a, b| a.person == b.person)
                .map(|group| policy(group[0]))
                .collect()
        } else {
            goals.par_iter().map(|goal| policy(goal)).collect()
        };
        let mut policies: Vec<_> = base.policies.par_iter().cloned().collect();
        policies.extend(picked);
        Autowork {
            next_id: base.next_id,
            policies,
            requests: None,
        }
    }
    /// Stage progress from completed production; no goal moves before actual work ends.
    pub fn prepare(
        &self,
        completed: &[Completion],
        blocked: &[Completion],
        tick: u64,
    ) -> Result<Option<Self>, &'static str> {
        if completed.is_empty() && blocked.is_empty() {
            return Ok(None);
        }
        let retry = if blocked.is_empty() {
            0
        } else {
            tick.checked_add(self.retry_interval)
                .ok_or("goal review overflow")?
        };
        let mut blocked: Vec<_> = blocked.par_iter().map(|c| (c.person, c.recipe)).collect();
        if blocked.par_windows(2).any(|pair| pair[0] > pair[1]) {
            blocked.par_sort_unstable();
        }
        let dense_blocked = blocked.first().and_then(|&(first, recipe)| {
            let last = blocked.last()?.0;
            (last.checked_sub(first) == Some((blocked.len() - 1) as u64)
                && blocked.par_iter().all(|&(_, r)| r == recipe)
                && blocked
                    .par_windows(2)
                    .all(|pair| pair[0].0.checked_add(1) == Some(pair[1].0)))
            .then_some((first, recipe))
        });
        let mut done: Vec<_> = completed.par_iter().map(|c| (c.person, c.recipe)).collect();
        if done.par_windows(2).any(|pair| pair[0] > pair[1]) {
            done.par_sort_unstable();
        }
        let counts: Vec<_> = done
            .par_chunk_by(|a, b| a == b)
            .map(|g| (g[0], g.len() as u64))
            .collect();
        let rows: Vec<_> = self
            .people
            .par_iter()
            .map(|goal| {
                let mut next = goal.clone();
                let count = counts
                    .binary_search_by_key(&(goal.person, goal.recipe), |c| c.0)
                    .ok()
                    .map_or(0, |i| counts[i].1);
                let event = if count > 0 && goal.enabled && goal.progress < goal.target {
                    next.progress = goal.progress.saturating_add(count).min(goal.target);
                    Some(Event {
                        tick,
                        person: goal.person,
                        recipe: goal.recipe,
                        progress: next.progress,
                        completed: next.progress == goal.target,
                        blocked: false,
                    })
                } else if dense_blocked.map_or_else(
                    || blocked.binary_search(&(goal.person, goal.recipe)).is_ok(),
                    |(first, recipe)| {
                        goal.recipe == recipe
                            && goal.person >= first
                            && goal.person - first < blocked.len() as u64
                    },
                ) && goal.enabled
                    && goal.progress < goal.target
                {
                    next.next_review = retry;
                    Some(Event {
                        tick,
                        person: goal.person,
                        recipe: goal.recipe,
                        progress: goal.progress,
                        completed: false,
                        blocked: true,
                    })
                } else {
                    None
                };
                (next, event)
            })
            .collect();
        let mut events: Vec<_> = rows.par_iter().filter_map(|row| row.1.clone()).collect();
        if events.is_empty() {
            return Ok(None);
        }
        if events
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].recipe) > (pair[1].person, pair[1].recipe))
        {
            events.par_sort_unstable_by_key(|e| (e.person, e.recipe));
        }
        Ok(Some(Self {
            retry_interval: self.retry_interval,
            plan_inputs: false,
            people: rows.into_par_iter().map(|row| row.0).collect(),
            events,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn goal(person: u64, recipe: u32, priority: i32, target: u64) -> Goal {
        Goal {
            person,
            recipe,
            priority,
            target,
            progress: 0,
            next_review: 0,
            enabled: true,
        }
    }
    #[test]
    fn completed_work_advances_the_selected_goal_and_blocking_selects_an_alternative() {
        let goals = Goals {
            retry_interval: 3,
            plan_inputs: false,
            people: vec![goal(1, 1, 1, 2), goal(1, 2, 3, 1), goal(2, 1, 1, 1)],
            events: vec![],
        };
        let base = Autowork {
            next_id: 1,
            requests: None,
            policies: vec![Policy {
                person: 2,
                recipe: 2,
                enabled: false,
                disregard_claims: false,
            }],
        };
        let first = goals.policies(&base, 0);
        assert_eq!(
            first
                .policies
                .iter()
                .find(|p| p.person == 1)
                .unwrap()
                .recipe,
            2
        );
        assert!(
            !first
                .policies
                .iter()
                .find(|p| p.person == 2)
                .unwrap()
                .enabled
        );
        let blocked = goals
            .prepare(
                &[],
                &[Completion {
                    person: 1,
                    recipe: 2,
                }],
                1,
            )
            .unwrap()
            .unwrap();
        assert!(blocked.events[0].blocked);
        assert_eq!(blocked.people[1].next_review, 4);
        assert_eq!(
            blocked
                .policies(&base, 1)
                .policies
                .iter()
                .find(|p| p.person == 1)
                .unwrap()
                .recipe,
            1
        );
        assert_eq!(
            blocked
                .policies(&base, 4)
                .policies
                .iter()
                .find(|p| p.person == 1)
                .unwrap()
                .recipe,
            2
        );
        let next = blocked
            .prepare(
                &[Completion {
                    person: 1,
                    recipe: 2,
                }],
                &[],
                5,
            )
            .unwrap()
            .unwrap();
        assert!(next.events[0].completed);
        assert_eq!(
            next.policies(&base, 5)
                .policies
                .iter()
                .find(|p| p.person == 1)
                .unwrap()
                .recipe,
            1
        );
        assert!(
            next.prepare(
                &[Completion {
                    person: 3,
                    recipe: 1
                }],
                &[],
                6
            )
            .unwrap()
            .is_none()
        );
        assert!(
            next.prepare(
                &[],
                &[Completion {
                    person: 1,
                    recipe: 1
                }],
                u64::MAX
            )
            .is_err()
        );
    }
    #[test]
    fn blocked_membership_handles_duplicate_and_sparse_identities() {
        let goals = Goals {
            retry_interval: 3,
            plan_inputs: false,
            people: (1..=3).map(|person| goal(person, 1, 0, 1)).collect(),
            events: vec![],
        };
        let next = goals
            .prepare(
                &[],
                &[
                    Completion {
                        person: 3,
                        recipe: 1,
                    },
                    Completion {
                        person: 1,
                        recipe: 1,
                    },
                    Completion {
                        person: 1,
                        recipe: 1,
                    },
                ],
                1,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            next.people
                .iter()
                .map(|g| g.next_review)
                .collect::<Vec<_>>(),
            vec![4, 0, 4]
        );
        assert_eq!(
            next.events.iter().map(|e| e.person).collect::<Vec<_>>(),
            vec![1, 3]
        );
    }
    #[test]
    fn population_selection_and_progress_are_worker_invariant() {
        let goals = Goals {
            retry_interval: 3,
            plan_inputs: false,
            people: (1..=10000)
                .rev()
                .map(|person| goal(person, 1, 0, 2))
                .collect(),
            events: vec![],
        };
        let base = Autowork {
            next_id: 1,
            requests: None,
            policies: vec![],
        };
        let completed: Vec<_> = (1..=5000)
            .map(|person| Completion { person, recipe: 1 })
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    (
                        goals.policies(&base, 0),
                        goals.prepare(&completed, &[], 1).unwrap().unwrap(),
                    )
                })
        };
        assert_eq!(run(1), run(4));
    }
}

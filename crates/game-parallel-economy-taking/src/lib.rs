//! Individual attempts to take real possessions, with independent resistance.
pub mod content;
mod reactions;
use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_genetics::random;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;
pub use reactions::{Interpretation, ObservedTaking, Reaction, Witness};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

/// An individual's possible action; no policy obliges anyone to act.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub willing: bool,
    pub interval: u64,
    #[serde(default)]
    pub escalate: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub affinity_at_least: Option<i32>,
    #[serde(default)]
    pub affinity_at_most: Option<i32>,
}

/// A target's own standing response, including a controlled person's preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub person: u64,
    #[serde(default)]
    pub notices: bool,
    #[serde(default)]
    pub resist: bool,
    #[serde(default)]
    pub resist_if_fear_at_least: Option<u64>,
    #[serde(default)]
    pub resist_if_affinity_at_least: Option<i32>,
    #[serde(default)]
    pub resist_if_affinity_at_most: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Transferred,
    Unavailable,
    Resisted,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub actor: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub noticed: bool,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Taking {
    #[serde(default)]
    pub policies: Arc<Vec<Policy>>,
    #[serde(default)]
    pub responses: Arc<Vec<Response>>,
    #[serde(default)]
    pub reactions: Arc<Vec<Reaction>>,
    #[serde(default)]
    pub witnesses: Arc<Vec<Witness>>,
    #[serde(default)]
    pub events: Vec<Event>,
}

fn detach<T: Clone + Send + Sync>(shared: &mut Arc<Vec<T>>) -> &mut Vec<T> {
    if Arc::get_mut(shared).is_none() {
        *shared = Arc::new(shared.par_iter().cloned().collect());
    }
    Arc::get_mut(shared).expect("exclusive taking generation")
}

impl Taking {
    pub fn policies_mut(&mut self) -> &mut Vec<Policy> {
        detach(&mut self.policies)
    }
    pub fn responses_mut(&mut self) -> &mut Vec<Response> {
        detach(&mut self.responses)
    }
    pub fn reactions_mut(&mut self) -> &mut Vec<Reaction> {
        detach(&mut self.reactions)
    }
    pub fn witnesses_mut(&mut self) -> &mut Vec<Witness> {
        detach(&mut self.witnesses)
    }

    /// Check content and save data before an owner participates in the world.
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u64,
    ) -> Result<(), &'static str> {
        let mut actors: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        actors.par_sort_unstable();
        let mut responders: Vec<_> = self.responses.par_iter().map(|p| p.person).collect();
        responders.par_sort_unstable();
        let mut reactions: Vec<_> = self
            .reactions
            .par_iter()
            .map(|p| (p.person, p.other))
            .collect();
        reactions.par_sort_unstable();
        let mut witnesses: Vec<_> = self
            .witnesses
            .par_iter()
            .map(|w| (w.person, w.actor))
            .collect();
        witnesses.par_sort_unstable();
        if actors.par_windows(2).any(|p| p[0] == p[1])
            || responders.par_windows(2).any(|p| p[0] == p[1])
            || reactions.par_windows(2).any(|p| p[0] == p[1])
            || witnesses.par_windows(2).any(|p| p[0] == p[1])
            || self.witnesses.par_iter().any(|w| {
                !people.contains(&w.person)
                    || !people.contains(&w.actor)
                    || w.person == w.actor
                    || w.target.is_some_and(|target| {
                        target == w.person || target == w.actor || !people.contains(&target)
                    })
                    || w.last_seen.is_some_and(|seen| {
                        seen.tick > now
                            || seen.target == w.person
                            || seen.target == w.actor
                            || !people.contains(&seen.target)
                            || !goods.contains(&seen.good)
                            || seen.quantity == 0
                    })
            })
            || self.reactions.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.other
                        .is_some_and(|other| other == p.person || !people.contains(&other))
            })
            || self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || !people.contains(&p.target)
                    || p.person == p.target
                    || !goods.contains(&p.good)
                    || p.quantity == 0
                    || p.interval == 0
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !people.contains(&id))
            })
            || self
                .responses
                .par_iter()
                .any(|p| !people.contains(&p.person))
            || self.events.par_iter().any(|e| {
                e.tick > now
                    || e.actor == e.target
                    || !people.contains(&e.actor)
                    || !people.contains(&e.target)
                    || !goods.contains(&e.good)
                    || e.quantity == 0
            })
        {
            return Err("invalid personal taking state");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub actor: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub escalate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contest {
    pub actor: u64,
    pub target: u64,
    pub region: u64,
}

struct Pending {
    intent: Intent,
    noticed: bool,
    opposed: bool,
    action: Option<u64>,
    region: u64,
}

/// Detached proposals. Goods and combat remain owned by their respective systems.
pub struct Plan {
    pub actors: Vec<u64>,
    /// All targets, including those who do not notice or spend an action.
    pub targets: Vec<u64>,
    pub actions: Vec<Transaction>,
    pub contests: Vec<Contest>,
    pending: Vec<Pending>,
    tick: u64,
    action_base: u64,
}

pub struct Context<'a> {
    pub owner: &'a Taking,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub locations: &'a Colocation,
    /// Sorted actors unavailable for initiating and participants already in conflict.
    pub busy: &'a [u64],
    pub blocked: &'a [u64],
    pub can_touch: &'a (dyn Fn(u64, u64) -> bool + Sync),
    pub max_hunger: u32,
    pub manual: Option<Intent>,
    pub action_base: u64,
    pub seed: u64,
    pub tick: u64,
}

impl Context<'_> {
    pub fn prepare(&self) -> Result<Plan, String> {
        let dense = self
            .people
            .first()
            .zip(self.people.last())
            .and_then(|(a, b)| {
                (b.id.checked_sub(a.id) == Some((self.people.len() - 1) as u64)).then_some(a.id)
            });
        let person = |id: u64| {
            let index = dense.map_or_else(
                || self.people.binary_search_by_key(&id, |p| p.id).ok(),
                |first| {
                    id.checked_sub(first)
                        .and_then(|i| usize::try_from(i).ok())
                        .filter(|&i| i < self.people.len())
                },
            )?;
            Some(&self.people[index])
        };
        let needs_feelings = self.owner.policies.par_iter().any(|p| {
            p.pressure_threshold.is_some()
                || p.affinity_at_least.is_some()
                || p.affinity_at_most.is_some()
        }) || self.owner.responses.par_iter().any(|p| {
            p.resist_if_fear_at_least.is_some()
                || p.resist_if_affinity_at_least.is_some()
                || p.resist_if_affinity_at_most.is_some()
        });
        let feelings = self
            .opinions
            .filter(|_| needs_feelings)
            .map(Opinions::index);
        let mut responses: Vec<_> = self.owner.responses.par_iter().collect();
        if responses.par_windows(2).any(|p| p[0].person > p[1].person) {
            responses.par_sort_unstable_by_key(|p| p.person);
        }
        let available = |intent: Intent, controlled: bool| {
            let actor = person(intent.actor)?;
            let target = person(intent.target)?;
            let region = self
                .locations
                .region((actor.id, actor.region), (target.id, target.region))?;
            (actor.controlled == controlled
                && actor.health > 0
                && target.health > 0
                && actor.id != target.id
                && intent.quantity > 0
                && actor.hunger <= self.max_hunger
                && self.blocked.binary_search(&actor.id).is_err()
                && self.blocked.binary_search(&target.id).is_err()
                && (controlled || self.busy.binary_search(&actor.id).is_err())
                && (self.can_touch)(actor.id, target.id))
            .then_some(region)
        };
        let mut choices: Vec<_> = self
            .owner
            .policies
            .par_iter()
            .filter_map(|policy| {
                if policy.interval == 0 || !self.tick.is_multiple_of(policy.interval) {
                    return None;
                }
                let intent = Intent {
                    actor: policy.person,
                    target: policy.target,
                    good: policy.good,
                    quantity: policy.quantity,
                    escalate: policy.escalate,
                };
                let region = available(intent, false)?;
                let affinity = || {
                    feelings
                        .as_ref()
                        .map_or(0, |index| index.affinity(intent.actor, intent.target))
                };
                let willing = policy.willing
                    && policy.affinity_at_least.is_none_or(|v| affinity() >= v)
                    && policy.affinity_at_most.is_none_or(|v| affinity() <= v);
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        feelings.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(intent.actor, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (willing || pressured).then_some((
                    false,
                    random(self.seed ^ intent.target, intent.actor, self.tick),
                    intent,
                    region,
                ))
            })
            .collect();
        if let Some(intent) = self.manual {
            let region = available(intent, true).ok_or(
                "taking requires distinct living, reachable people and an available actor",
            )?;
            choices.push((true, 0, intent, region));
        }
        if choices
            .par_windows(2)
            .any(|pair| pair[0].2.target > pair[1].2.target)
        {
            choices.par_sort_unstable_by_key(|row| row.2.target);
        }
        // Resolve a shared target with a parallel reduction before constructing
        // participant claims. A hot target does not serialize all contenders.
        let mut choices: Vec<_> = choices
            .par_chunk_by(|a, b| a.2.target == b.2.target)
            .map(|group| {
                if group.len() == 1 {
                    group[0]
                } else {
                    *group
                        .par_iter()
                        .min_by_key(|(manual, priority, intent, _)| {
                            (!manual, *priority, intent.actor, intent.target)
                        })
                        .expect("nonempty target group")
                }
            })
            .collect();
        let disjoint_ranges = choices
            .first()
            .zip(choices.last())
            .is_some_and(|(first, last)| {
                (last.2.actor < first.2.target || last.2.target < first.2.actor)
                    && choices
                        .par_windows(2)
                        .all(|pair| pair[0].2.actor < pair[1].2.actor)
            });
        let selected: Vec<_> = if disjoint_ranges {
            choices
        } else {
            choices.par_sort_unstable_by_key(|(manual, priority, intent, _)| {
                (!manual, *priority, intent.actor, intent.target)
            });
            let mut claims: Vec<_> = choices
                .par_iter()
                .enumerate()
                .flat_map_iter(|(index, row)| [(row.2.actor, index), (row.2.target, index)])
                .collect();
            claims.par_sort_unstable();
            choices
                .into_par_iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    let actor = claims.partition_point(|claim| claim.0 < row.2.actor);
                    let target = claims.partition_point(|claim| claim.0 < row.2.target);
                    (claims[actor].1 == index && claims[target].1 == index).then_some(row)
                })
                .collect()
        };
        let mut pending: Vec<_> = selected
            .into_par_iter()
            .map(|(_, _, intent, region)| {
                let response = responses
                    .binary_search_by_key(&intent.target, |p| p.person)
                    .ok()
                    .map(|i| responses[i]);
                let noticed = response.is_some_and(|p| p.notices);
                let opposed = noticed
                    && response.is_some_and(|p| {
                        let fear = || {
                            feelings
                                .as_ref()
                                .map_or(0, |index| index.fear(intent.target, intent.actor))
                        };
                        let affinity = || {
                            feelings
                                .as_ref()
                                .map_or(0, |index| index.affinity(intent.target, intent.actor))
                        };
                        p.resist
                            || p.resist_if_fear_at_least.is_some_and(|v| fear() >= v)
                            || p.resist_if_affinity_at_least
                                .is_some_and(|v| affinity() >= v)
                            || p.resist_if_affinity_at_most
                                .is_some_and(|v| affinity() <= v)
                    });
                Pending {
                    intent,
                    noticed,
                    opposed,
                    action: None,
                    region,
                }
            })
            .collect();
        if pending.par_windows(2).any(|pair| {
            (pair[0].intent.actor, pair[0].intent.target)
                > (pair[1].intent.actor, pair[1].intent.target)
        }) {
            pending.par_sort_unstable_by_key(|p| (p.intent.actor, p.intent.target));
        }
        // An unnoticed or unopposed target remains free to pursue other work.
        let mut actors: Vec<_> = pending
            .par_iter()
            .flat_map_iter(|p| {
                std::iter::once(p.intent.actor).chain(p.opposed.then_some(p.intent.target))
            })
            .collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        let mut targets: Vec<_> = pending.par_iter().map(|p| p.intent.target).collect();
        if targets.par_windows(2).any(|pair| pair[0] > pair[1]) {
            targets.par_sort_unstable();
        }
        let flags: Vec<_> = pending.par_iter().map(|p| u64::from(!p.opposed)).collect();
        let action_count: u64 = flags.par_iter().sum();
        self.action_base
            .checked_add(action_count)
            .ok_or("taking action overflow")?;
        let prefixes = game_parallel_lot_allocation::prefixes(&flags);
        let actions: Vec<_> = pending
            .par_iter_mut()
            .zip(prefixes.into_par_iter())
            .filter_map(|(p, prefix)| {
                if p.opposed {
                    return None;
                }
                let id = self.action_base + prefix as u64;
                p.action = Some(id);
                Some(Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: Place::Person(p.intent.target),
                        to: Some(Place::Person(p.intent.actor)),
                        good: p.intent.good,
                        quantity: p.intent.quantity,
                    }],
                    creations: vec![],
                })
            })
            .collect();
        let contests = pending
            .par_iter()
            .filter(|p| p.opposed && p.intent.escalate)
            .map(|p| Contest {
                actor: p.intent.actor,
                target: p.intent.target,
                region: p.region,
            })
            .collect();
        Ok(Plan {
            actors,
            targets,
            actions,
            contests,
            pending,
            tick: self.tick,
            action_base: self.action_base,
        })
    }
}

impl Plan {
    /// Assign the host's final contiguous action range after other planners run.
    pub fn rebase(&mut self, base: u64) -> Result<(), &'static str> {
        base.checked_add(self.actions.len() as u64)
            .ok_or("taking action overflow")?;
        let previous = self.action_base;
        self.actions.par_iter_mut().for_each(|action| {
            let id = base + (action.id - previous);
            action.id = id;
            for movement in &mut action.movements {
                movement.id = id;
            }
        });
        self.pending.par_iter_mut().for_each(|pending| {
            if let Some(id) = &mut pending.action {
                *id = base + (*id - previous);
            }
        });
        self.action_base = base;
        Ok(())
    }
    /// Resolve only admitted movements and conflicts actually started by the host.
    pub fn publish(&self, events: &Events, started: &[(u64, u64)]) -> Vec<Event> {
        // A host normally returns sorted receipts for this contiguous action
        // range. Keep a general fallback for unordered or partial receipts.
        let direct = self
            .action_base
            .checked_add(self.actions.len() as u64)
            .and_then(|end| {
                let rows = &events.physical_actions;
                if rows.par_windows(2).any(|p| p[0].id >= p[1].id) {
                    return None;
                }
                let begin = rows.partition_point(|row| row.id < self.action_base);
                let finish = rows.partition_point(|row| row.id < end);
                let selected = &rows[begin..finish];
                (selected.len() == self.actions.len()
                    && selected
                        .par_iter()
                        .enumerate()
                        .all(|(index, row)| row.id == self.action_base + index as u64))
                .then_some(selected)
            });
        let mut accepted: Vec<_> = if direct.is_none() {
            events
                .physical_actions
                .par_iter()
                .filter(|e| e.accepted)
                .map(|e| e.id)
                .collect()
        } else {
            Vec::new()
        };
        if direct.is_none() {
            accepted.par_sort_unstable();
        }
        let mut started = started.to_vec();
        started.par_sort_unstable();
        self.pending
            .par_iter()
            .map(|p| {
                let outcome = if p.opposed {
                    if p.intent.escalate
                        && started
                            .binary_search(&(p.intent.actor, p.intent.target))
                            .is_ok()
                    {
                        Outcome::Conflict
                    } else {
                        Outcome::Resisted
                    }
                } else if p.action.is_some_and(|id| {
                    direct.map_or_else(
                        || accepted.binary_search(&id).is_ok(),
                        |rows| {
                            id.checked_sub(self.action_base)
                                .and_then(|offset| usize::try_from(offset).ok())
                                .and_then(|offset| rows.get(offset))
                                .is_some_and(|row| row.accepted)
                        },
                    )
                }) {
                    Outcome::Transferred
                } else {
                    Outcome::Unavailable
                };
                Event {
                    tick: self.tick,
                    actor: p.intent.actor,
                    target: p.intent.target,
                    good: p.intent.good,
                    quantity: p.intent.quantity,
                    noticed: p.noticed,
                    outcome,
                }
            })
            .collect()
    }
}

//! Deterministic, read-only arbitration of indivisible multi-resource intents.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A receiver-defined resource namespace and its stable logical coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceKey {
    pub domain: u64,
    pub owner: u64,
    pub item: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Demand {
    pub resource: ResourceKey,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub id: u64,
    pub actor: u64,
    /// Other people whose exclusive participation this action needs. The actor
    /// is always a participant, even when omitted here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<u64>,
    /// Lower values are served first, before the rotating fairness order.
    pub priority: u8,
    pub demands: Vec<Demand>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Granted,
    ActorAlreadyGranted,
    ParticipantAlreadyGranted(u64),
    Insufficient(ResourceKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    /// Sorted by intent ID, independent of input and task completion order.
    pub decisions: BTreeMap<u64, Decision>,
    /// Exact quantities tentatively granted. No authoritative stock is changed.
    pub granted: BTreeMap<ResourceKey, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    DuplicateIntent(u64),
    ZeroQuantity(u64),
    DemandOverflow(u64),
    DuplicatePrecedence(u64),
    MissingPrecedence(u64),
}

/// Stable priority order with actor precedence rotated independently in each
/// class. Inputs must contain each actor once. This is only an ordering policy;
/// bounded callers still need to resolve actual shared-resource conflicts.
pub fn order_actors(actors: &[(u64, u8)], round: u64) -> Vec<u64> {
    let mut classes = BTreeMap::<u8, Vec<u64>>::new();
    for &(actor, priority) in actors {
        classes.entry(priority).or_default().push(actor);
    }
    let mut ordered = Vec::with_capacity(actors.len());
    for (_, mut class) in classes {
        class.sort_unstable();
        let offset = (round % class.len() as u64) as usize;
        class.rotate_left(offset);
        ordered.extend(class);
    }
    ordered
}

#[derive(Clone)]
struct Normalized {
    id: u64,
    actor: u64,
    participants: BTreeSet<u64>,
    priority: u8,
    demands: BTreeMap<ResourceKey, u64>,
}

fn root(parents: &mut [usize], mut i: usize) -> usize {
    while parents[i] != i {
        parents[i] = parents[parents[i]];
        i = parents[i];
    }
    i
}
fn join(parents: &mut [usize], a: usize, b: usize) {
    let a = root(parents, a);
    let b = root(parents, b);
    if a != b {
        parents[b] = a;
    }
}

/// Resolve all-or-nothing claims against a single immutable availability view.
///
/// Intents sharing any resource or participant form a conflict group. Groups may run
/// concurrently; within each priority class, sorted actor ranks rotate by the
/// supplied logical round. A missing resource has zero availability. The result
/// is advisory: owners must validate participants and commit effects against
/// current state. Participants are reserved atomically with all stock claims.
pub fn arbitrate(
    pool: &rayon::ThreadPool,
    available: &BTreeMap<ResourceKey, u64>,
    intents: &[Intent],
    round: u64,
) -> Result<Resolution, Error> {
    arbitrate_inner(pool, available, intents, round, None)
}

/// Resolve using an explicit actor precedence already chosen by the caller.
/// Priority still precedes this order. Each intent actor must occur exactly once
/// in the precedence list; results remain independent of input and worker order.
pub fn arbitrate_ordered(
    pool: &rayon::ThreadPool,
    available: &BTreeMap<ResourceKey, u64>,
    intents: &[Intent],
    precedence: &[u64],
) -> Result<Resolution, Error> {
    let mut ranks = BTreeMap::new();
    for (rank, &actor) in precedence.iter().enumerate() {
        if ranks.insert(actor, rank).is_some() {
            return Err(Error::DuplicatePrecedence(actor));
        }
    }
    for intent in intents {
        if !ranks.contains_key(&intent.actor) {
            return Err(Error::MissingPrecedence(intent.actor));
        }
    }
    arbitrate_inner(pool, available, intents, 0, Some(&ranks))
}

fn arbitrate_inner(
    pool: &rayon::ThreadPool,
    available: &BTreeMap<ResourceKey, u64>,
    intents: &[Intent],
    round: u64,
    precedence: Option<&BTreeMap<u64, usize>>,
) -> Result<Resolution, Error> {
    let mut ids = BTreeSet::new();
    let mut normalized = Vec::with_capacity(intents.len());
    for intent in intents {
        if !ids.insert(intent.id) {
            return Err(Error::DuplicateIntent(intent.id));
        }
        let mut demands = BTreeMap::<ResourceKey, u64>::new();
        for demand in &intent.demands {
            if demand.quantity == 0 {
                return Err(Error::ZeroQuantity(intent.id));
            }
            let quantity = demands.entry(demand.resource).or_default();
            *quantity = quantity
                .checked_add(demand.quantity)
                .ok_or(Error::DemandOverflow(intent.id))?;
        }
        normalized.push(Normalized {
            id: intent.id,
            actor: intent.actor,
            participants: intent
                .participants
                .iter()
                .copied()
                .filter(|p| *p != intent.actor)
                .collect(),
            priority: intent.priority,
            demands,
        });
    }
    let mut parents: Vec<_> = (0..normalized.len()).collect();
    let mut resources = BTreeMap::new();
    let mut participants = BTreeMap::new();
    for (i, intent) in normalized.iter().enumerate() {
        for participant in std::iter::once(&intent.actor).chain(&intent.participants) {
            if let Some(previous) = participants.insert(*participant, i) {
                join(&mut parents, i, previous);
            }
        }
        for resource in intent.demands.keys() {
            if let Some(previous) = resources.insert(*resource, i) {
                join(&mut parents, i, previous);
            }
        }
    }
    let mut groups = BTreeMap::<usize, Vec<&Normalized>>::new();
    for (i, intent) in normalized.iter().enumerate() {
        groups
            .entry(root(&mut parents, i))
            .or_default()
            .push(intent);
    }
    let groups: Vec<_> = groups.into_values().collect();
    let resolve = |group: &Vec<&Normalized>| {
        let mut classes = BTreeMap::<u8, BTreeSet<u64>>::new();
        for intent in group {
            classes
                .entry(intent.priority)
                .or_default()
                .insert(intent.actor);
        }
        let classes: BTreeMap<u8, Vec<u64>> = classes
            .into_iter()
            .map(|(priority, actors)| (priority, actors.into_iter().collect()))
            .collect();
        let mut ordered = group.clone();
        ordered.sort_by_key(|intent| {
            let actors = &classes[&intent.priority];
            let rank = if let Some(precedence) = precedence {
                precedence[&intent.actor]
            } else {
                let start = (round % actors.len() as u64) as usize;
                let rank = actors.binary_search(&intent.actor).unwrap();
                (rank + actors.len() - start) % actors.len()
            };
            (intent.priority, rank, intent.id)
        });
        let mut remaining = BTreeMap::<ResourceKey, u64>::new();
        let mut granted = BTreeMap::<ResourceKey, u64>::new();
        let mut used_actors = BTreeSet::new();
        let mut used_participants = BTreeSet::new();
        let mut decisions = BTreeMap::new();
        for intent in ordered {
            let decision = if used_actors.contains(&intent.actor) {
                Decision::ActorAlreadyGranted
            } else if used_participants.contains(&intent.actor) {
                Decision::ParticipantAlreadyGranted(intent.actor)
            } else if let Some(person) = intent
                .participants
                .iter()
                .find(|p| used_actors.contains(*p) || used_participants.contains(*p))
            {
                Decision::ParticipantAlreadyGranted(*person)
            } else if let Some(resource) = intent.demands.iter().find_map(|(resource, amount)| {
                let free = *remaining
                    .entry(*resource)
                    .or_insert_with(|| available.get(resource).copied().unwrap_or(0));
                (free < *amount).then_some(*resource)
            }) {
                Decision::Insufficient(resource)
            } else {
                used_actors.insert(intent.actor);
                used_participants.extend(intent.participants.iter().copied());
                for (resource, amount) in &intent.demands {
                    *remaining.get_mut(resource).unwrap() -= amount;
                    *granted.entry(*resource).or_default() += amount;
                }
                Decision::Granted
            };
            decisions.insert(intent.id, decision);
        }
        Resolution { decisions, granted }
    };
    let resolved: Vec<_> = if groups.len() > 1 && pool.current_num_threads() > 1 {
        pool.install(|| groups.par_iter().map(resolve).collect())
    } else {
        groups.iter().map(resolve).collect()
    };
    let mut result = Resolution {
        decisions: BTreeMap::new(),
        granted: BTreeMap::new(),
    };
    for group in resolved {
        result.decisions.extend(group.decisions);
        result.granted.extend(group.granted);
    }
    Ok(result)
}

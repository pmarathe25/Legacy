//! Deterministic per-person selection among owner-provided opportunities.
//! Selection creates intentions only: feature owners must still revalidate and
//! commit the chosen action, and other people's responses remain independent.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub const MAX_DOMAINS: usize = 64;
pub const MAX_OPPORTUNITIES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Key {
    pub domain: u32,
    pub action: u64,
}

/// A receiver-owned candidate, prepared from the actor's accessible evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub actor: u64,
    pub key: Key,
    /// An observed or personally estimated consequence, never global welfare.
    pub value: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub domain: u32,
    pub enabled: bool,
    pub bias: i64,
    /// Signed per-thousand interpretation of the proposed consequence.
    #[serde(default = "unit_weight")]
    pub weight: i32,
}

fn unit_weight() -> i32 {
    1000
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: u64,
    /// The person's value for remaining idle at this decision opportunity.
    pub idle: i64,
    /// Whether an equally valued opportunity may win over idleness.
    #[serde(default)]
    pub act_on_tie: bool,
    /// Sorted by domain; unlisted domains are not accepted automatically.
    pub preferences: Vec<Preference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub actor: u64,
    /// None is the explicit personal choice to remain idle.
    pub action: Option<Key>,
}

fn random(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Validate saved profiles without manufacturing a decision round.
pub fn validate_people(people: &[Person]) -> Result<(), &'static str> {
    if people.par_windows(2).any(|w| w[0].id >= w[1].id)
        || people.par_iter().any(|p| {
            p.id == 0
                || p.preferences.len() > MAX_DOMAINS
                || p.preferences.windows(2).any(|w| w[0].domain >= w[1].domain)
                || p.preferences.iter().any(|row| row.domain == 0)
        })
    {
        return Err("invalid personal choice profiles");
    }
    Ok(())
}

/// Select at most one intention per person without touching any feature state.
/// Each person's tie policy governs whether an opportunity equal to idle may
/// win. Stable semantic identifiers, not input order, break action ties.
pub fn select(
    people: &[Person],
    proposals: &[Proposal],
    seed: u64,
    round: u64,
) -> Result<Vec<Selection>, &'static str> {
    validate_people(people)?;
    let mut sorted: Vec<_> = proposals.par_iter().copied().collect();
    let person_index = |id: u64| {
        people
            .first()
            .and_then(|person| id.checked_sub(person.id))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| people.get(index).is_some_and(|person| person.id == id))
            .or_else(|| people.binary_search_by_key(&id, |person| person.id).ok())
    };
    if sorted
        .par_windows(2)
        .any(|w| (w[0].actor, w[0].key) > (w[1].actor, w[1].key))
    {
        sorted.par_sort_unstable_by_key(|row| (row.actor, row.key));
    }
    if sorted
        .par_windows(2)
        .any(|w| (w[0].actor, w[0].key) == (w[1].actor, w[1].key))
        || sorted
            .par_chunk_by(|a, b| a.actor == b.actor)
            .any(|rows| rows.len() > MAX_OPPORTUNITIES)
        || sorted.par_iter().any(|p| {
            p.actor == 0
                || p.key.domain == 0
                || p.key.action == 0
                || person_index(p.actor).is_none()
        })
    {
        return Err("invalid personal choice proposals");
    }
    let aligned = people.len() == sorted.len()
        && people
            .par_iter()
            .zip(sorted.par_iter())
            .all(|(person, proposal)| person.id == proposal.actor);
    Ok(people
        .par_iter()
        .enumerate()
        .map(|(index, person)| {
            let (start, end) = if aligned {
                (index, index + 1)
            } else {
                (
                    sorted.partition_point(|row| row.actor < person.id),
                    sorted.partition_point(|row| row.actor <= person.id),
                )
            };
            let mut best: Option<(i128, u64, Key)> = None;
            for proposal in &sorted[start..end] {
                let Some(pref) = person
                    .preferences
                    .binary_search_by_key(&proposal.key.domain, |p| p.domain)
                    .ok()
                    .map(|i| person.preferences[i])
                    .filter(|p| p.enabled)
                else {
                    continue;
                };
                let value = i128::from(proposal.value) * i128::from(pref.weight) / 1000
                    + i128::from(pref.bias);
                if value < i128::from(person.idle)
                    || (value == i128::from(person.idle) && !person.act_on_tie)
                {
                    continue;
                }
                let tie = random(
                    seed ^ round.rotate_left(13)
                        ^ person.id
                        ^ u64::from(proposal.key.domain).rotate_left(27)
                        ^ proposal.key.action,
                );
                let candidate = (value, tie, proposal.key);
                if best.is_none_or(|prior| candidate > prior) {
                    best = Some(candidate);
                }
            }
            Selection {
                actor: person.id,
                action: best.map(|row| row.2),
            }
        })
        .collect())
}

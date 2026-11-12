//! Deterministic per-person selection among owner-provided opportunities.
//! Selection creates intentions only: feature owners must still revalidate and
//! commit the chosen action, and other people's responses remain independent.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

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

/// A person's additional attitude toward one particular opportunity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPreference {
    pub key: Key,
    pub enabled: bool,
    pub bias: i64,
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
    /// Optional, sorted refinements to the person's domain preferences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionPreference>,
}

/// A data-defined inclination for a newly born person, not a copied decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BirthPreference {
    pub domain: u32,
    pub enabled: bool,
    pub bias: i64,
    #[serde(default = "unit_weight")]
    pub weight: i32,
    /// Optional inherited quantitative locus in the person's own genome.
    #[serde(default)]
    pub locus: Option<u8>,
    /// Signed change to bias per 1000 expressed units away from neutral 500.
    #[serde(default)]
    pub genetic_bias: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BirthProfile {
    pub idle: i64,
    #[serde(default)]
    pub act_on_tie: bool,
    pub preferences: Vec<BirthPreference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Newborn {
    pub id: u64,
    pub genes: [u16; 4],
}

impl BirthProfile {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .preferences
            .windows(2)
            .any(|rows| rows[0].domain >= rows[1].domain)
            || self
                .preferences
                .iter()
                .any(|row| row.domain == 0 || row.locus.is_some_and(|locus| locus >= 4))
        {
            return Err("invalid newborn choice preferences");
        }
        Ok(())
    }

    /// Construct independent inclinations from each newborn's own genes.
    pub fn enroll(&self, births: &[Newborn]) -> Vec<Person> {
        births
            .par_iter()
            .map(|birth| Person {
                id: birth.id,
                idle: self.idle,
                act_on_tie: self.act_on_tie,
                preferences: self
                    .preferences
                    .iter()
                    .map(|row| {
                        let gene = row
                            .locus
                            .and_then(|locus| birth.genes.get(usize::from(locus)))
                            .copied()
                            .unwrap_or(500);
                        let bias = i128::from(row.bias)
                            + i128::from(row.genetic_bias) * (i128::from(gene) - 500) / 1000;
                        Preference {
                            domain: row.domain,
                            enabled: row.enabled,
                            bias: bias.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
                            weight: row.weight,
                        }
                    })
                    .collect(),
                actions: Vec::new(),
            })
            .collect()
    }
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
                || p.preferences.windows(2).any(|w| w[0].domain >= w[1].domain)
                || p.preferences.iter().any(|row| row.domain == 0)
                || p.actions.windows(2).any(|w| w[0].key >= w[1].key)
                || p.actions
                    .iter()
                    .any(|row| row.key.domain == 0 || row.key.action == 0)
        })
    {
        return Err("invalid personal choice profiles");
    }
    Ok(())
}

/// Interpret a proposal through this person's domain and specific preferences.
/// None means the person has disabled or not adopted the corresponding domain
/// or has explicitly rejected this particular opportunity.
pub fn score(person: &Person, key: Key, value: i64) -> Option<i128> {
    let preference = person
        .preferences
        .binary_search_by_key(&key.domain, |row| row.domain)
        .ok()
        .map(|index| person.preferences[index])
        .filter(|row| row.enabled)?;
    let specific = person
        .actions
        .binary_search_by_key(&key, |row| row.key)
        .ok()
        .map(|index| person.actions[index]);
    if specific.is_some_and(|row| !row.enabled) {
        return None;
    }
    Some(
        i128::from(value) * i128::from(preference.weight) / 1000
            + i128::from(preference.bias)
            + i128::from(specific.map_or(0, |row| row.bias)),
    )
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
                let Some(value) = score(person, proposal.key, proposal.value) else {
                    continue;
                };
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

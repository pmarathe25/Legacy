//! Parallel resolution of individually chosen, simultaneous physical strikes.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub mod wounds;

/// An actual choice to strike; a target's consent is not required.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strike {
    pub actor: u64,
    pub target: u64,
    pub force: u32,
}

/// A target's own chosen defense. Absence means no defensive reduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defense {
    pub person: u64,
    pub reduction: u32,
}

/// Attributed potential harm, before the health owner caps loss at health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeEvent {
    pub actor: u64,
    pub target: u64,
    pub force: u32,
    pub defended: u32,
    pub harm: u32,
}

/// Total potential harm to one person, calculated from the opening state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Impact {
    pub target: u64,
    pub harm: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub events: Vec<StrikeEvent>,
    pub impacts: Vec<Impact>,
}

/// Resolve chosen strikes without mutating people, health, or relationships.
/// The host must validate personal choices, physical reach, and capabilities.
/// Each actor can strike once; many actors may strike the same target. Damage
/// is simultaneous, including when an actor is also targeted in the batch.
pub fn resolve(strikes: &[Strike], defenses: &[Defense]) -> Result<Resolution, &'static str> {
    if defenses
        .par_windows(2)
        .any(|rows| rows[0].person >= rows[1].person)
        || defenses.par_iter().any(|row| row.person == 0)
        || strikes.par_iter().any(|row| {
            row.actor == 0 || row.target == 0 || row.actor == row.target || row.force == 0
        })
    {
        return Err("invalid strikes or defenses");
    }
    let mut actors: Vec<_> = strikes.par_iter().map(|row| row.actor).collect();
    actors.par_sort_unstable();
    if actors.par_windows(2).any(|rows| rows[0] == rows[1]) {
        return Err("a person cannot strike twice in one batch");
    }
    let mut events: Vec<_> = strikes
        .par_iter()
        .map(|strike| {
            let reduction = defenses
                .binary_search_by_key(&strike.target, |row| row.person)
                .ok()
                .map_or(0, |index| defenses[index].reduction);
            let defended = reduction.min(strike.force);
            StrikeEvent {
                actor: strike.actor,
                target: strike.target,
                force: strike.force,
                defended,
                harm: strike.force - defended,
            }
        })
        .collect();
    events.par_sort_unstable_by_key(|row| (row.target, row.actor));
    let impacts = events
        .par_chunk_by(|a, b| a.target == b.target)
        .map(|group| Impact {
            target: group[0].target,
            harm: if group.len() >= 1024 {
                group
                    .par_iter()
                    .map(|row| u64::from(row.harm))
                    .reduce(|| 0, u64::saturating_add)
            } else {
                group
                    .iter()
                    .fold(0u64, |total, row| total.saturating_add(u64::from(row.harm)))
            },
        })
        .collect();
    Ok(Resolution { events, impacts })
}

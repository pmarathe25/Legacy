//! Persistent physical injuries, independent of personal decisions or blame.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub minimum_harm: u32,
    pub recovery_ticks: u64,
    /// Physical force lost per point of wound severity; zero leaves force unchanged.
    #[serde(default)]
    pub force_loss_per_severity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wound {
    pub person: u64,
    pub severity: u32,
    pub sustained_at: u64,
    pub heals_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Injury {
    pub person: u64,
    /// Actual health lost by the person, capped by their opening health.
    pub harm: u32,
}

/// A physical medicine and its effect when an individual actually uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remedy {
    pub good: u32,
    pub quantity: u64,
    pub severity_relief: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wounds {
    pub rules: Rules,
    #[serde(default)]
    pub cases: Vec<Wound>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remedies: Vec<Remedy>,
}

impl Wounds {
    pub fn available_force(&self, person: u64, maximum: u32) -> u32 {
        let loss = self
            .cases
            .binary_search_by_key(&person, |case| case.person)
            .ok()
            .map_or(0, |index| {
                self.cases[index]
                    .severity
                    .saturating_mul(self.rules.force_loss_per_severity)
            });
        maximum.saturating_sub(loss)
    }
    pub fn validate(
        &self,
        tick: u64,
        exists: &(dyn Fn(u64) -> bool + Sync),
    ) -> Result<(), &'static str> {
        if self.rules.minimum_harm == 0
            || self.rules.recovery_ticks == 0
            || self
                .cases
                .par_windows(2)
                .any(|pair| pair[0].person >= pair[1].person)
            || self
                .remedies
                .par_windows(2)
                .any(|pair| pair[0].good >= pair[1].good)
            || self.remedies.par_iter().any(|remedy| {
                remedy.good == 0 || remedy.quantity == 0 || remedy.severity_relief == 0
            })
            || self.cases.par_iter().any(|wound| {
                !exists(wound.person)
                    || wound.severity == 0
                    || wound.sustained_at > tick
                    || wound.heals_at <= tick
                    || wound.heals_at <= wound.sustained_at
            })
        {
            return Err("invalid wounds");
        }
        Ok(())
    }

    /// Prepare a generation from simultaneous, already capped physical losses.
    /// No individual attacker is credited with a group injury.
    pub fn injured(&self, injuries: &[Injury], tick: u64) -> Result<Self, &'static str> {
        if self.rules.minimum_harm == 0 || self.rules.recovery_ticks == 0 {
            return Err("invalid wound rules");
        }
        if injuries
            .par_windows(2)
            .any(|pair| pair[0].person >= pair[1].person)
            || injuries.par_iter().any(|row| row.person == 0)
        {
            return Err("injuries must be sorted by distinct person");
        }
        let has_injuries = injuries
            .par_iter()
            .any(|row| row.harm >= self.rules.minimum_harm);
        let healing = if has_injuries {
            tick.checked_add(self.rules.recovery_ticks)
                .ok_or("wound recovery overflow")?
        } else {
            tick
        };
        let mut cases: Vec<_> = self
            .cases
            .par_iter()
            .filter(|case| case.heals_at > tick)
            .cloned()
            .collect();
        let mut added: Vec<_> = injuries
            .par_iter()
            .filter(|row| row.harm >= self.rules.minimum_harm)
            .map(|row| {
                let previous = self
                    .cases
                    .binary_search_by_key(&row.person, |case| case.person)
                    .ok()
                    .map(|index| &self.cases[index])
                    .filter(|case| case.heals_at > tick);
                Wound {
                    person: row.person,
                    severity: previous
                        .map_or(row.harm, |case| case.severity.saturating_add(row.harm)),
                    sustained_at: tick,
                    heals_at: previous.map_or(healing, |case| case.heals_at.max(healing)),
                }
            })
            .collect();
        if !added.is_empty() {
            cases = cases
                .into_par_iter()
                .filter(|case| {
                    added
                        .binary_search_by_key(&case.person, |row| row.person)
                        .is_err()
                })
                .collect();
            cases.append(&mut added);
            cases.par_sort_unstable_by_key(|case| case.person);
        }
        Ok(Self {
            rules: self.rules.clone(),
            cases,
            remedies: self.remedies.clone(),
        })
    }

    /// Apply only treatments whose physical consumption was accepted.
    /// Each pair identifies a person and the medicine they actually consumed.
    pub fn treated(&self, accepted: &[(u64, u32)]) -> Result<Self, &'static str> {
        if accepted.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || accepted.par_iter().any(|(person, good)| {
                *person == 0
                    || self
                        .remedies
                        .binary_search_by_key(good, |remedy| remedy.good)
                        .is_err()
            })
        {
            return Err("invalid accepted wound treatments");
        }
        let cases = self
            .cases
            .par_iter()
            .filter_map(|case| {
                let Some((_, good)) = accepted
                    .binary_search_by_key(&case.person, |row| row.0)
                    .ok()
                    .map(|index| accepted[index])
                else {
                    return Some(case.clone());
                };
                let relief = self.remedies[self
                    .remedies
                    .binary_search_by_key(&good, |remedy| remedy.good)
                    .expect("validated remedy")]
                .severity_relief;
                let mut next = case.clone();
                next.severity = next.severity.saturating_sub(relief);
                (next.severity > 0).then_some(next)
            })
            .collect();
        Ok(Self {
            rules: self.rules.clone(),
            cases,
            remedies: self.remedies.clone(),
        })
    }
}

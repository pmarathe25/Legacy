//! Personal responses to social pressure, independent of institutional authority.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub struct Contact {
    pub busy: bool,
    pub person: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub hunger: u32,
}
pub fn priority(seed: u64, tick: u64, id: u64) -> u64 {
    let mut x = seed ^ tick.wrapping_mul(0x9e3779b97f4a7c15) ^ id;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fear_at_least: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_at_least: Option<u64>,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}

impl Policy {
    pub fn responds(&self, fear: u64, obligation: u64) -> bool {
        self.enabled
            && (self.fear_at_least.is_some_and(|limit| fear >= limit)
                || self
                    .obligation_at_least
                    .is_some_and(|limit| obligation >= limit))
    }
}
pub fn validate(rows: &[Policy], participants: &HashSet<u64>) -> Result<(), &'static str> {
    let mut seen = HashSet::new();
    if rows.iter().any(|row| {
        !participants.contains(&row.person)
            || !seen.insert(row.person)
            || row.fear_at_least == Some(0)
            || row.obligation_at_least == Some(0)
            || (row.fear_at_least.is_none() && row.obligation_at_least.is_none())
    }) {
        return Err("invalid social pressure preference");
    }
    Ok(())
}

/// Newborns can start with a parent's inclination, never their directed experiences.
pub fn inherit(rows: &mut Vec<Policy>, births: &[(u64, [u64; 2], usize)]) {
    use rayon::prelude::*;
    if births.is_empty() {
        return;
    }
    let sorted = if rows.par_windows(2).any(|w| w[0].person > w[1].person) {
        let mut ordered: Vec<_> = rows.par_iter().cloned().collect();
        ordered.par_sort_unstable_by_key(|row| row.person);
        Some(ordered)
    } else {
        None
    };
    let by_person = sorted.as_deref().unwrap_or(rows.as_slice());
    let children: Vec<_> = births
        .par_iter()
        .filter_map(|&(person, parents, count)| {
            if count == 0 {
                return None;
            }
            let parent = parents[(person % count as u64) as usize];
            let index = by_person
                .binary_search_by_key(&parent, |row| row.person)
                .ok()?;
            let mut preference = by_person[index].clone();
            preference.person = person;
            Some(preference)
        })
        .collect();
    rows.par_extend(children);
}

#[macro_export]
macro_rules! inherit_birth_pressure {
    ($world:expr, $family:expr, $births:expr) => {
        if let Some(social) = $world
            .social
            .as_mut()
            .filter(|social| !social.pressures.is_empty())
        {
            let parents: Vec<_> = $births
                .par_iter()
                .map(|&person| {
                    let slot = $family.store.locate(person).expect("newborn family record");
                    let row = &$family.store.metadata[slot];
                    (person, row.parents, row.parent_count)
                })
                .collect();
            $crate::inherit(&mut social.pressures, &parents);
        }
    };
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Encounter {
    pub tick: u64,
    pub region: u64,
    pub a: u64,
    pub b: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressured: Option<u64>,
}
impl Encounter {
    pub fn unpressured(&self) -> impl Iterator<Item = u64> + '_ {
        [self.a, self.b]
            .into_iter()
            .filter(|&person| self.pressured != Some(person))
    }
}

#[macro_export]
macro_rules! world_pressure_method {
    () => {
        /// Evaluate one target's own pressure preference and directed experiences.
        pub fn socially_pressured(&self, target: u64, requester: u64) -> bool {
            use rayon::prelude::*;
            let Some(policy) = self
                .social
                .as_ref()
                .and_then(|social| social.pressures.par_iter().find_any(|p| p.person == target))
            else {
                return false;
            };
            let Some(opinion) = self.opinions.as_ref().and_then(|opinions| {
                opinions
                    .records
                    .par_iter()
                    .find_any(|o| o.observer == target && o.target == requester)
            }) else {
                return false;
            };
            policy.responds(opinion.fear, opinion.obligation)
        }
        /// Change only this person's own inclination to yield to pressure.
        pub fn set_social_pressure(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            if self.suspended_person() == Some(person) {
                return Err("player control is suspended".into());
            }
            if !self
                .regions
                .iter()
                .flat_map(|region| &region.people)
                .chain(self.travelers.iter().map(|traveler| &traveler.person))
                .any(|actor| actor.id == person && actor.health > 0)
            {
                return Err("person must be alive".into());
            }
            let policy = self
                .social
                .as_mut()
                .ok_or("social is disabled")?
                .pressures
                .iter_mut()
                .find(|policy| policy.person == person)
                .ok_or("person has no social pressure preference")?;
            policy.enabled = enabled;
            Ok(())
        }
    };
}

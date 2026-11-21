//! Physical recovery of living people, with additional recovery from actual rest.
use game_economy_person::Person;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedRecovery {
    pub period_millis: u64,
    /// Time actually rested by each person since the last recovery event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rest: Vec<(u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default)]
    pub interval: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<ElapsedRecovery>,
    pub health_per_interval: u32,
    pub rest_bonus: u32,
    pub max_hunger: u32,
}

impl Rules {
    pub fn valid_cadence(&self) -> bool {
        match &self.elapsed {
            Some(clock) => clock.period_millis > 0 && self.interval == 0,
            None => self.interval > 0,
        }
    }

    pub fn next_elapsed(&self, now: u128) -> Option<u64> {
        let period = self.elapsed.as_ref()?.period_millis;
        if period == 0 {
            return None;
        }
        Some(period - (now % u128::from(period)) as u64)
    }

    pub fn validate_elapsed(&self, now: u128, people: &[Person]) -> Result<(), String> {
        if !self.valid_cadence() {
            return Err("invalid recovery cadence".into());
        }
        if let Some(clock) = &self.elapsed {
            let age = (now % u128::from(clock.period_millis)) as u64;
            if clock.rest.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
                || clock.rest.par_iter().any(|&(person, millis)| {
                    millis == 0
                        || millis > age
                        || people.binary_search_by_key(&person, |row| row.id).is_err()
                })
            {
                return Err("invalid saved elapsed rest for recovery".into());
            }
        }
        Ok(())
    }

    pub fn stage_elapsed(
        &self,
        now: u128,
        millis: u64,
        rest: &[(u64, u64)],
        people: Option<(&mut [Person], u32)>,
    ) -> Result<Self, String> {
        let clock = self
            .elapsed
            .as_ref()
            .ok_or("recovery requires an elapsed cadence")?;
        let deadline = self.next_elapsed(now).ok_or("missing recovery deadline")?;
        if millis > deadline
            || rest.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || rest.par_iter().any(|&(_, value)| value > millis)
        {
            return Err("invalid elapsed recovery segment".into());
        }
        let mut combined = clock.rest.clone();
        combined.par_extend(rest.par_iter().copied().filter(|&(_, value)| value > 0));
        combined.par_sort_unstable_by_key(|row| row.0);
        let mut merged: Vec<_> = combined
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let millis = group.iter().map(|row| u128::from(row.1)).sum::<u128>();
                if millis > u128::from(clock.period_millis) {
                    return Err("rest exceeds recovery period".to_string());
                }
                Ok((group[0].0, millis as u64))
            })
            .collect::<Result<_, String>>()?;
        if millis == deadline {
            let (people, max_health) =
                people.ok_or("due recovery requires a staged person generation")?;
            people.par_iter_mut().for_each(|person| {
                if person.health == 0 || person.hunger > self.max_hunger {
                    return;
                }
                let rest = merged
                    .binary_search_by_key(&person.id, |row| row.0)
                    .ok()
                    .map_or(0, |index| merged[index].1);
                let bonus = (u128::from(self.rest_bonus) * u128::from(rest)
                    / u128::from(clock.period_millis)) as u32;
                person.health = person
                    .health
                    .saturating_add(self.health_per_interval)
                    .saturating_add(bonus)
                    .min(max_health);
            });
            merged.clear();
        }
        let mut next = self.clone();
        next.elapsed.as_mut().expect("checked mode").rest = merged;
        Ok(next)
    }
}

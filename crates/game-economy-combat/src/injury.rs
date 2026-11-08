//! Sparse, persistent wounds caused by actual attributed combat strikes.
use game_conflict::Event;
use game_lot_runtime::Work;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub minimum_damage: u32,
    pub recovery_ticks: u64,
    /// Percentage of ordinary work rate retained while wounded.
    pub work_percent: u8,
    /// This injury prevents tactical movement while it is present.
    #[serde(default)]
    pub immobilizes: bool,
    pub medicine: u32,
    pub medicine_quantity: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn injuries() -> Injuries {
        Injuries {
            rules: Rules {
                minimum_damage: 2,
                recovery_ticks: 3,
                work_percent: 50,
                immobilizes: true,
                medicine: 1,
                medicine_quantity: 2,
            },
            cases: vec![],
        }
    }

    #[test]
    fn treatment_cannot_erase_a_new_wound_from_the_same_boundary() {
        let mut opening = injuries();
        opening.advance(
            &[Event {
                tick: 1,
                encounter: 1,
                attacker: 2,
                target: 1,
                damage: 2,
            }],
            1,
        );
        let mut next = opening.clone();
        next.advance(
            &[Event {
                tick: 2,
                encounter: 1,
                attacker: 3,
                target: 1,
                damage: 2,
            }],
            2,
        );
        next.treat_opening(&opening, &[(1, 1, 2)]);
        assert_eq!(next.cases[0].attacker, 3);
        next.advance(&[], 5);
        assert!(next.cases.is_empty());
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub person: u64,
    pub sustained_at: u64,
    pub heals_at: u64,
    pub attacker: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Injuries {
    pub rules: Rules,
    #[serde(default)]
    pub cases: Vec<Case>,
}

impl Injuries {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), String> {
        let rules = &self.rules;
        let mut ids: Vec<_> = self.cases.par_iter().map(|c| c.person).collect();
        ids.par_sort_unstable();
        if rules.minimum_damage == 0
            || rules.recovery_ticks == 0
            || rules.work_percent > 100
            || rules.medicine_quantity == 0
            || !goods.contains(&rules.medicine)
            || ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.cases.par_iter().any(|case| {
                !people.contains(&case.person)
                    || !people.contains(&case.attacker)
                    || case.sustained_at > tick
                    || case.heals_at <= case.sustained_at
                    || case.heals_at <= tick
            })
        {
            return Err("invalid persistent injury".into());
        }
        Ok(())
    }

    /// Recover due cases and retain at most one new attributed wound per person.
    pub fn advance(&mut self, events: &[Event], tick: u64) {
        if events.is_empty() && self.cases.par_iter().all(|case| case.heals_at > tick) {
            return;
        }
        let mut hits: Vec<_> = events
            .par_iter()
            .filter(|event| event.damage >= self.rules.minimum_damage)
            .map(|event| (event.target, event.attacker))
            .collect();
        if hits.is_empty() && self.cases.par_iter().all(|case| case.heals_at > tick) {
            return;
        }
        if hits.par_windows(2).any(|pair| pair[0] > pair[1]) {
            hits.par_sort_unstable();
        }
        let new: Vec<_> = hits
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| Case {
                person: group[0].0,
                sustained_at: tick,
                heals_at: tick.saturating_add(self.rules.recovery_ticks),
                attacker: group[0].1,
            })
            .collect();
        let mut people: Vec<_> = new.par_iter().map(|c| c.person).collect();
        people.par_sort_unstable();
        let mut cases: Vec<_> = self
            .cases
            .par_iter()
            .filter(|case| case.heals_at > tick && people.binary_search(&case.person).is_err())
            .cloned()
            .collect();
        cases.par_extend(new);
        cases.par_sort_unstable_by_key(|case| case.person);
        self.cases = cases;
    }

    /// Apply opening-generation capability to continuing jobs, without scanning
    /// every wound for every worker.
    pub fn reduce_work(&self, work: &[Work], rates: &mut Vec<(u64, u64)>, tick: u64) {
        if self.cases.is_empty() || self.rules.work_percent == 100 || rates.is_empty() {
            return;
        }
        let mut wounded: Vec<_> = self
            .cases
            .par_iter()
            .filter(|c| c.heals_at > tick)
            .map(|c| c.person)
            .collect();
        wounded.par_sort_unstable();
        let mut affected: Vec<_> = work
            .par_iter()
            .filter(|w| wounded.binary_search(&w.job.worker).is_ok())
            .map(|w| w.job.id)
            .collect();
        affected.par_sort_unstable();
        rates.par_iter_mut().for_each(|(job, rate)| {
            if affected.binary_search(job).is_ok() {
                *rate = ((u128::from(*rate) * u128::from(self.rules.work_percent)) / 100) as u64;
            }
        });
        if rates.par_iter().any(|row| row.1 == 0) {
            *rates = rates.par_iter().copied().filter(|row| row.1 > 0).collect();
        }
    }

    /// Treat only a wound present when the boundary opened; a new strike in
    /// the same boundary is not erased by medicine selected earlier.
    pub fn treat_opening(&mut self, opening: &Self, accepted: &[(u64, u32, u64)]) {
        let mut treated: Vec<_> = accepted
            .par_iter()
            .filter(|(_, good, quantity)| {
                *good == opening.rules.medicine && *quantity >= opening.rules.medicine_quantity
            })
            .map(|(patient, _, _)| *patient)
            .collect();
        if treated.is_empty() {
            return;
        }
        treated.par_sort_unstable();
        let mut old: Vec<_> = opening
            .cases
            .par_iter()
            .filter(|case| treated.binary_search(&case.person).is_ok())
            .map(|case| (case.person, case.sustained_at, case.heals_at, case.attacker))
            .collect();
        old.par_sort_unstable();
        self.cases = self
            .cases
            .par_iter()
            .filter(|case| {
                old.binary_search(&(case.person, case.sustained_at, case.heals_at, case.attacker))
                    .is_err()
            })
            .cloned()
            .collect();
    }
}

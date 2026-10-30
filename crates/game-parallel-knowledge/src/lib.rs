//! Bounded personal observations and voluntary encounter-based reports.
pub mod adapter;
use game_parallel_social::Encounter;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub max_records: usize,
    pub minds: Vec<Mind>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    #[serde(default)]
    pub records: Vec<Record>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub region: u64,
    pub food: u32,
    pub observed_at: u64,
    pub witness: u64,
    pub learned_from: Option<u64>,
}
#[derive(Clone, Copy)]
pub struct Observation {
    pub person: u64,
    pub health: u32,
    pub local: Option<(u64, u32)>,
}

impl Knowledge {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        now: u64,
    ) -> Result<(), &'static str> {
        if self.max_records == 0 {
            return Err("invalid knowledge capacity");
        }
        let mut minds = HashSet::new();
        for mind in &self.minds {
            if !people.contains(&mind.person)
                || !minds.insert(mind.person)
                || mind.records.len() > self.max_records
            {
                return Err("invalid personal mind");
            }
            let mut recorded = HashSet::new();
            for r in &mind.records {
                if !regions.contains(&r.region)
                    || !people.contains(&r.witness)
                    || r.observed_at > now
                    || r.learned_from
                        .is_some_and(|id| !people.contains(&id) || id == mind.person)
                    || !recorded.insert(r.region)
                {
                    return Err("invalid personal record");
                }
            }
        }
        Ok(())
    }
    /// Observations and reports read the same committed snapshot. Reports never
    /// relay a record learned earlier in this same phase.
    pub fn prepare(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
    ) -> Result<Self, &'static str> {
        self.prepare_inner(observations, regions, encounters, now, false, |_, _| true)
    }
    /// The compiled host has validated all owner identities and generated encounters.
    pub fn prepare_validated(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
    ) -> Result<Self, &'static str> {
        self.prepare_inner(observations, regions, encounters, now, true, |_, _| true)
    }
    /// Apply each receiver's personal judgment to a report, preserving direct observation.
    pub fn prepare_validated_with(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
        accept: impl Fn(u64, u64) -> bool + Sync,
    ) -> Result<Self, &'static str> {
        self.prepare_inner(observations, regions, encounters, now, true, accept)
    }
    fn prepare_inner(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
        validated: bool,
        accept: impl Fn(u64, u64) -> bool + Sync,
    ) -> Result<Self, &'static str> {
        if self.max_records == 0 {
            return Err("invalid knowledge capacity");
        }
        let ordered = if observations
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut rows = observations.to_vec();
            rows.par_sort_unstable_by_key(|o| o.person);
            Some(rows)
        } else {
            None
        };
        let observations = ordered.as_deref().unwrap_or(observations);
        if !validated
            && observations
                .par_windows(2)
                .any(|p| p[0].person == p[1].person)
        {
            return Err("duplicate observation person");
        }
        let mut minds: Vec<_> = self.minds.par_iter().collect();
        if minds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            minds.par_sort_unstable_by_key(|m| m.person);
        }
        if !validated
            && (minds.par_windows(2).any(|p| p[0].person == p[1].person)
                || minds.par_iter().any(|m| {
                    observations
                        .binary_search_by_key(&m.person, |o| o.person)
                        .is_err()
                        || m.records.len() > self.max_records
                }))
        {
            return Err("invalid personal mind");
        }
        if !validated
            && minds.par_iter().any(|mind| {
                mind.records.iter().enumerate().any(|(i, r)| {
                    !regions.contains(&r.region)
                        || observations
                            .binary_search_by_key(&r.witness, |o| o.person)
                            .is_err()
                        || r.observed_at > now
                        || r.learned_from.is_some_and(|id| {
                            id == mind.person
                                || observations
                                    .binary_search_by_key(&id, |o| o.person)
                                    .is_err()
                        })
                        || mind.records[..i].iter().any(|old| old.region == r.region)
                })
            })
        {
            return Err("invalid personal record");
        }
        let mut partners: Vec<_> = encounters
            .par_iter()
            .flat_map_iter(|e| [(e.a, e.b), (e.b, e.a)])
            .filter(|&(receiver, sender)| accept(receiver, sender))
            .collect();
        partners.par_sort_unstable();
        if !validated && partners.par_windows(2).any(|p| p[0].0 == p[1].0) {
            return Err("duplicate encounter participant");
        }
        if !validated
            && encounters.par_iter().any(|e| {
                let a = observations
                    .binary_search_by_key(&e.a, |o| o.person)
                    .ok()
                    .map(|i| observations[i]);
                let b = observations
                    .binary_search_by_key(&e.b, |o| o.person)
                    .ok()
                    .map(|i| observations[i]);
                e.tick != now.saturating_add(1)
                    || e.a >= e.b
                    || a.is_none_or(|p| p.health == 0 || p.local.is_none_or(|(r, _)| r != e.region))
                    || b.is_none_or(|p| p.health == 0 || p.local.is_none_or(|(r, _)| r != e.region))
            })
        {
            return Err("invalid encounter for knowledge");
        }
        let observation_base = observations.first().and_then(|first| {
            observations.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((observations.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let mind_base = minds.first().and_then(|first| {
            minds.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((minds.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let observation_index = |id: u64| {
            observation_base
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < observations.len())
                .or_else(|| {
                    if observation_base.is_none() {
                        observations
                            .binary_search_by_key(&id, |row| row.person)
                            .ok()
                    } else {
                        None
                    }
                })
        };
        let mind_index = |id: u64| {
            mind_base
                .and_then(|first| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < minds.len())
                .or_else(|| {
                    if mind_base.is_none() {
                        minds.binary_search_by_key(&id, |row| row.person).ok()
                    } else {
                        None
                    }
                })
        };
        let partner_slots: Vec<_> = if partners.is_empty() {
            Vec::new()
        } else {
            let partners = &partners;
            observations
                .par_chunks(4096)
                .flat_map_iter(|chunk| {
                    let mut index = partners.partition_point(|row| row.0 < chunk[0].person);
                    chunk.iter().map(move |observation| {
                        while index < partners.len() && partners[index].0 < observation.person {
                            index += 1;
                        }
                        partners
                            .get(index)
                            .filter(|row| row.0 == observation.person)
                            .map(|row| row.1)
                    })
                })
                .collect()
        };
        let next: Vec<Mind> = self
            .minds
            .par_iter()
            .map(|old| {
                let mut mind = old.clone();
                let position = observation_index(mind.person).expect("validated mind");
                let observation = observations[position];
                if observation.health == 0 {
                    return mind;
                }
                if let Some((region, food)) = observation.local {
                    let record = Record {
                        region,
                        food,
                        observed_at: now,
                        witness: mind.person,
                        learned_from: None,
                    };
                    if let Some(old) = mind.records.iter_mut().find(|old| old.region == region) {
                        *old = record;
                    } else {
                        remember(&mut mind.records, record, self.max_records, None, true);
                    }
                }
                if let Some(partner) = partner_slots.get(position).copied().flatten()
                    && let Some(j) = mind_index(partner)
                {
                    let local_region = observation.local.map(|(region, _)| region);
                    let report = minds[j]
                        .records
                        .iter()
                        .filter(|r| Some(r.region) != local_region)
                        .filter(|r| {
                            mind.records
                                .iter()
                                .find(|old| old.region == r.region)
                                .is_none_or(|old| {
                                    (r.observed_at, r.witness) > (old.observed_at, old.witness)
                                })
                        })
                        .max_by_key(|r| (r.observed_at, r.region, r.witness));
                    if let Some(report) = report {
                        let mut received = report.clone();
                        received.learned_from = Some(partner);
                        remember(
                            &mut mind.records,
                            received,
                            self.max_records,
                            local_region,
                            false,
                        );
                    }
                }
                mind.records.sort_unstable_by_key(|r| r.region);
                mind
            })
            .collect();
        Ok(Self {
            max_records: self.max_records,
            minds: next,
        })
    }
}
fn remember(
    records: &mut Vec<Record>,
    record: Record,
    limit: usize,
    protected: Option<u64>,
    force: bool,
) {
    if let Some(old) = records.iter_mut().find(|old| old.region == record.region) {
        if force || (record.observed_at, record.witness) >= (old.observed_at, old.witness) {
            *old = record;
        }
    } else {
        if records.len() == limit {
            let Some((i, oldest)) = records
                .iter()
                .enumerate()
                .filter(|(_, r)| Some(r.region) != protected)
                .min_by_key(|(_, r)| (r.observed_at, r.region, r.witness))
            else {
                return;
            };
            if !force
                && (record.observed_at, record.region, record.witness)
                    <= (oldest.observed_at, oldest.region, oldest.witness)
            {
                return;
            }
            records.remove(i);
        }
        records.push(record);
    }
}

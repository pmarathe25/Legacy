//! Bounded personal observations and voluntary encounter-based reports.
pub mod adapter;
use game_social::Encounter;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub max_records: usize,
    pub minds: Vec<Mind>,
    /// Optional per-person food-memory retention; absent people retain the
    /// existing bounded-record behavior.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retention: Vec<Retention>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    pub person: u64,
    pub ticks: u64,
    #[serde(default)]
    pub traces: Vec<RecallTrace>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallTrace {
    pub region: u64,
    pub reinforced_at: u64,
}

impl Retention {
    pub fn remembers(&self, record: &Record, now: u64) -> bool {
        let last = self
            .traces
            .iter()
            .find(|trace| trace.region == record.region)
            .map_or(record.observed_at, |trace| trace.reinforced_at);
        now.saturating_sub(last) < self.ticks
    }
    fn reinforce(&mut self, region: u64, tick: u64) {
        if let Some(trace) = self.traces.iter_mut().find(|trace| trace.region == region) {
            trace.reinforced_at = trace.reinforced_at.max(tick);
        } else {
            self.traces.push(RecallTrace {
                region,
                reinforced_at: tick,
            });
        }
    }
}

/// Immutable lookup used by decisions and views; examining a memory is passive.
pub struct RecallIndex<'a> {
    rows: Vec<&'a Retention>,
}
impl RecallIndex<'_> {
    pub fn remembers(&self, person: u64, record: &Record, now: u64) -> bool {
        self.rows
            .binary_search_by_key(&person, |row| row.person)
            .ok()
            .is_none_or(|index| self.rows[index].remembers(record, now))
    }
}

enum ReportUses {
    Dense {
        base: u64,
        slots: Vec<(AtomicU64, AtomicU64)>,
    },
    Sparse(Vec<(u64, AtomicU64, AtomicU64)>),
}
impl ReportUses {
    fn new(base: Option<u64>, population: usize, encounters: &[Encounter]) -> Self {
        if let Some(base) = base
            && encounters.len().saturating_mul(2) >= population / 2
        {
            return Self::Dense {
                base,
                slots: (0..population)
                    .into_par_iter()
                    .map(|_| (AtomicU64::new(0), AtomicU64::new(0)))
                    .collect(),
            };
        }
        let mut people: Vec<_> = encounters
            .par_iter()
            .flat_map_iter(|event| [event.a, event.b])
            .collect();
        if people.par_windows(2).any(|pair| pair[0] > pair[1]) {
            people.par_sort_unstable();
        }
        Self::Sparse(
            people
                .par_chunk_by(|a, b| a == b)
                .map(|group| (group[0], AtomicU64::new(0), AtomicU64::new(0)))
                .collect(),
        )
    }
    fn record(&self, person: u64, source: u64, region: u64) {
        let slot = match self {
            Self::Dense { base, slots } => person
                .checked_sub(*base)
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|index| slots.get(index))
                .map(|(a, b)| (a, b)),
            Self::Sparse(rows) => rows
                .binary_search_by_key(&person, |row| row.0)
                .ok()
                .map(|index| (&rows[index].1, &rows[index].2)),
        };
        if let Some((a, b)) = slot {
            a.store(source, Ordering::Relaxed);
            b.store(region, Ordering::Relaxed);
        }
    }
    fn pairs(&self) -> Vec<(u64, u64)> {
        let collect = |person: u64, source: &AtomicU64, region: &AtomicU64| {
            let source = source.load(Ordering::Relaxed);
            let region = region.load(Ordering::Relaxed);
            (source != 0 && region != 0)
                .then_some([(person, region), (source, region)])
                .into_iter()
                .flatten()
        };
        match self {
            Self::Dense { base, slots } => slots
                .par_iter()
                .enumerate()
                .flat_map_iter(|(index, (source, region))| {
                    collect(*base + index as u64, source, region)
                })
                .collect(),
            Self::Sparse(rows) => rows
                .par_iter()
                .flat_map_iter(|(person, source, region)| collect(*person, source, region))
                .collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    /// This person chooses to remember where they met other people.
    #[serde(default)]
    pub remember_people: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people_seen: Vec<SeenPerson>,
    /// Independent willingness to report one's own past encounters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub people_reporting: Option<Reporting>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people_reports: Vec<PersonReport>,
    /// A private choice to share, optionally motivated by pressure from the listener.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting: Option<Reporting>,
    /// A private choice about what to tell a particular listener, or everyone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misreport: Option<Misreport>,
    #[serde(default)]
    pub records: Vec<Record>,
}
/// A personal memory of an actual local encounter, not a current location claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeenPerson {
    pub person: u64,
    pub region: u64,
    pub observed_at: u64,
}
/// A statement about a past encounter, not a claim about the person's current location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonReport {
    pub person: u64,
    pub region: u64,
    pub observed_at: u64,
    pub speaker: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reporting {
    pub willing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_threshold: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Misreport {
    #[serde(default)]
    pub target: Option<u64>,
    /// Signed change to the quantity in the speaker's remembered report.
    pub food_delta: i64,
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
impl Record {
    /// Produce a statement for a listener without changing the speaker's memory.
    pub fn stated_to(&self, sender: u64, listener: u64, misreport: Option<Misreport>) -> Self {
        let mut statement = self.clone();
        if let Some(policy) = misreport.filter(|p| p.target.is_none_or(|id| id == listener)) {
            statement.food = (i128::from(statement.food) + i128::from(policy.food_delta))
                .clamp(0, i128::from(u32::MAX)) as u32;
        }
        statement.learned_from = Some(sender);
        statement
    }
}
#[derive(Clone, Copy)]
pub struct Observation {
    pub person: u64,
    pub health: u32,
    pub local: Option<(u64, u32)>,
}
/// A personal observation differs from a remembered report. Stock can change,
/// so this observation does not establish whether the speaker was dishonest.
pub fn report_discrepancy(records: &[Record], observation: Observation) -> Option<(u64, u64)> {
    let (region, food) = observation.local.filter(|_| observation.health > 0)?;
    let previous = records.iter().find(|record| record.region == region)?;
    let speaker = previous.learned_from?;
    (previous.food != food && speaker != observation.person)
        .then_some((observation.person, speaker))
}

impl Knowledge {
    pub fn recall_index(&self) -> RecallIndex<'_> {
        let mut rows: Vec<_> = self.retention.par_iter().collect();
        if rows
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            rows.par_sort_unstable_by_key(|row| row.person);
        }
        RecallIndex { rows }
    }
    /// Recall is a pure view: looking at a memory does not reinforce it.
    pub fn recall_at(&self, person: u64, now: u64) -> Option<Vec<Record>> {
        let mind = self.minds.iter().find(|mind| mind.person == person)?;
        let index = self.recall_index();
        Some(
            mind.records
                .iter()
                .filter(|record| index.remembers(person, record, now))
                .cloned()
                .collect(),
        )
    }
    /// Apply accepted reception at the actual boundary without changing the claim's age.
    pub fn receive_reports_at(
        &mut self,
        reports: &[(u64, Record, Option<u64>)],
        now: u64,
    ) -> Result<(), &'static str> {
        if reports.par_windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("received reports must have unique ordered recipients");
        }
        if !self.retention.is_empty() {
            let mut policies: Vec<_> = self.retention.par_iter().collect();
            if policies
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                policies.par_sort_unstable_by_key(|policy| policy.person);
            }
            self.minds.par_iter_mut().for_each(|mind| {
                if reports
                    .binary_search_by_key(&mind.person, |row| row.0)
                    .is_ok()
                    && let Ok(index) =
                        policies.binary_search_by_key(&mind.person, |policy| policy.person)
                {
                    mind.records
                        .retain(|record| policies[index].remembers(record, now));
                }
            });
        }
        self.receive_reports(reports)?;
        if self.retention.is_empty() {
            return Ok(());
        }
        let mut minds: Vec<_> = self.minds.par_iter().collect();
        if minds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            minds.par_sort_unstable_by_key(|mind| mind.person);
        }
        self.retention.par_iter_mut().for_each(|policy| {
            let Some((_, report, _)) = reports
                .binary_search_by_key(&policy.person, |row| row.0)
                .ok()
                .map(|i| &reports[i])
            else {
                return;
            };
            let Some(mind) = minds
                .binary_search_by_key(&policy.person, |mind| mind.person)
                .ok()
                .map(|i| minds[i])
            else {
                return;
            };
            policy.traces.retain(|trace| {
                mind.records
                    .iter()
                    .any(|record| record.region == trace.region)
            });
            if report.observed_at <= now
                && mind
                    .records
                    .iter()
                    .any(|record| same_evidence(record, report))
            {
                policy.reinforce(report.region, now);
            }
        });
        Ok(())
    }
    pub fn receive_report_at(
        &mut self,
        recipient: u64,
        report: Record,
        local_region: Option<u64>,
        now: u64,
    ) -> bool {
        if let (Some(policy), Some(mind)) = (
            self.retention.iter().find(|row| row.person == recipient),
            self.minds.iter_mut().find(|mind| mind.person == recipient),
        ) {
            mind.records.retain(|record| policy.remembers(record, now));
        }
        let changed = self.receive_report(recipient, report.clone(), local_region);
        if let (Some(policy), Some(mind)) = (
            self.retention
                .iter_mut()
                .find(|row| row.person == recipient),
            self.minds.iter().find(|mind| mind.person == recipient),
        ) {
            policy.traces.retain(|trace| {
                mind.records
                    .iter()
                    .any(|record| record.region == trace.region)
            });
            if report.observed_at <= now
                && mind
                    .records
                    .iter()
                    .any(|record| same_evidence(record, &report))
            {
                policy.reinforce(report.region, now);
            }
        }
        changed
    }
    /// Apply at most one received statement per person in a sorted independent batch.
    pub fn receive_reports(
        &mut self,
        reports: &[(u64, Record, Option<u64>)],
    ) -> Result<(), &'static str> {
        if reports.par_windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("received reports must have unique ordered recipients");
        }
        let limit = self.max_records;
        let first = reports.first().map(|row| row.0);
        self.minds.par_iter_mut().for_each(|mind| {
            let direct = first
                .and_then(|id| mind.person.checked_sub(id))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| reports.get(index).is_some_and(|row| row.0 == mind.person));
            if let Some(index) =
                direct.or_else(|| reports.binary_search_by_key(&mind.person, |row| row.0).ok())
            {
                let (_, report, local) = &reports[index];
                remember(&mut mind.records, report.clone(), limit, *local, false);
                mind.records.sort_unstable_by_key(|record| record.region);
            }
        });
        if !self.retention.is_empty() {
            let mut minds: Vec<_> = self.minds.par_iter().collect();
            if minds
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                minds.par_sort_unstable_by_key(|mind| mind.person);
            }
            self.retention.par_iter_mut().for_each(|policy| {
                if let Ok(index) = minds.binary_search_by_key(&policy.person, |mind| mind.person) {
                    policy.traces.retain(|trace| {
                        minds[index].records.iter().any(|record| {
                            record.region == trace.region
                                && record.observed_at <= trace.reinforced_at
                        })
                    });
                }
            });
        }
        Ok(())
    }
    /// Store one received statement under the same bounded, recency-aware rules
    /// as ordinary reports. The caller decides whether the listener accepts it.
    pub fn receive_report(
        &mut self,
        recipient: u64,
        report: Record,
        local_region: Option<u64>,
    ) -> bool {
        let Some(mind) = self.minds.iter_mut().find(|mind| mind.person == recipient) else {
            return false;
        };
        let before = mind.records.clone();
        remember(
            &mut mind.records,
            report,
            self.max_records,
            local_region,
            false,
        );
        mind.records.sort_unstable_by_key(|record| record.region);
        let changed = mind.records != before;
        if let Some(policy) = self
            .retention
            .iter_mut()
            .find(|row| row.person == recipient)
        {
            policy.traces.retain(|trace| {
                mind.records.iter().any(|record| {
                    record.region == trace.region && record.observed_at <= trace.reinforced_at
                })
            });
        }
        changed
    }
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
                || mind.people_seen.len() > self.max_records
                || mind.people_reports.len() > self.max_records
                || mind
                    .misreport
                    .and_then(|p| p.target)
                    .is_some_and(|id| id == mind.person || !people.contains(&id))
            {
                return Err("invalid personal mind");
            }
            let mut seen_people = HashSet::new();
            if mind.people_seen.iter().any(|seen| {
                seen.person == mind.person
                    || !people.contains(&seen.person)
                    || !regions.contains(&seen.region)
                    || seen.observed_at > now
                    || !seen_people.insert(seen.person)
            }) {
                return Err("invalid personal encounter memory");
            }
            let mut reported_people = HashSet::new();
            if mind.people_reports.iter().any(|report| {
                report.person == mind.person
                    || report.speaker == mind.person
                    || report.speaker == report.person
                    || !people.contains(&report.person)
                    || !people.contains(&report.speaker)
                    || !regions.contains(&report.region)
                    || report.observed_at > now
                    || !reported_people.insert(report.person)
            }) {
                return Err("invalid personal encounter report");
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
        if !self.retention.is_empty() {
            let mind_by_person: HashMap<_, _> =
                self.minds.iter().map(|mind| (mind.person, mind)).collect();
            let mut policies = HashSet::new();
            if self.retention.iter().any(|policy| {
                let Some(mind) = mind_by_person.get(&policy.person) else {
                    return true;
                };
                let mut regions_seen = HashSet::new();
                policy.ticks == 0
                    || !policies.insert(policy.person)
                    || policy.traces.len() > self.max_records
                    || policy.traces.iter().any(|trace| {
                        !regions.contains(&trace.region)
                            || trace.reinforced_at > now
                            || !regions_seen.insert(trace.region)
                            || mind
                                .records
                                .iter()
                                .find(|record| record.region == trace.region)
                                .is_none_or(|record| trace.reinforced_at < record.observed_at)
                    })
            }) {
                return Err("invalid food-memory retention");
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
        self.prepare_inner(
            observations,
            regions,
            encounters,
            now,
            false,
            (|_, _| true, |_, _| 0),
        )
    }
    /// The compiled host has validated all owner identities and generated encounters.
    pub fn prepare_validated(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
    ) -> Result<Self, &'static str> {
        self.prepare_inner(
            observations,
            regions,
            encounters,
            now,
            true,
            (|_, _| true, |_, _| 0),
        )
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
        self.prepare_inner(
            observations,
            regions,
            encounters,
            now,
            true,
            (accept, |_, _| 0),
        )
    }
    /// Apply the speaker's own fear and obligation separately from the listener's judgment.
    pub fn prepare_validated_with_pressure(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
        accept: impl Fn(u64, u64) -> bool + Sync,
        pressure: impl Fn(u64, u64) -> u64 + Sync,
    ) -> Result<Self, &'static str> {
        self.prepare_inner(
            observations,
            regions,
            encounters,
            now,
            true,
            (accept, pressure),
        )
    }
    fn prepare_inner(
        &self,
        observations: &[Observation],
        regions: &HashSet<u64>,
        encounters: &[Encounter],
        now: u64,
        validated: bool,
        policies: (
            impl Fn(u64, u64) -> bool + Sync,
            impl Fn(u64, u64) -> u64 + Sync,
        ),
    ) -> Result<Self, &'static str> {
        let (accept, pressure) = policies;
        if self.max_records == 0 {
            return Err("invalid knowledge capacity");
        }
        if !validated && !self.retention.is_empty() {
            let people: HashSet<_> = observations.par_iter().map(|row| row.person).collect();
            self.validate(&people, regions, now)?;
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
                        || m.people_seen.len() > self.max_records
                        || m.people_reports.len() > self.max_records
                        || m.misreport.and_then(|p| p.target).is_some_and(|id| {
                            id == m.person
                                || observations
                                    .binary_search_by_key(&id, |o| o.person)
                                    .is_err()
                        })
                }))
        {
            return Err("invalid personal mind");
        }
        if !validated
            && minds.par_iter().any(|mind| {
                mind.people_seen.iter().enumerate().any(|(i, seen)| {
                    seen.person == mind.person
                        || !regions.contains(&seen.region)
                        || seen.observed_at > now
                        || observations
                            .binary_search_by_key(&seen.person, |o| o.person)
                            .is_err()
                        || mind.people_seen[..i]
                            .iter()
                            .any(|old| old.person == seen.person)
                }) || mind.people_reports.iter().enumerate().any(|(i, report)| {
                    report.person == mind.person
                        || report.speaker == mind.person
                        || report.speaker == report.person
                        || !regions.contains(&report.region)
                        || report.observed_at > now
                        || observations
                            .binary_search_by_key(&report.person, |o| o.person)
                            .is_err()
                        || observations
                            .binary_search_by_key(&report.speaker, |o| o.person)
                            .is_err()
                        || mind.people_reports[..i]
                            .iter()
                            .any(|old| old.person == report.person)
                }) || mind.records.iter().enumerate().any(|(i, r)| {
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
        let observation_base = observations.first().and_then(|first| {
            observations.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((observations.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let dense_partners = observation_base
            .filter(|_| validated && !encounters.is_empty())
            .filter(|_| {
                observations
                    .last()
                    .is_some_and(|last| last.person != u64::MAX)
            })
            .map(|first| {
                let slots: Vec<_> = (0..observations.len())
                    .into_par_iter()
                    .map(|_| AtomicU64::new(u64::MAX))
                    .collect();
                let index = |id: u64| {
                    id.checked_sub(first)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .filter(|&i| i < slots.len())
                };
                encounters.par_iter().for_each(|event| {
                    if let Some(a) = index(event.a)
                        && accept(event.a, event.b)
                    {
                        slots[a].fetch_min(event.b, Ordering::Relaxed);
                    }
                    if let Some(b) = index(event.b)
                        && accept(event.b, event.a)
                    {
                        slots[b].fetch_min(event.a, Ordering::Relaxed);
                    }
                });
                slots
            });
        let mut partners: Vec<_> = if dense_partners.is_some() {
            Vec::new()
        } else {
            encounters
                .par_iter()
                .flat_map_iter(|e| [(e.a, e.b), (e.b, e.a)])
                .filter(|&(receiver, sender)| accept(receiver, sender))
                .collect()
        };
        if dense_partners.is_none() {
            partners.par_sort_unstable();
        }
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
        let mut seen_contacts: Vec<(u64, u64, u64, u64)> =
            if !encounters.is_empty() && minds.par_iter().any(|mind| mind.remember_people) {
                encounters
                    .par_iter()
                    .filter(|event| event.tick == now.saturating_add(1))
                    .flat_map_iter(|event| {
                        [
                            (event.a, event.b, event.region, event.tick),
                            (event.b, event.a, event.region, event.tick),
                        ]
                    })
                    .collect()
            } else {
                Vec::new()
            };
        seen_contacts.par_sort_unstable();
        let recall = self.recall_index();
        let report_uses = (!self.retention.is_empty() && !encounters.is_empty())
            .then(|| ReportUses::new(observation_base, observations.len(), encounters));
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
                if !self.retention.is_empty() {
                    mind.records
                        .retain(|record| recall.remembers(mind.person, record, now));
                }
                if mind.remember_people {
                    let start = seen_contacts.partition_point(|row| row.0 < mind.person);
                    let end = seen_contacts.partition_point(|row| row.0 <= mind.person);
                    if let Some(&(_, other, region, observed_at)) = seen_contacts[start..end]
                        .iter()
                        .find(|&&(_, other, region, _)| {
                            observation.local.is_some_and(|(here, _)| here == region)
                                && observation_index(other).is_some_and(|i| {
                                    observations[i].health > 0
                                        && observations[i]
                                            .local
                                            .is_some_and(|(here, _)| here == region)
                                })
                        })
                    {
                        let seen = SeenPerson {
                            person: other,
                            region,
                            observed_at,
                        };
                        if let Some(previous) =
                            mind.people_seen.iter_mut().find(|row| row.person == other)
                        {
                            *previous = seen;
                        } else {
                            if mind.people_seen.len() == self.max_records {
                                let oldest = mind
                                    .people_seen
                                    .iter()
                                    .enumerate()
                                    .min_by_key(|(_, row)| (row.observed_at, row.person))
                                    .map(|(i, _)| i)
                                    .expect("positive memory capacity");
                                mind.people_seen.remove(oldest);
                            }
                            mind.people_seen.push(seen);
                        }
                        mind.people_seen.sort_unstable_by_key(|row| row.person);
                    }
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
                let partner = if let Some(slots) = &dense_partners {
                    let value = slots[position].load(Ordering::Relaxed);
                    (value != u64::MAX).then_some(value)
                } else {
                    partner_slots.get(position).copied().flatten()
                };
                if let Some(partner) = partner
                    && let Some(j) = mind_index(partner)
                    && minds[j].reporting.is_none_or(|choice| {
                        choice.willing
                            || choice.pressure_threshold.is_some_and(|threshold| {
                                pressure(partner, mind.person) >= threshold
                            })
                    })
                {
                    let local_region = observation.local.map(|(region, _)| region);
                    let report = minds[j]
                        .records
                        .iter()
                        .filter(|record| recall.remembers(partner, record, now))
                        .filter(|r| Some(r.region) != local_region)
                        .filter(|r| {
                            mind.records
                                .iter()
                                .find(|old| old.region == r.region)
                                .is_none_or(|old| {
                                    (r.observed_at, r.witness) > (old.observed_at, old.witness)
                                        || (!self.retention.is_empty()
                                            && same_evidence(
                                                old,
                                                &r.stated_to(
                                                    partner,
                                                    mind.person,
                                                    minds[j].misreport,
                                                ),
                                            ))
                                })
                        })
                        .max_by_key(|r| {
                            let newer = self.retention.is_empty()
                                || mind
                                    .records
                                    .iter()
                                    .find(|old| old.region == r.region)
                                    .is_none_or(|old| {
                                        (r.observed_at, r.witness) > (old.observed_at, old.witness)
                                    });
                            (newer, r.observed_at, r.region, r.witness)
                        });
                    if let Some(report) = report {
                        let received = report.stated_to(partner, mind.person, minds[j].misreport);
                        if !mind.records.iter().any(|old| same_evidence(old, &received)) {
                            remember(
                                &mut mind.records,
                                received.clone(),
                                self.max_records,
                                local_region,
                                false,
                            );
                        }
                        if let Some(uses) = &report_uses
                            && mind.records.iter().any(|old| same_evidence(old, &received))
                        {
                            uses.record(mind.person, partner, received.region);
                        }
                    }
                }
                if let Some(partner) = partner
                    && let Some(j) = mind_index(partner)
                    && minds[j].people_reporting.is_some_and(|choice| {
                        choice.willing
                            || choice.pressure_threshold.is_some_and(|threshold| {
                                pressure(partner, mind.person) >= threshold
                            })
                    })
                    && let Some(seen) = minds[j]
                        .people_seen
                        .iter()
                        .filter(|seen| seen.person != mind.person)
                        .filter(|seen| {
                            mind.people_seen
                                .iter()
                                .find(|old| old.person == seen.person)
                                .is_none_or(|old| old.observed_at < seen.observed_at)
                        })
                        .filter(|seen| {
                            mind.people_reports
                                .iter()
                                .find(|old| old.person == seen.person)
                                .is_none_or(|old| {
                                    (old.observed_at, old.speaker) < (seen.observed_at, partner)
                                })
                        })
                        .max_by_key(|seen| (seen.observed_at, seen.person))
                {
                    let report = PersonReport {
                        person: seen.person,
                        region: seen.region,
                        observed_at: seen.observed_at,
                        speaker: partner,
                    };
                    if let Some(old) = mind
                        .people_reports
                        .iter_mut()
                        .find(|old| old.person == seen.person)
                    {
                        *old = report;
                    } else {
                        if mind.people_reports.len() == self.max_records {
                            let (oldest, old) = mind
                                .people_reports
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, row)| (row.observed_at, row.person, row.speaker))
                                .expect("positive memory capacity");
                            if (report.observed_at, report.person, report.speaker)
                                > (old.observed_at, old.person, old.speaker)
                            {
                                mind.people_reports.remove(oldest);
                            }
                        }
                        if mind.people_reports.len() < self.max_records {
                            mind.people_reports.push(report);
                        }
                    }
                    mind.people_reports.sort_unstable_by_key(|row| row.person);
                }
                mind.records.sort_unstable_by_key(|r| r.region);
                mind
            })
            .collect();
        let mut reinforced: Vec<_> = report_uses
            .as_ref()
            .map_or_else(Vec::new, ReportUses::pairs);
        reinforced.par_sort_unstable();
        let retention = if self.retention.is_empty() {
            Vec::new()
        } else {
            let mut next_index: Vec<_> = next.par_iter().collect();
            if next_index
                .par_windows(2)
                .any(|pair| pair[0].person > pair[1].person)
            {
                next_index.par_sort_unstable_by_key(|mind| mind.person);
            }
            self.retention
                .par_iter()
                .map(|old_policy| {
                    let mut policy = old_policy.clone();
                    let Some(old_mind) = mind_index(policy.person).map(|i| minds[i]) else {
                        return policy;
                    };
                    let Some(new_mind) = next_index
                        .binary_search_by_key(&policy.person, |mind| mind.person)
                        .ok()
                        .map(|i| next_index[i])
                    else {
                        return policy;
                    };
                    let observation = observation_index(policy.person).map(|i| observations[i]);
                    policy.traces.retain(|trace| {
                        new_mind
                            .records
                            .iter()
                            .any(|record| record.region == trace.region)
                    });
                    for record in &new_mind.records {
                        let locally_observed = observation.is_some_and(|row| {
                            row.health > 0
                                && row.local.is_some_and(|(region, _)| region == record.region)
                        });
                        let changed = old_mind
                            .records
                            .iter()
                            .find(|old| old.region == record.region)
                            .is_none_or(|old| {
                                old != record || !recall.remembers(policy.person, old, now)
                            });
                        if locally_observed {
                            policy.reinforce(record.region, now);
                        } else if changed {
                            policy.reinforce(record.region, now.saturating_add(1));
                        }
                    }
                    let begin = reinforced.partition_point(|row| row.0 < policy.person);
                    let end = reinforced.partition_point(|row| row.0 <= policy.person);
                    for &(_, region) in &reinforced[begin..end] {
                        if new_mind
                            .records
                            .iter()
                            .any(|record| record.region == region)
                        {
                            policy.reinforce(region, now.saturating_add(1));
                        }
                    }
                    policy
                })
                .collect()
        };
        Ok(Self {
            max_records: self.max_records,
            minds: next,
            retention,
        })
    }
}
fn same_evidence(a: &Record, b: &Record) -> bool {
    (a.region, a.food, a.observed_at, a.witness) == (b.region, b.food, b.observed_at, b.witness)
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

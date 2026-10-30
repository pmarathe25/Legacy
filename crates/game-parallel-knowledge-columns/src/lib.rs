//! Persistent bounded personal records with committed/scratch identity blocks.
use game_parallel_knowledge::{Knowledge, Mind, Observation, Record};
use game_parallel_social::Encounter;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::sync::atomic::{AtomicU64, Ordering};
const BLOCK: usize = 1024;
static INSTANCE: AtomicU64 = AtomicU64::new(1);
type Records = SmallVec<[Record; 4]>;
/// Host supplies committed health/locality/food without a population-wide adapter vector.
pub trait ObservationView: Sync {
    fn read(&self, person: u64) -> Observation;
}
#[derive(Clone, Copy)]
pub struct Read<'a> {
    ids: &'a [u64],
    current: &'a [Vec<Records>],
}
impl<'a> Read<'a> {
    pub fn records(self, person: u64) -> Option<&'a [Record]> {
        let slot = locate(self.ids, person)?;
        Some(&self.current[slot / BLOCK][slot % BLOCK])
    }
}
pub struct Preparation<'a> {
    read: Read<'a>,
    next: &'a mut [Vec<Records>],
    limit: usize,
    draft: Draft,
}
pub struct Runtime {
    ids: Vec<u64>,
    order: Vec<u64>,
    current: Vec<Vec<Records>>,
    next: Vec<Vec<Records>>,
    max_records: usize,
    tick: u64,
    epoch: u64,
    instance: u64,
}
pub struct Draft {
    tick: u64,
    epoch: u64,
    instance: u64,
}
fn locate(ids: &[u64], id: u64) -> Option<usize> {
    let first = *ids.first()?;
    if ids.last()?.checked_sub(first) == Some((ids.len() - 1) as u64) {
        usize::try_from(id.checked_sub(first)?)
            .ok()
            .filter(|&i| i < ids.len())
    } else {
        ids.binary_search(&id).ok()
    }
}
impl Runtime {
    /// Compile after world identity, region, witness and clock validation.
    pub fn compile_validated(owner: &Knowledge, tick: u64) -> Result<Self, &'static str> {
        if owner.max_records == 0
            || owner
                .minds
                .iter()
                .any(|m| m.records.len() > owner.max_records)
        {
            return Err("invalid knowledge capacity");
        }
        let mut minds: Vec<_> = owner.minds.iter().collect();
        minds.par_sort_unstable_by_key(|m| m.person);
        if minds.windows(2).any(|p| p[0].person == p[1].person) {
            return Err("duplicate mind");
        }
        let ids = minds.iter().map(|m| m.person).collect();
        let (current, next) = minds
            .par_chunks(BLOCK)
            .map(|chunk| {
                let current: Vec<Records> = chunk
                    .iter()
                    .map(|m| m.records.iter().cloned().collect())
                    .collect();
                let next = current.clone();
                (current, next)
            })
            .unzip();
        Ok(Self {
            ids,
            current,
            next,
            order: owner.minds.iter().map(|m| m.person).collect(),
            max_records: owner.max_records,
            tick,
            epoch: 0,
            instance: INSTANCE.fetch_add(1, Ordering::Relaxed),
        })
    }
    pub fn records(&self, person: u64) -> Option<&[Record]> {
        self.read().records(person)
    }
    pub fn export(&self) -> Knowledge {
        Knowledge {
            max_records: self.max_records,
            minds: self
                .order
                .par_iter()
                .map(|&person| Mind {
                    person,
                    records: self.records(person).unwrap().to_vec(),
                })
                .collect(),
        }
    }
    pub fn read(&self) -> Read<'_> {
        Read {
            ids: &self.ids,
            current: &self.current,
        }
    }
    pub fn begin(&mut self, now: u64) -> Result<Preparation<'_>, &'static str> {
        if now != self.tick {
            return Err("knowledge phase must advance one tick");
        }
        let tick = now.checked_add(1).ok_or("knowledge clock overflow")?;
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("knowledge generation overflow")?;
        Ok(Preparation {
            read: Read {
                ids: &self.ids,
                current: &self.current,
            },
            next: &mut self.next,
            limit: self.max_records,
            draft: Draft {
                tick,
                epoch: self.epoch,
                instance: self.instance,
            },
        })
    }
    pub fn prepare_validated(
        &mut self,
        view: &impl ObservationView,
        encounters: &[Encounter],
        now: u64,
    ) -> Result<Draft, &'static str> {
        Ok(self.begin(now)?.prepare(view, encounters))
    }
    pub fn check(&self, draft: &Draft) -> Result<(), &'static str> {
        if draft.instance != self.instance
            || draft.epoch != self.epoch
            || self.tick.checked_add(1) != Some(draft.tick)
        {
            return Err("stale knowledge draft");
        }
        Ok(())
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), &'static str> {
        self.check(&draft)?;
        std::mem::swap(&mut self.current, &mut self.next);
        self.tick = draft.tick;
        Ok(())
    }
    /// Append canonical newborn identities after the complete world boundary commits.
    pub fn enroll(&mut self, births: &[u64]) -> Result<(), &'static str> {
        if births.is_empty() {
            return Ok(());
        }
        if births.par_windows(2).any(|p| p[0] >= p[1])
            || self.ids.last().is_some_and(|id| *id >= births[0])
        {
            return Err("newborn minds must append unique canonical identities");
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("knowledge generation overflow")?;
        let partial = self.ids.len() % BLOCK;
        let tail = if partial == 0 {
            0
        } else {
            (BLOCK - partial).min(births.len())
        };
        if tail > 0 {
            self.current
                .last_mut()
                .expect("partial knowledge block")
                .extend((0..tail).map(|_| Records::new()));
            self.next
                .last_mut()
                .expect("partial knowledge block")
                .extend((0..tail).map(|_| Records::new()));
        }
        let blocks: Vec<_> = births[tail..]
            .par_chunks(BLOCK)
            .map(|chunk| {
                let current = vec![Records::new(); chunk.len()];
                let next = current.clone();
                (current, next)
            })
            .collect();
        self.ids.par_extend(births.par_iter().copied());
        self.order.par_extend(births.par_iter().copied());
        for (current, next) in blocks {
            self.current.push(current);
            self.next.push(next);
        }
        Ok(())
    }
}
fn remember(
    records: &mut Records,
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

impl<'a> Preparation<'a> {
    pub fn read(&self) -> Read<'a> {
        self.read
    }
    pub fn prepare(self, view: &impl ObservationView, encounters: &[Encounter]) -> Draft {
        self.prepare_with(view, encounters, |_, _| true)
    }
    /// Apply the recipient's committed directed trust to reports.
    pub fn prepare_trusted(
        self,
        view: &impl ObservationView,
        encounters: &[Encounter],
        opinions: Option<&game_parallel_opinions::Opinions>,
    ) -> Draft {
        let Some(owner) = opinions.filter(|owner| owner.rules.minimum_report_trust.is_some())
        else {
            return self.prepare(view, encounters);
        };
        let threshold = owner
            .rules
            .minimum_report_trust
            .expect("configured threshold");
        let trust = owner.index();
        self.prepare_with(view, encounters, |receiver, sender| {
            trust.trust(receiver, sender) >= threshold
        })
    }
    /// Filter each directed report independently, while preserving direct observations.
    pub fn prepare_with(
        self,
        view: &impl ObservationView,
        encounters: &[Encounter],
        accept: impl Fn(u64, u64) -> bool + Sync,
    ) -> Draft {
        let now = self.draft.tick - 1;
        let mut partners: Vec<_> = encounters
            .par_iter()
            .flat_map_iter(|e| [(e.a, e.b), (e.b, e.a)])
            .filter(|&(receiver, sender)| accept(receiver, sender))
            .collect();
        partners.par_sort_unstable();
        let ids = self.read.ids;
        let blocks = self.read.current;
        let limit = self.limit;
        self.next
            .par_iter_mut()
            .enumerate()
            .for_each(|(block_index, rows)| {
                let base = block_index * BLOCK;
                let mut partner_index = partners.partition_point(|p| p.0 < ids[base]);
                for (offset, records) in rows.iter_mut().enumerate() {
                    let person = ids[base + offset];
                    records.clone_from(&blocks[block_index][offset]);
                    let observation = view.read(person);
                    if observation.health == 0 {
                        continue;
                    }
                    if let Some((region, food)) = observation.local {
                        remember(
                            records,
                            Record {
                                region,
                                food,
                                observed_at: now,
                                witness: person,
                                learned_from: None,
                            },
                            limit,
                            None,
                            true,
                        );
                    }
                    while partner_index < partners.len() && partners[partner_index].0 < person {
                        partner_index += 1;
                    }
                    if let Some(&(_, partner)) =
                        partners.get(partner_index).filter(|p| p.0 == person)
                        && let Some(slot) = locate(ids, partner)
                    {
                        let local = observation.local.map(|(region, _)| region);
                        let report = blocks[slot / BLOCK][slot % BLOCK]
                            .iter()
                            .filter(|r| Some(r.region) != local)
                            .filter(|r| {
                                records
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
                            remember(records, received, limit, local, false);
                        }
                    }
                    records.sort_unstable_by_key(|r| r.region);
                }
            });
        self.draft
    }
}

#[cfg(test)]
#[path = "../tests/support/differential.rs"]
mod differential;

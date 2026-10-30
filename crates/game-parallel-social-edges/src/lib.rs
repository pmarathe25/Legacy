//! Persistent paged social relationships with sparse transactional updates and expiry.
pub mod explicit;
use game_parallel_social::{Encounter, Relationship, Rules};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    sync::atomic::{AtomicU64, Ordering},
};
const SHARDS: usize = 64;
const PAGE: usize = 256;
static INSTANCE: AtomicU64 = AtomicU64::new(1);
type Key = (u64, u64);
fn key(r: &Relationship) -> Key {
    (r.a, r.b)
}
fn shard((a, b): Key) -> usize {
    let mut x = a.wrapping_mul(0x9e3779b97f4a7c15) ^ b;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    ((x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb) as usize) & (SHARDS - 1)
}
fn expiry(r: &Relationship, retention: Option<u64>) -> Option<u64> {
    r.last_met.checked_add(retention?)?.checked_add(1)
}
#[derive(Default)]
struct Entry {
    row: Option<Relationship>,
    expiry_offset: usize,
}
#[derive(Default)]
struct Shard {
    pages: Vec<Vec<Entry>>,
    free: Vec<usize>,
    index: HashMap<Key, usize>,
    expiry: BTreeMap<u64, Vec<usize>>,
}
impl Shard {
    fn get(&self, slot: usize) -> &Relationship {
        self.pages[slot / PAGE][slot % PAGE].row.as_ref().unwrap()
    }
    fn schedule(&mut self, slot: usize, due: Option<u64>) {
        if let Some(due) = due {
            let bucket = self.expiry.entry(due).or_default();
            self.pages[slot / PAGE][slot % PAGE].expiry_offset = bucket.len();
            bucket.push(slot);
        }
    }
    fn unschedule(&mut self, slot: usize, due: Option<u64>) {
        if let Some(due) = due {
            let offset = self.pages[slot / PAGE][slot % PAGE].expiry_offset;
            let bucket = self.expiry.get_mut(&due).expect("indexed social expiry");
            debug_assert_eq!(bucket[offset], slot);
            bucket.swap_remove(offset);
            if let Some(&moved) = bucket.get(offset) {
                self.pages[moved / PAGE][moved % PAGE].expiry_offset = offset;
            }
            if bucket.is_empty() {
                self.expiry.remove(&due);
            }
        }
    }
    fn replace(&mut self, slot: usize, row: Relationship, retention: Option<u64>) {
        self.unschedule(slot, expiry(self.get(slot), retention));
        self.schedule(slot, expiry(&row, retention));
        self.pages[slot / PAGE][slot % PAGE].row = Some(row);
    }
    fn insert(&mut self, row: Relationship, retention: Option<u64>) {
        let slot = if let Some(slot) = self.free.pop() {
            slot
        } else {
            if self.pages.last().is_none_or(|p| p.len() == PAGE) {
                self.pages.push(Vec::with_capacity(PAGE));
            }
            let slot = (self.pages.len() - 1) * PAGE + self.pages.last().unwrap().len();
            self.pages.last_mut().unwrap().push(Entry::default());
            slot
        };
        self.index.insert(key(&row), slot);
        self.schedule(slot, expiry(&row, retention));
        self.pages[slot / PAGE][slot % PAGE].row = Some(row);
    }
    fn remove(&mut self, slot: usize, retention: Option<u64>) {
        self.unschedule(slot, expiry(self.get(slot), retention));
        let row = self.pages[slot / PAGE][slot % PAGE].row.take().unwrap();
        self.index.remove(&key(&row));
        self.free.push(slot);
    }
}

#[derive(Default)]
struct Patch {
    expired: Vec<usize>,
    updates: Vec<(usize, Relationship)>,
    added: Vec<Relationship>,
}
pub struct Draft {
    patches: Vec<Patch>,
    tick: u64,
    epoch: u64,
    instance: u64,
}
pub struct Runtime {
    shards: Vec<Shard>,
    rules: Rules,
    authored: Option<Vec<Key>>,
    tick: u64,
    epoch: u64,
    instance: u64,
}
impl Runtime {
    /// Caller validates participant identities and relationship values on import.
    pub fn compile_validated(
        rows: &[Relationship],
        rules: Rules,
        tick: u64,
    ) -> Result<Self, &'static str> {
        if rules.interval == 0 || rules.familiarity_gain == 0 || rules.familiarity_max == 0 {
            return Err("invalid social rules");
        }
        let mut shards: Vec<_> = (0..SHARDS).map(|_| Shard::default()).collect();
        for row in rows {
            let s = &mut shards[shard(key(row))];
            if s.index.contains_key(&key(row)) {
                return Err("duplicate social relationship");
            }
            s.insert(row.clone(), rules.relationship_retention);
        }
        Ok(Self {
            shards,
            rules,
            authored: Some(rows.iter().map(key).collect()),
            tick,
            epoch: 0,
            instance: INSTANCE.fetch_add(1, Ordering::Relaxed),
        })
    }
    pub fn get(&self, a: u64, b: u64) -> Option<&Relationship> {
        let s = &self.shards[shard((a, b))];
        s.index.get(&(a, b)).map(|&slot| s.get(slot))
    }
    pub fn export(&self) -> Vec<Relationship> {
        if let Some(order) = &self.authored {
            return order
                .par_iter()
                .filter_map(|&(a, b)| self.get(a, b).cloned())
                .collect();
        }
        let mut rows: Vec<_> = self
            .shards
            .par_iter()
            .flat_map_iter(|s| s.pages.iter().flatten().filter_map(|r| r.row.clone()))
            .collect();
        rows.par_sort_unstable_by_key(key);
        rows
    }
    /// Encounters are unique canonical pairs from the validated social selection phase.
    pub fn prepare_validated(
        &mut self,
        events: &[Encounter],
        tick: u64,
    ) -> Result<Draft, &'static str> {
        if self.tick.checked_add(1) != Some(tick) {
            return Err("social phase must advance one tick");
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("social generation overflow")?;
        let chunks: Vec<_> = events
            .par_chunks(4096)
            .map(|chunk| {
                let mut buckets: [Vec<&Encounter>; SHARDS] = std::array::from_fn(|_| Vec::new());
                for event in chunk {
                    buckets[shard((event.a, event.b))].push(event);
                }
                buckets
            })
            .collect();
        let patches = self
            .shards
            .par_iter()
            .enumerate()
            .map(|(i, s)| {
                let mut expired: Vec<_> = s
                    .expiry
                    .range(..=tick)
                    .flat_map(|(_, slots)| slots.iter().copied())
                    .collect();
                expired.sort_unstable();
                let mut patch = Patch {
                    expired,
                    ..Patch::default()
                };
                for event in chunks.iter().flat_map(|chunk| chunk[i].iter()) {
                    if let Some(&slot) = s.index.get(&(event.a, event.b))
                        && patch.expired.binary_search(&slot).is_err()
                    {
                        let old = s.get(slot);
                        let mut next = old.clone();
                        next.encounters = next
                            .encounters
                            .checked_add(1)
                            .ok_or("social encounter counter overflow")?;
                        next.last_met = tick;
                        next.familiarity = next
                            .familiarity
                            .saturating_add(self.rules.familiarity_gain)
                            .min(self.rules.familiarity_max);
                        patch.updates.push((slot, next));
                    } else {
                        patch.added.push(Relationship {
                            a: event.a,
                            b: event.b,
                            last_met: tick,
                            encounters: 1,
                            familiarity: self
                                .rules
                                .familiarity_gain
                                .min(self.rules.familiarity_max),
                        });
                    }
                }
                Ok(patch)
            })
            .collect::<Result<_, &'static str>>()?;
        Ok(Draft {
            patches,
            tick,
            epoch: self.epoch,
            instance: self.instance,
        })
    }
    pub fn check(&self, draft: &Draft) -> Result<(), &'static str> {
        if draft.instance != self.instance
            || draft.epoch != self.epoch
            || self.tick.checked_add(1) != Some(draft.tick)
        {
            return Err("stale social draft");
        }
        Ok(())
    }
    pub fn commit(&mut self, draft: Draft) -> Result<(), &'static str> {
        self.check(&draft)?;
        let retention = self.rules.relationship_retention;
        self.shards
            .par_iter_mut()
            .zip(draft.patches)
            .for_each(|(s, patch)| {
                for slot in patch.expired {
                    s.remove(slot, retention);
                }
                for (slot, row) in patch.updates {
                    s.replace(slot, row, retention);
                }
                for row in patch.added {
                    s.insert(row, retention);
                }
            });
        self.tick = draft.tick;
        if self.tick.is_multiple_of(self.rules.interval) {
            self.authored = None;
        }
        Ok(())
    }
    pub fn live_len(&self) -> usize {
        self.shards.iter().map(|s| s.index.len()).sum()
    }
    pub fn allocated_slots(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.pages.iter().map(Vec::len).sum::<usize>())
            .sum()
    }
    pub fn expiry_len(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.expiry.values().map(Vec::len).sum::<usize>())
            .sum()
    }
}

/// Staged receiver-owned contact selection and sparse relationship accounting.
pub struct Selected {
    pub events: Vec<Encounter>,
    pub needs: Option<game_parallel_social::Needs>,
    pub edges: Draft,
}
impl Selected {
    /// People who spent this tick on the selected encounters.
    pub fn actors(&self) -> Vec<u64> {
        self.events
            .par_iter()
            .flat_map_iter(|event| [event.a, event.b])
            .collect()
    }
}
impl Runtime {
    pub fn prepare_indexed(
        &mut self,
        social: &game_parallel_social::Social,
        people: &[(u64, Option<u64>, u32, u32)],
        busy: &[u64],
        seed: u64,
        tick: u64,
    ) -> Result<Selected, &'static str> {
        self.prepare_indexed_with(social, people, busy, seed, tick, |_, _| true)
    }
    pub fn prepare_indexed_with(
        &mut self,
        social: &game_parallel_social::Social,
        people: &[(u64, Option<u64>, u32, u32)],
        busy: &[u64],
        seed: u64,
        tick: u64,
        allowed: impl Fn(u64, u32) -> bool + Sync,
    ) -> Result<Selected, &'static str> {
        self.prepare_indexed_with_extra(social, people, busy, (seed, tick), &[], allowed)
    }
    pub fn prepare_indexed_with_extra(
        &mut self,
        social: &game_parallel_social::Social,
        people: &[(u64, Option<u64>, u32, u32)],
        busy: &[u64],
        time: (u64, u64),
        forced: &[Encounter],
        allowed: impl Fn(u64, u32) -> bool + Sync,
    ) -> Result<Selected, &'static str> {
        let (seed, tick) = time;
        let mut busy = busy.to_vec();
        busy.par_sort_unstable();
        let ordered = !people.par_windows(2).any(|pair| pair[0].0 > pair[1].0);
        let contacts: Vec<_> = if ordered {
            people
                .par_chunks(4096)
                .flat_map_iter(|chunk| {
                    let mut cursor = busy.partition_point(|&person| person < chunk[0].0);
                    let busy = &busy;
                    chunk.iter().map(move |&(person, region, health, hunger)| {
                        while cursor < busy.len() && busy[cursor] < person {
                            cursor += 1;
                        }
                        game_parallel_social::Contact {
                            person,
                            region,
                            health,
                            hunger,
                            busy: busy.get(cursor) == Some(&person),
                        }
                    })
                })
                .collect()
        } else {
            people
                .par_iter()
                .map(
                    |&(person, region, health, hunger)| game_parallel_social::Contact {
                        person,
                        region,
                        health,
                        hunger,
                        busy: busy.binary_search(&person).is_ok(),
                    },
                )
                .collect()
        };
        let mut selection = social.select_validated(&contacts, seed, tick, allowed)?;
        if !forced.is_empty() {
            selection.events.extend_from_slice(forced);
            selection
                .events
                .par_sort_unstable_by_key(|event| (event.a, event.b));
            selection.needs = social
                .needs
                .as_ref()
                .map(|needs| needs.advance(&social.participants, &contacts, &selection.events));
        }
        let edges = self.prepare_validated(&selection.events, tick)?;
        Ok(Selected {
            events: selection.events,
            needs: selection.needs,
            edges,
        })
    }
}

#[cfg(test)]
#[path = "../tests/support/differential.rs"]
mod differential;

//! One indexed expiration deadline per nonempty perishable container.
use game_lot_model::{Container, Place, Spoilage, State};
use game_lot_planning::{Draft, TransactionTime};
use rayon::prelude::*;
use std::collections::BTreeSet;

const SHARDS: usize = 64;
const PARALLEL_CHANGES: usize = 256;
type Deadline = (u64, Place);

fn shard(place: Place) -> usize {
    let key = match place {
        Place::Person(id) => id,
        Place::Stock { region, good } => region.rotate_left(17) ^ u64::from(good),
        Place::Inventory(id) => id ^ 0x19,
        Place::Site(id) => id ^ 0x31,
        Place::JobInput(id) => id ^ 0x27,
        Place::JobOutput(id) => id ^ 0x41,
    };
    let mix = key ^ (key >> 6) ^ (key >> 19);
    (mix as usize) & (SHARDS - 1)
}

pub(super) struct ExpiryIndex {
    shards: Vec<BTreeSet<Deadline>>,
    next: Option<u64>,
}

impl ExpiryIndex {
    pub fn new(containers: &[Container]) -> Self {
        let mut rows: Vec<_> = containers
            .par_iter()
            .filter_map(|row| {
                row.next_expiry
                    .map(|at| (shard(row.place), (at, row.place)))
            })
            .collect();
        rows.par_sort_unstable();
        let shards = (0..SHARDS)
            .into_par_iter()
            .map(|index| {
                let first = rows.partition_point(|row| row.0 < index);
                let last = rows.partition_point(|row| row.0 <= index);
                rows[first..last].iter().map(|row| row.1).collect()
            })
            .collect();
        let mut index = Self { shards, next: None };
        index.refresh();
        index
    }

    fn refresh(&mut self) {
        self.next = self
            .shards
            .iter()
            .filter_map(|rows| rows.first().map(|row| row.0))
            .min();
    }

    pub fn next(&self) -> Option<u64> {
        self.next
    }

    pub fn update(&mut self, before: &[Container], changes: &[(usize, Container)]) {
        let edits = |(index, after): &(usize, Container)| {
            let old = before[*index].next_expiry;
            let new = after.next_expiry;
            (old != new).then(|| (shard(after.place), after.place, old, new))
        };
        let publish = |rows: &mut BTreeSet<Deadline>, place, old, new| {
            if let Some(old) = old {
                rows.remove(&(old, place));
            }
            if let Some(new) = new {
                rows.insert((new, place));
            }
        };
        if changes.len() < PARALLEL_CHANGES {
            let mut refresh = false;
            for (slot, place, old, new) in changes.iter().filter_map(edits) {
                refresh |= old.is_some() && old == self.next;
                if let Some(new) = new {
                    self.next = Some(self.next.map_or(new, |current| current.min(new)));
                }
                publish(&mut self.shards[slot], place, old, new);
            }
            if refresh {
                self.refresh();
            }
            return;
        }

        let mut edits: Vec<_> = changes.par_iter().filter_map(edits).collect();
        if edits.is_empty() {
            return;
        }
        edits.par_sort_unstable_by_key(|row| row.0);
        self.shards
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, rows)| {
                let first = edits.partition_point(|row| row.0 < index);
                let last = edits.partition_point(|row| row.0 <= index);
                let edits = &edits[first..last];
                if edits.len() == rows.len()
                    && edits.iter().all(|row| row.2.is_some() && row.3.is_none())
                {
                    rows.clear();
                    return;
                }
                for &(_, place, old, new) in edits {
                    publish(rows, place, old, new);
                }
            });
        self.refresh();
    }

    pub fn prepare(
        &self,
        state: &State,
        instance: u64,
        time: TransactionTime,
    ) -> Result<Draft, String> {
        state
            .revision
            .checked_add(1)
            .ok_or("lot revision overflow")?;
        let mut draft = Draft {
            base_instance: instance,
            base_tick: state.tick,
            base_revision: state.revision,
            next_tick: time.next_tick,
            changes: Default::default(),
            outcomes: Default::default(),
            spoiled: Vec::new(),
            creations: Vec::new(),
            transactions: Vec::new(),
        };
        if self.next().is_none_or(|deadline| deadline > time.at) {
            return Ok(draft);
        }
        let due: Vec<_> = self
            .shards
            .par_iter()
            .flat_map_iter(|rows| {
                rows.iter()
                    .take_while(|&&(at, _)| at <= time.at)
                    .map(|&(_, place)| place)
            })
            .collect();
        let (indices, ordered): (Vec<_>, _) = if due.len() >= state.containers.len() / 8 {
            (
                state
                    .containers
                    .par_iter()
                    .enumerate()
                    .filter_map(|(index, row)| {
                        row.next_expiry
                            .is_some_and(|at| at <= time.at)
                            .then_some(index)
                    })
                    .collect(),
                true,
            )
        } else {
            (
                due.par_iter()
                    .map(|place| {
                        state
                            .containers
                            .binary_search_by_key(place, |row| row.place)
                            .expect("expiry index names existing custody")
                    })
                    .collect(),
                false,
            )
        };
        let mut replacements: Vec<_> = indices
            .into_par_iter()
            .map(|index| {
                let old = &state.containers[index];
                let place = old.place;
                let classify = |lot: &game_lot_model::Lot| {
                    if let Some(at) = lot.expires_at.filter(|&at| at <= time.at) {
                        rayon::iter::Either::Right(Spoilage {
                            place,
                            good: lot.good,
                            quantity: lot.quantity,
                            expires_at: at,
                            batch: lot.batch,
                        })
                    } else {
                        rayon::iter::Either::Left(lot.clone())
                    }
                };
                let (lots, receipts): (Vec<_>, Vec<_>) = if old.lots.len() >= 1024
                    && old
                        .lots
                        .first()
                        .zip(old.lots.last())
                        .is_some_and(|(first, last)| first.good == last.good)
                {
                    let due = old
                        .lots
                        .partition_point(|lot| lot.expires_at.is_some_and(|at| at <= time.at));
                    let receipts = old.lots[..due]
                        .par_iter()
                        .map(|lot| Spoilage {
                            place,
                            good: lot.good,
                            quantity: lot.quantity,
                            expires_at: lot.expires_at.expect("due lot"),
                            batch: lot.batch,
                        })
                        .collect();
                    (old.lots[due..].to_vec(), receipts)
                } else if old.lots.len() >= 1024 {
                    old.lots.par_iter().partition_map(classify)
                } else {
                    let mut receipts = Vec::new();
                    let mut lots = Vec::with_capacity(old.lots.len());
                    for lot in &old.lots {
                        match classify(lot) {
                            rayon::iter::Either::Left(lot) => lots.push(lot),
                            rayon::iter::Either::Right(receipt) => receipts.push(receipt),
                        }
                    }
                    (lots, receipts)
                };
                let updated = Container {
                    place,
                    capacity: old.capacity,
                    used: old.used
                        - receipts
                            .iter()
                            .map(|row| state.footprints.admitted(row.good, row.quantity))
                            .sum::<u64>(),
                    next_expiry: lots.iter().filter_map(|row| row.expires_at).min(),
                    lots,
                };
                ((index, updated), receipts)
            })
            .collect();
        if !ordered {
            replacements.par_sort_unstable_by_key(|row| row.0.0);
        }
        let (changes, mut spoiled): (Vec<_>, Vec<_>) = replacements.into_par_iter().unzip();
        draft.spoiled = if spoiled.len() == 1 {
            spoiled.pop().expect("one expiry holder")
        } else {
            spoiled.into_par_iter().flatten().collect()
        };
        draft.changes = changes.into();
        Ok(draft)
    }
}

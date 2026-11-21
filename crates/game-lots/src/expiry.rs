//! One indexed expiration deadline per nonempty perishable container.
use game_lot_model::{Container, Place, Spoilage, State};
use game_lot_planning::{Draft, TransactionTime};
use rayon::prelude::*;
use std::collections::BTreeSet;

pub(super) struct ExpiryIndex {
    due: BTreeSet<(u64, Place)>,
}

impl ExpiryIndex {
    pub fn new(containers: &[Container]) -> Self {
        let rows: Vec<_> = containers
            .par_iter()
            .filter_map(|row| row.next_expiry.map(|at| (at, row.place)))
            .collect();
        Self {
            due: rows.into_iter().collect(),
        }
    }

    pub fn next(&self) -> Option<u64> {
        self.due.first().map(|&(time, _)| time)
    }

    pub fn update(&mut self, before: &[Container], changes: &[(usize, Container)]) {
        for (index, after) in changes {
            let before = &before[*index];
            if before.next_expiry == after.next_expiry {
                continue;
            }
            if let Some(old) = before.next_expiry {
                self.due.remove(&(old, before.place));
            }
            if let Some(new) = after.next_expiry {
                self.due.insert((new, after.place));
            }
        }
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
            .due
            .iter()
            .take_while(|&&(at, _)| at <= time.at)
            .map(|&(_, place)| place)
            .collect();
        let mut replacements: Vec<_> = due
            .par_iter()
            .map(|place| {
                let index = state
                    .containers
                    .binary_search_by_key(place, |row| row.place)
                    .expect("expiry index names existing custody");
                let old = &state.containers[index];
                let mut receipts = Vec::new();
                let mut lots = Vec::with_capacity(old.lots.len());
                for lot in &old.lots {
                    if let Some(at) = lot.expires_at.filter(|&at| at <= time.at) {
                        receipts.push(Spoilage {
                            place: *place,
                            good: lot.good,
                            quantity: lot.quantity,
                            expires_at: at,
                            batch: lot.batch,
                        });
                    } else {
                        lots.push(lot.clone());
                    }
                }
                let updated = Container {
                    place: *place,
                    capacity: old.capacity,
                    used: old.used - receipts.iter().map(|row| row.quantity).sum::<u64>(),
                    next_expiry: lots.iter().filter_map(|row| row.expires_at).min(),
                    lots,
                };
                ((index, updated), receipts)
            })
            .collect();
        replacements.par_sort_unstable_by_key(|row| row.0.0);
        let mut changes = Vec::with_capacity(replacements.len());
        for (change, mut receipts) in replacements {
            changes.push(change);
            draft.spoiled.append(&mut receipts);
        }
        draft.changes = changes.into();
        Ok(draft)
    }
}

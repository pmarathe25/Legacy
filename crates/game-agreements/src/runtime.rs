//! Elapsed deadline and party indexes rebuilt when agreement statuses change.
use crate::{Agreements, Contract, Event, EventKind, Status};
use rayon::prelude::*;
use std::collections::HashMap;
type PartiesAndGood = (u64, u64, u32);
const BLOCK: usize = 4096;
pub struct Runtime {
    pending: Vec<(u64, usize)>,
    cursor: usize,
    last_due_by_party: HashMap<PartiesAndGood, u64>,
}
impl Runtime {
    pub fn compile(state: &Agreements) -> Self {
        let mut pending: Vec<_> = state
            .contracts
            .par_iter()
            .enumerate()
            .filter_map(|(index, contract)| {
                matches!(contract.status, Status::Active | Status::Proposed)
                    .then_some((contract.due, index))
            })
            .collect();
        pending.par_sort_unstable();
        let last_due_by_party = if state.time_unit_millis.is_some() {
            // Keep each pair live through its latest outstanding promise.
            pending
                .par_iter()
                .fold(
                    HashMap::new,
                    |mut entries: HashMap<PartiesAndGood, u64>, &(due, slot)| {
                        let contract = &state.contracts[slot];
                        let key = (contract.debtor, contract.creditor, contract.good);
                        let latest = entries.entry(key).or_insert(due);
                        *latest = (*latest).max(due);
                        entries
                    },
                )
                .reduce(HashMap::new, |mut left, mut right| {
                    if left.len() < right.len() {
                        std::mem::swap(&mut left, &mut right);
                    }
                    for (key, due) in right {
                        let latest = left.entry(key).or_insert(due);
                        *latest = (*latest).max(due);
                    }
                    left
                })
        } else {
            HashMap::new()
        };
        Self {
            pending,
            cursor: 0,
            last_due_by_party,
        }
    }

    pub fn has_open_contracts(&self) -> bool {
        self.cursor < self.pending.len()
    }

    /// Pair this index with the agreement state it was compiled from and updated with.
    pub fn open_contracts<'a>(
        &'a self,
        state: &'a Agreements,
    ) -> impl ParallelIterator<Item = &'a Contract> {
        self.pending[self.cursor..]
            .par_iter()
            .map(|&(_, slot)| &state.contracts[slot])
    }
    pub fn has_live_promise(&self, parties_and_good: PartiesAndGood, now: u64) -> bool {
        self.last_due_by_party
            .get(&parties_and_good)
            .is_some_and(|due| *due >= now)
    }

    /// The advertised due instant is still valid for delivery.
    pub fn next_expiration_after(&self, now: u128) -> Option<u64> {
        let due = self.pending.get(self.cursor)?.0.checked_add(1)?;
        u64::try_from(u128::from(due).saturating_sub(now)).ok()
    }

    pub fn advance(&mut self, state: &mut Agreements, tick: u64) {
        self.advance_at(
            state,
            crate::Instant {
                now: tick,
                evidence_tick: tick,
            },
        );
    }

    /// Publish only the contracts that have become due. Historical rows are not scanned.
    pub fn advance_at(&mut self, state: &mut Agreements, instant: crate::Instant) {
        let end = self.pending.partition_point(|entry| entry.0 < instant.now);
        if end <= self.cursor {
            state.events.clear();
            return;
        }
        let mut patches: Vec<_> = self.pending[self.cursor..end]
            .par_iter()
            .filter_map(|&(_, slot)| {
                let contract = &state.contracts[slot];
                let (status, kind) = match contract.status {
                    Status::Active => (Status::Defaulted, EventKind::Defaulted),
                    Status::Proposed => (Status::Withdrawn, EventKind::Withdrawn),
                    _ => return None,
                };
                let event = Event {
                    tick: instant.evidence_tick,
                    contract: contract.id,
                    actor: contract.debtor,
                    kind,
                    quantity: contract.quantity.saturating_sub(contract.delivered),
                };
                Some((slot, status, event))
            })
            .collect();
        patches.par_sort_unstable_by_key(|row| row.0);
        if patches.len() <= 64 {
            for (slot, status, _) in &patches {
                state.contracts[*slot].status = *status;
            }
        } else {
            let opening = patches.first().expect("nonempty patches").0;
            let closing = patches.last().expect("nonempty patches").0;
            state.contracts[opening..=closing]
                .par_chunks_mut(BLOCK)
                .enumerate()
                .for_each(|(block, chunk)| {
                    let base = opening + block * BLOCK;
                    let start = patches.partition_point(|row| row.0 < base);
                    let end = patches.partition_point(|row| row.0 < base + chunk.len());
                    for (slot, status, _) in &patches[start..end] {
                        chunk[*slot - base].status = *status;
                    }
                });
        }
        state.events = patches.into_par_iter().map(|(_, _, event)| event).collect();
        state
            .events
            .par_sort_unstable_by_key(|event| event.contract);
        self.cursor = end;
    }
}

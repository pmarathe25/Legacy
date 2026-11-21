//! Due-date index for the fused host. Commands rebuild it at compilation boundaries.
use crate::{Agreements, Event, EventKind, Status};
use rayon::prelude::*;
const BLOCK: usize = 4096;
pub struct Runtime {
    pending: Vec<(u64, usize)>,
    cursor: usize,
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
        Self { pending, cursor: 0 }
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

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
    /// Called after all fallible stages; no future deadline is scanned per tick.
    pub fn advance(&mut self, state: &mut Agreements, tick: u64) {
        let end = self.pending.partition_point(|entry| entry.0 < tick);
        if end <= self.cursor {
            state.events.clear();
            return;
        }
        let mut indices: Vec<_> = self.pending[self.cursor..end]
            .par_iter()
            .map(|entry| entry.1)
            .collect();
        indices.par_sort_unstable();
        let mut events: Vec<_> = state
            .contracts
            .par_chunks_mut(BLOCK)
            .enumerate()
            .flat_map_iter(|(block, chunk)| {
                let base = block * BLOCK;
                let start = indices.partition_point(|&index| index < base);
                let end = indices.partition_point(|&index| index < base + chunk.len());
                let mut events = Vec::with_capacity(end - start);
                for &index in &indices[start..end] {
                    let contract = &mut chunk[index - base];
                    if !matches!(contract.status, Status::Active | Status::Proposed) {
                        continue;
                    }
                    let active = contract.status == Status::Active;
                    contract.status = if active {
                        Status::Defaulted
                    } else {
                        Status::Withdrawn
                    };
                    events.push(Event {
                        tick,
                        contract: contract.id,
                        actor: contract.debtor,
                        kind: if active {
                            EventKind::Defaulted
                        } else {
                            EventKind::Withdrawn
                        },
                        quantity: contract.quantity.saturating_sub(contract.delivered),
                    });
                }
                events
            })
            .collect();
        events.par_sort_unstable_by_key(|event| event.contract);
        state.events = events;
        self.cursor = end;
    }
}

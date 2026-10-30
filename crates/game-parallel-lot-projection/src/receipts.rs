//! Validate physical evidence against the exact selected action.
use game_parallel_lot_runtime::{Events, ExternalReceipt};
use game_parallel_lots::Transaction;
use rayon::prelude::*;
pub struct Index<'a> {
    rows: Vec<&'a ExternalReceipt>,
}
impl<'a> Index<'a> {
    pub fn new(events: &'a Events) -> Result<Self, String> {
        let mut rows: Vec<_> = events.physical_movements.par_iter().collect();
        rows.par_sort_unstable_by_key(|row| (row.action, row.request.id));
        if rows.par_windows(2).any(|pair| {
            (pair[0].action, pair[0].request.id) == (pair[1].action, pair[1].request.id)
        }) {
            return Err("duplicate physical receipt".into());
        }
        Ok(Self { rows })
    }
    pub fn verify(&self, action: &Transaction, accepted: bool) -> bool {
        let first = self.rows.partition_point(|row| row.action < action.id);
        let last = self.rows.partition_point(|row| row.action <= action.id);
        let rows = &self.rows[first..last];
        action.creations.is_empty()
            && rows.len() == action.movements.len()
            && action.movements.iter().all(|request| {
                rows.binary_search_by_key(&request.id, |row| row.request.id)
                    .ok()
                    .is_some_and(|index| {
                        let row = rows[index];
                        row.request == *request
                            && row.movement.id == request.id
                            && row.movement.accepted == accepted
                            && if accepted {
                                row.movement.lots.iter().all(|lot| lot.good == request.good)
                                    && row
                                        .movement
                                        .lots
                                        .iter()
                                        .map(|lot| u128::from(lot.quantity))
                                        .sum::<u128>()
                                        == u128::from(request.quantity)
                            } else {
                                row.movement.lots.is_empty()
                            }
                    })
            })
    }
}

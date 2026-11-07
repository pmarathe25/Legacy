//! External physical receipts borrow dated lots from their owning batch.
use game_parallel_lots::{Lot, Movement, Outcome, OutcomeBatch, OutcomeHeader, OutcomeRef};
use rayon::prelude::*;

#[derive(Debug, PartialEq, Eq)]
pub struct ExternalReceipt {
    pub action: u64,
    pub request: Movement,
    pub movement: Outcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalReceiptRef<'a> {
    pub action: u64,
    pub request: Movement,
    pub movement: OutcomeRef<'a>,
}
impl ExternalReceiptRef<'_> {
    pub fn to_owned(self) -> ExternalReceipt {
        ExternalReceipt {
            action: self.action,
            request: self.request,
            movement: self.movement.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExternalReceiptHeader {
    pub action: u64,
    pub request: Movement,
    pub movement: OutcomeHeader,
}

#[derive(Debug, Default)]
pub struct ExternalReceiptBatch {
    headers: Vec<ExternalReceiptHeader>,
    arenas: Vec<Vec<Lot>>,
}
impl ExternalReceiptBatch {
    pub fn from_parts(headers: Vec<ExternalReceiptHeader>, arenas: Vec<Vec<Lot>>) -> Self {
        assert!(headers.par_iter().all(|row| {
            let h = row.movement;
            arenas
                .get(h.arena)
                .is_some_and(|arena| h.begin <= h.end && h.end <= arena.len())
        }));
        Self { headers, arenas }
    }
    fn view(&self, row: &ExternalReceiptHeader) -> ExternalReceiptRef<'_> {
        let h = row.movement;
        ExternalReceiptRef {
            action: row.action,
            request: row.request,
            movement: OutcomeRef {
                id: h.id,
                accepted: h.accepted,
                lots: &self.arenas[h.arena][h.begin..h.end],
            },
        }
    }
    pub fn len(&self) -> usize {
        self.headers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }
    pub fn iter(&self) -> ReceiptIter<'_> {
        ReceiptIter {
            batch: self,
            rows: self.headers.iter(),
        }
    }
    pub fn par_iter(&self) -> impl IndexedParallelIterator<Item = ExternalReceiptRef<'_>> + '_ {
        self.headers.par_iter().map(|row| self.view(row))
    }
    pub fn into_owned(self) -> Vec<ExternalReceipt> {
        self.par_iter().map(ExternalReceiptRef::to_owned).collect()
    }
    pub fn sort_by_action(&mut self) {
        if self
            .headers
            .par_windows(2)
            .any(|pair| pair[0].action > pair[1].action)
        {
            self.headers.par_sort_unstable_by_key(|row| row.action);
        }
    }
}
impl PartialEq for ExternalReceiptBatch {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}
impl Eq for ExternalReceiptBatch {}
impl From<Vec<ExternalReceipt>> for ExternalReceiptBatch {
    fn from(receipts: Vec<ExternalReceipt>) -> Self {
        let (keys, outcomes): (Vec<_>, Vec<_>) = receipts
            .into_par_iter()
            .map(|row| ((row.action, row.request), row.movement))
            .unzip();
        let (headers, arenas) = OutcomeBatch::from_outcomes(outcomes).into_parts();
        let headers = keys
            .into_par_iter()
            .zip(headers)
            .map(|((action, request), movement)| ExternalReceiptHeader {
                action,
                request,
                movement,
            })
            .collect();
        Self::from_parts(headers, arenas)
    }
}
pub struct ReceiptIter<'a> {
    batch: &'a ExternalReceiptBatch,
    rows: std::slice::Iter<'a, ExternalReceiptHeader>,
}
impl<'a> Iterator for ReceiptIter<'a> {
    type Item = ExternalReceiptRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.rows.next().map(|row| self.batch.view(row))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rows.size_hint()
    }
}
impl ExactSizeIterator for ReceiptIter<'_> {}
impl<'a> IntoIterator for &'a ExternalReceiptBatch {
    type Item = ExternalReceiptRef<'a>;
    type IntoIter = ReceiptIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

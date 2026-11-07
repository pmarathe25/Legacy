//! Transient physical receipts with batch-owned dated cohorts.
use crate::{Lot, LotList, Outcome};
use rayon::prelude::*;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutcomeHeader {
    pub id: u64,
    pub accepted: bool,
    pub arena: usize,
    pub begin: usize,
    pub end: usize,
}

/// One independently produced chunk. Ranges refer to this chunk's lot arena.
pub struct OutcomeChunk {
    pub headers: Vec<(u64, bool, Range<usize>)>,
    pub lots: Vec<Lot>,
}

/// A receipt view cannot outlive the batch that owns its lots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutcomeRef<'a> {
    pub id: u64,
    pub accepted: bool,
    pub lots: &'a [Lot],
}
impl OutcomeRef<'_> {
    pub fn to_owned(self) -> Outcome {
        Outcome {
            id: self.id,
            accepted: self.accepted,
            lots: self.lots.to_vec().into(),
        }
    }
}

#[derive(Default, Debug)]
pub struct OutcomeBatch {
    headers: Vec<OutcomeHeader>,
    arenas: Vec<Vec<Lot>>,
}
impl OutcomeBatch {
    pub fn from_chunks(chunks: Vec<OutcomeChunk>) -> Self {
        let (parts, arenas): (Vec<_>, Vec<_>) = chunks
            .into_par_iter()
            .enumerate()
            .map(|(arena, chunk)| {
                let length = chunk.lots.len();
                let headers: Vec<_> = chunk
                    .headers
                    .into_iter()
                    .map(|(id, accepted, range)| {
                        assert!(range.start <= range.end && range.end <= length);
                        OutcomeHeader {
                            id,
                            accepted,
                            arena,
                            begin: range.start,
                            end: range.end,
                        }
                    })
                    .collect();
                (headers, chunk.lots)
            })
            .unzip();
        let headers = parts.into_par_iter().flatten().collect();
        Self { headers, arenas }
    }
    pub fn from_outcomes(outcomes: Vec<Outcome>) -> Self {
        let (headers, arenas): (Vec<_>, Vec<_>) = outcomes
            .into_par_iter()
            .enumerate()
            .map(|(arena, outcome)| {
                let lots = outcome.lots.into_vec();
                let header = OutcomeHeader {
                    id: outcome.id,
                    accepted: outcome.accepted,
                    arena,
                    begin: 0,
                    end: lots.len(),
                };
                (header, lots)
            })
            .unzip();
        Self { headers, arenas }
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = OutcomeRef<'_>> + '_ {
        self.headers.iter().map(|header| self.view(header))
    }
    pub fn par_iter(&self) -> impl IndexedParallelIterator<Item = OutcomeRef<'_>> + '_ {
        self.headers.par_iter().map(|header| self.view(header))
    }
    fn view(&self, header: &OutcomeHeader) -> OutcomeRef<'_> {
        OutcomeRef {
            id: header.id,
            accepted: header.accepted,
            lots: &self.arenas[header.arena][header.begin..header.end],
        }
    }
    pub fn into_owned(self) -> Vec<Outcome> {
        let arenas: Vec<Option<Arc<[Lot]>>> = self
            .arenas
            .into_par_iter()
            .map(|lots| (!lots.is_empty()).then(|| Arc::from(lots)))
            .collect();
        self.headers
            .into_par_iter()
            .map(|header| Outcome {
                id: header.id,
                accepted: header.accepted,
                lots: if header.begin == header.end {
                    LotList::default()
                } else {
                    LotList::shared(
                        arenas[header.arena]
                            .as_ref()
                            .expect("nonempty receipt arena")
                            .clone(),
                        header.begin..header.end,
                    )
                },
            })
            .collect()
    }
    pub fn append(&mut self, mut other: Self) {
        let offset = self.arenas.len();
        other
            .headers
            .par_iter_mut()
            .for_each(|header| header.arena += offset);
        self.headers.par_extend(other.headers);
        self.arenas.append(&mut other.arenas);
    }
    pub fn sort_by_id(&mut self) {
        if self
            .headers
            .par_windows(2)
            .any(|pair| pair[0].id > pair[1].id)
        {
            self.headers.par_sort_unstable_by_key(|header| header.id);
        }
    }
    pub fn into_parts(self) -> (Vec<OutcomeHeader>, Vec<Vec<Lot>>) {
        (self.headers, self.arenas)
    }
}

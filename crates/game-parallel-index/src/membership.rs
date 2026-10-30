//! Sorted or dense membership lookup without shared per-vote mutation.
use rayon::prelude::*;
use std::borrow::Cow;
pub struct Membership<'a> {
    rows: Cow<'a, [u64]>,
    dense: Option<u64>,
}
impl<'a> Membership<'a> {
    pub fn new(rows: &'a [u64]) -> Self {
        let rows = if rows.par_windows(2).any(|pair| pair[0] > pair[1]) {
            let mut copy = rows.to_vec();
            copy.par_sort_unstable();
            Cow::Owned(copy)
        } else {
            Cow::Borrowed(rows)
        };
        let dense = rows.first().and_then(|&first| {
            rows.last().and_then(|&last| {
                (last.checked_sub(first) == Some((rows.len() - 1) as u64)).then_some(first)
            })
        });
        Self { rows, dense }
    }
    pub fn contains(&self, person: u64) -> bool {
        self.dense
            .is_some_and(|first| person >= first && person - first < self.rows.len() as u64)
            || (self.dense.is_none() && self.rows.binary_search(&person).is_ok())
    }
    pub fn duplicates(&self) -> bool {
        self.rows.par_windows(2).any(|pair| pair[0] == pair[1])
    }
}

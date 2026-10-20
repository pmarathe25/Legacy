//! Numeric columns are filled directly; no intermediate candidate copy or heap per column.
use super::scoring;
use crate::{Candidate, Decision};
pub struct Batch {
    data: Vec<i64>,
    len: usize,
    dirty: bool,
}
impl Batch {
    pub fn new(rows: impl ExactSizeIterator<Item = Candidate>) -> Self {
        let n = rows.len();
        let mut data = vec![0; n.checked_mul(7).expect("numeric batch too large")];
        if n > 0 {
            let mut columns = data.chunks_exact_mut(n);
            let [needs, goals, values, efforts, risks, ids] =
                std::array::from_fn(|_| columns.next().unwrap());
            for (i, c) in rows.enumerate() {
                needs[i] = c.need;
                goals[i] = c.goal;
                values[i] = c.value;
                efforts[i] = c.effort;
                risks[i] = c.risk;
                ids[i] = c.id as i64;
            }
        }
        Self {
            data,
            len: n,
            dirty: true,
        }
    }
    #[inline]
    pub fn id(&self, i: usize) -> u64 {
        self.data[5 * self.len..6 * self.len][i] as u64
    }
    pub fn penalize(&mut self, i: usize, cost: i64) {
        let risk = &mut self.data[4 * self.len..5 * self.len][i];
        *risk = risk.saturating_add(cost);
        self.dirty = true;
    }
    /// Reconstruct only a selected motive; nonnumeric flags remain caller-owned.
    pub fn apply(&self, i: usize, candidate: &mut Candidate) {
        candidate.need = self.data[i];
        candidate.goal = self.data[self.len + i];
        candidate.value = self.data[2 * self.len + i];
        candidate.effort = self.data[3 * self.len + i];
        candidate.risk = self.data[4 * self.len + i];
        candidate.id = self.id(i);
    }
    fn compute(&mut self, portable: bool) {
        let (input, output) = self.data.split_at_mut(6 * self.len);
        let fields = std::array::from_fn(|i| &input[i * self.len..(i + 1) * self.len]);
        if portable {
            scoring::portable(fields, output);
        } else {
            scoring::dispatch(fields, output);
        }
        self.dirty = false;
    }
    /// Explicit portable evaluation supports reproducible backend comparisons.
    pub fn score_portable(&mut self) {
        self.compute(true);
    }
    pub fn choose(
        &mut self,
        order: &[usize],
        current: Option<u64>,
        margin: i64,
    ) -> Option<Decision> {
        if self.dirty {
            self.compute(false);
        }
        let ids = &self.data[5 * self.len..6 * self.len];
        let scores = &self.data[6 * self.len..];
        let best = *order
            .iter()
            .max_by_key(|&&i| (scores[i], std::cmp::Reverse(ids[i] as u64)))?;
        let selected = order
            .iter()
            .copied()
            .find(|&i| {
                Some(ids[i] as u64) == current && scores[best] <= scores[i].saturating_add(margin)
            })
            .unwrap_or(best);
        Some(Decision {
            id: self.id(selected),
            score: scores[selected],
            reasons: vec![],
        })
    }
}

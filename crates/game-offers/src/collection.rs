//! Rank compact slots; move executable payloads only on replacement and final ordering.
use super::*;
use std::cmp::Reverse;
type Rank = (i64, Reverse<u64>);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Collection {
    pub person: PersonId,
    pub capture: bool,
    pub weights: Option<game_traits::Weights>,
    pub ongoing: Option<u64>,
    pub items: Vec<Proposal>,
    pub heap: Vec<(usize, Rank)>,
    pub identities: rustc_hash::FxHashMap<u64, usize>,
}
impl Collection {
    pub fn rejects(
        &self,
        key: &str,
        mut motive: Candidate,
        weights: Option<game_traits::Weights>,
    ) -> bool {
        let Some(&(slot, priority)) = self.heap.first() else {
            return false;
        };
        if self.capture || self.weights != weights || self.identities.contains_key(&identity(key)) {
            return false;
        }
        motive.id = identity(key);
        let worst = &self.items[slot];
        (false, rank(motive, weights), key)
            < (
                self.ongoing
                    .is_some_and(|id| worst.continuation == Some(id)),
                priority,
                worst.key.as_str(),
            )
    }
    pub fn insert(
        &mut self,
        proposal: Proposal,
        weights: Option<game_traits::Weights>,
    ) -> Result<()> {
        if self.capture {
            self.items.push(proposal);
            return Ok(());
        }
        let key_hash = identity(&proposal.key);
        require(
            !self.identities.contains_key(&key_hash)
                || self.items.iter().all(|p| p.key != proposal.key),
            "duplicate proposal key",
        )?;
        let score = if self.items.len() >= LIMIT {
            if self.heap.is_empty() || self.weights != weights {
                self.heap.clear();
                self.heap.extend(
                    self.items
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (i, rank(p.motive, weights))),
                );
                self.weights = weights;
                for i in (0..self.heap.len() / 2).rev() {
                    self.sift_down(i);
                }
            }
            rank(proposal.motive, weights)
        } else {
            (0, Reverse(0))
        };
        if self.items.len() == LIMIT {
            let (slot, priority) = self.heap[0];
            let worst = &self.items[slot];
            let continues =
                |p: &Proposal| self.ongoing.is_some_and(|id| p.continuation == Some(id));
            if (continues(&proposal), score, &proposal.key)
                < (continues(worst), priority, &worst.key)
            {
                return Ok(());
            }
        }
        *self.identities.entry(key_hash).or_default() += 1;
        if self.items.len() == LIMIT {
            let slot = self.heap[0].0;
            let removed = std::mem::replace(&mut self.items[slot], proposal);
            self.heap[0].1 = score;
            self.sift_down(0);
            let removed_hash = identity(&removed.key);
            let count = self.identities.get_mut(&removed_hash).unwrap();
            *count -= 1;
            if *count == 0 {
                self.identities.remove(&removed_hash);
            }
        } else {
            self.items.push(proposal);
        }
        Ok(())
    }
    fn less(&self, a: usize, b: usize) -> bool {
        let priority = |i: usize| {
            let (slot, rank) = self.heap[i];
            let p = &self.items[slot];
            (
                self.ongoing.is_some_and(|id| p.continuation == Some(id)),
                rank,
                &p.key,
            )
        };
        priority(a) < priority(b)
    }
    fn sift_down(&mut self, mut i: usize) {
        loop {
            let mut child = 2 * i + 1;
            if child >= self.heap.len() {
                break;
            }
            if child + 1 < self.heap.len() && self.less(child + 1, child) {
                child += 1;
            }
            if !self.less(child, i) {
                break;
            }
            self.heap.swap(i, child);
            i = child;
        }
    }
    pub fn finish(mut self) -> Vec<Proposal> {
        // Small collections avoid the auxiliary index allocations.
        if self.items.len() < 32 {
            self.items.sort_unstable_by(|a, b| a.key.cmp(&b.key));
        } else {
            let mut order: Vec<_> = (0..self.items.len()).collect();
            order.sort_unstable_by(|a, b| self.items[*a].key.cmp(&self.items[*b].key));
            let mut destinations = vec![0; order.len()];
            for (to, from) in order.into_iter().enumerate() {
                destinations[from] = to;
            }
            // At most N swaps of full payloads, instead of moving them throughout sorting.
            for i in 0..self.items.len() {
                while destinations[i] != i {
                    let to = destinations[i];
                    self.items.swap(i, to);
                    destinations.swap(i, to);
                }
            }
        }
        self.items
    }
}
fn rank(mut candidate: Candidate, weights: Option<game_traits::Weights>) -> Rank {
    candidate.reweight(|term| {
        weights
            .as_ref()
            .and_then(|w| w.get(term))
            .copied()
            .unwrap_or(100)
    });
    (candidate.score(), Reverse(candidate.id))
}

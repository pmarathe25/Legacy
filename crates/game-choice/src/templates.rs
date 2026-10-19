//! Bounded process-local reuse of immutable, prepared executable offers.
use super::*;
use std::collections::{BTreeMap, VecDeque};
pub struct Templates<K> {
    limit: usize,
    values: BTreeMap<K, Proposal>,
    order: VecDeque<K>,
}
impl<K: Ord + Clone> Templates<K> {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            values: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }
    /// The key must describe every executable payload field; motives remain caller-owned.
    pub fn get(&mut self, key: K, build: impl FnOnce() -> Result<Proposal>) -> Result<Proposal> {
        if let Some(value) = self.values.get(&key) {
            return Ok(value.clone());
        }
        let value = build()?;
        if self.values.len() >= self.limit {
            self.values.remove(&self.order.pop_front().unwrap());
        }
        self.order.push_back(key.clone());
        self.values.insert(key, value.clone());
        Ok(value)
    }
}

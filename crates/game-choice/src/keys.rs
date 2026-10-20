//! Numeric fingerprints avoid repeatedly chasing every offered string during deduplication.
use super::*;
pub struct Keys(rustc_hash::FxHashSet<u64>);
impl Keys {
    pub fn new(offers: &[Proposal]) -> Self {
        Self(offers.iter().map(|o| identity(&o.key)).collect())
    }
    /// Existing offers must be append-only. Fingerprints are an index, not equality:
    /// collisions still compare complete keys, including deliberately overridden motive IDs.
    pub fn insert(&mut self, key: &str, offers: &[Proposal]) -> bool {
        self.0.insert(identity(key)) || offers.iter().all(|o| o.key != key)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_keys_and_colliding_buckets_keep_full_string_semantics() {
        let mut offers = vec![Proposal::new("a", motive(0, 0, 0, 0, 0), Wait).unwrap()];
        offers[0].motive.id = 0;
        let mut keys = Keys::new(&offers);
        assert!(!keys.insert("a", &offers));
        keys.0.insert(identity("b")); // Exercise the collision path, without assuming collision-free hashes.
        assert!(keys.insert("b", &offers));
        offers.push(Proposal::new("b", motive(0, 0, 0, 0, 0), Wait).unwrap());
        assert!(!keys.insert("b", &offers));
        assert!(keys.insert("b:雪", &offers));
    }
}

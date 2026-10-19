//! Bounded weak sharing restores immutable genome payloads across saved plants and lots.
use crate::*;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Weak},
};
pub type Genome = std::collections::BTreeMap<GeneName, Alleles>;
const LIMIT: usize = 4096;
#[derive(Default)]
struct Pool {
    values: HashMap<u64, Weak<Genome>>,
    order: VecDeque<u64>,
}
impl Pool {
    fn intern(&mut self, hash: u64, genome: Genome) -> Arc<Genome> {
        if let Some(old) = self.values.get(&hash).and_then(Weak::upgrade)
            && *old == genome
        {
            return old;
        }
        if !self.values.contains_key(&hash) {
            if self.values.len() == LIMIT {
                self.values.remove(&self.order.pop_front().unwrap());
            }
            self.order.push_back(hash);
        }
        let genome = Arc::new(genome);
        self.values.insert(hash, Arc::downgrade(&genome));
        genome
    }
}
thread_local! {static GENOMES: RefCell<Pool> = RefCell::new(Pool::default());}
/// Values and identity metadata remain unchanged; expired entries retain no genome nodes.
pub fn deserialize_genome<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Arc<Genome>, D::Error> {
    let genome = Genome::deserialize(deserializer)?;
    let mut hash = DefaultHasher::new();
    for (key, alleles) in &genome {
        key.hash(&mut hash);
        alleles.0.hash(&mut hash);
    }
    GENOMES.with(|pool| Ok(pool.borrow_mut().intern(hash.finish(), genome)))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn genome(n: u16) -> Genome {
        [("cold".into(), Alleles([n, n]))].into()
    }
    #[test]
    fn sharing_is_bounded_weak_and_checks_complete_values_on_hash_collisions() {
        let mut pool = Pool::default();
        let first = pool.intern(1, genome(10));
        assert!(Arc::ptr_eq(&first, &pool.intern(1, genome(10))));
        let other = pool.intern(1, genome(20));
        assert_eq!(*other, genome(20));
        assert!(!Arc::ptr_eq(&first, &other));
        let weak = Arc::downgrade(&other);
        drop(other);
        assert!(weak.upgrade().is_none());
        for n in 0..LIMIT + 1 {
            pool.intern(n as u64, genome(n as u16));
        }
        assert!(pool.values.len() <= LIMIT && pool.order.len() <= LIMIT);
        assert_eq!(*first, genome(10));
    }
}

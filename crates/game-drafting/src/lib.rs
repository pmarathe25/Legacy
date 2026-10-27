//! Conservative dependencies for ordered speculative drafts, with no gameplay state.
use rayon::prelude::*;
use serde::Serialize;
pub mod arbitration;
use std::{
    any::TypeId,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
const SLOTS: usize = 256;
fn slot<T: 'static>() -> usize {
    let mut hash = rustc_hash::FxHasher::default();
    TypeId::of::<T>().hash(&mut hash);
    hash.finish() as usize % SLOTS
}
#[derive(Default)]
pub struct Reads([AtomicU64; 4]);
impl Reads {
    pub fn include(&self, other: &Self) {
        for (a, b) in self.0.iter().zip(&other.0) {
            a.fetch_or(b.load(Relaxed), Relaxed);
        }
    }
    pub fn record<T: 'static>(&self) {
        let i = slot::<T>();
        let bits = &self.0[i / 64];
        let bit = 1 << (i % 64);
        if bits.load(Relaxed) & bit == 0 {
            bits.fetch_or(bit, Relaxed);
        }
    }
}
pub struct Versions {
    clock: u64,
    changed: [u64; SLOTS],
}
impl Default for Versions {
    fn default() -> Self {
        Self {
            clock: 0,
            changed: [0; SLOTS],
        }
    }
}
impl Versions {
    pub fn write<T: 'static>(&mut self) {
        self.clock += 1;
        self.changed[slot::<T>()] = self.clock;
    }
    pub fn clock(&self) -> u64 {
        self.clock
    }
    pub fn valid(&self, reads: &Reads, at: u64) -> bool {
        reads.0.iter().enumerate().all(|(word, bits)| {
            let mut bits = bits.load(Relaxed);
            while bits != 0 {
                let i = bits.trailing_zeros() as usize;
                if self.changed[word * 64 + i] > at {
                    return false;
                }
                bits &= bits - 1;
            }
            true
        })
    }
}
/// Prepare every independent callback once, then visit all callbacks in order.
/// The caller validates dependencies and retries stale or effectful work.
pub fn ordered<C: Sync, D: Send, E>(
    context: &mut C,
    pool: &rayon::ThreadPool,
    independent: &[bool],
    draft: impl Fn(&C, usize) -> D + Sync + Send,
    mut apply: impl FnMut(&mut C, usize, Option<D>) -> Result<(), E>,
) -> Result<(), E> {
    let drafts: Vec<_> = pool.install(|| {
        independent
            .par_iter()
            .enumerate()
            .map(|(i, yes)| yes.then(|| draft(context, i)))
            .collect()
    });
    for (i, draft) in drafts.into_iter().enumerate() {
        apply(context, i, draft)?;
    }
    Ok(())
}
#[derive(Default)]
pub struct Counters {
    pub drafted: AtomicU64,
    pub accepted: AtomicU64,
    pub stale: AtomicU64,
    pub effectful: AtomicU64,
}
#[derive(Debug, Serialize)]
pub struct Statistics {
    pub drafted: u64,
    pub accepted: u64,
    pub stale: u64,
    pub effectful: u64,
}
impl Counters {
    pub fn retry(&self, blocked: bool) {
        let counter = if blocked {
            &self.effectful
        } else {
            &self.stale
        };
        counter.fetch_add(1, Relaxed);
    }
    pub fn snapshot(&self) -> Statistics {
        Statistics {
            drafted: self.drafted.load(Relaxed),
            accepted: self.accepted.load(Relaxed),
            stale: self.stale.load(Relaxed),
            effectful: self.effectful.load(Relaxed),
        }
    }
}

pub fn read<T: 'static>(reads: &Option<std::sync::Arc<Reads>>, scratch: Option<TypeId>) {
    if scratch != Some(TypeId::of::<T>())
        && let Some(reads) = reads
    {
        reads.record::<T>();
    }
}

pub struct Isolation {
    resource: TypeId,
    effectful: std::sync::atomic::AtomicBool,
}
impl Isolation {
    pub fn new<T: 'static>() -> Self {
        Self {
            resource: TypeId::of::<T>(),
            effectful: false.into(),
        }
    }
    pub fn ready<T: 'static>(&self) -> bool {
        self.resource == TypeId::of::<T>() && !self.effectful.load(Relaxed)
    }
    pub fn block(&self) {
        self.effectful.store(true, Relaxed);
    }
}

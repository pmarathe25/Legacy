//! One personal evidence store, with transient retention and sleep-encoded memories.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
mod observation;
mod requests;
mod retention;
pub use game_memory_state::{Book, Content, Fact, Facts, Memories, Trace};
pub use requests::{Initialize, ObserveScene, ObserveScenes, Recall, Receive, Significance};
pub use retention::duration as short_term_duration;
#[derive(Clone, Serialize, Deserialize)]
struct Sweep {
    person: PersonId,
}
impl Request for Sweep {
    const KEY: &'static str = "memory.sweep";
    type Response = ();
}
/// Read-only cue retrieval follows only personally established links, with a hard result bound.
pub fn retrieve<V: Content>(state: &Memories<V>, person: PersonId, cue: &str) -> Vec<String> {
    let (Some(book), Some(facts)) = (state.1.get(&person), state.0.get(&person)) else {
        return vec![];
    };
    let mut keys = std::collections::BTreeSet::new();
    let prefix = format!("{cue}:");
    for (k, _) in game_facts::with_prefix(facts, &prefix).take(16) {
        keys.insert(k.to_string());
        if let Some(t) = book.entries.get(k) {
            keys.extend(t.related.iter().map(ToString::to_string));
        }
    }
    keys.into_iter().take(16).collect()
}
pub fn plugin<V: Content>(b: &mut Builder) -> Result<()> {
    b.plugin("memory", 1)?;
    b.resource::<Memories<V>>()?;
    requests::register::<V>(b)?;
    b.request::<Sweep>(|tx, r| {
        if let Some(book) = tx.edit::<Memories<V>>()?.1.get_mut(&r.person) {
            book.sweep = None;
            book.due = None;
        }
        retention::prune::<V>(tx, r.person)?;
        retention::schedule::<V>(tx, r.person)
    })?;
    b.subscribe::<game_physiology::Rested>("memory.sleep-consolidation", |tx, e| {
        retention::sleep::<V>(tx, e.person)
    })?;
    b.validate_save(|h| {
        let s = h.get::<Memories<V>>()?;
        require(
            s.0.values().all(|f| f.valid_at(h.now(), h.revision()))
                && s.1.values().all(|b| {
                    b.entries
                        .values()
                        .all(|t| t.learned <= h.now() && t.rehearsed <= h.now())
                }),
            "memory from future",
        )
    });
    Ok(())
}

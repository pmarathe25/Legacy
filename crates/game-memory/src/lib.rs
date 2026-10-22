//! One personal evidence store, with transient retention and sleep-encoded memories.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
mod persistence;
mod requests;
mod retention;
pub use requests::{Initialize, ObserveScene, Recall, Receive, Significance};
pub use retention::duration as short_term_duration;
pub type Fact<V> = game_facts::Fact<V>;
/// Payload owners preserve their schema and restrict which details reconstruction may change.
pub trait Content: game_facts::Interpretation {
    fn reconstruct(&self, draw: u64) -> Option<Self>;
    fn cues(&self, key: &str) -> Vec<String>;
}
pub type Facts<V> = game_facts::Interpreted<V>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub learned: SimTime,
    pub rehearsed: SimTime,
    pub expires: Option<SimTime>,
    pub importance: u16,
    pub emotion: i16,
    pub strength: u16,
    pub recalls: u64,
    pub cues: Vec<String>,
    pub related: Vec<Arc<str>>,
}
impl Trace {
    fn legacy(observed: SimTime, cues: Vec<String>) -> Self {
        Self {
            learned: observed,
            rehearsed: observed,
            expires: None,
            importance: 500,
            emotion: 0,
            strength: 500,
            recalls: 0,
            cues,
            related: vec![],
        }
    }

    pub fn accessibility(&self, at: SimTime) -> u16 {
        if let Some(end) = self.expires {
            return (u128::from(end.0.saturating_sub(at.0)) * 1000
                / u128::from(end.0 - self.learned.0))
            .min(1000) as u16;
        }
        self.strength
            .saturating_sub((at.0.saturating_sub(self.rehearsed.0) / (30 * DAY.0)).min(1000) as u16)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub entries: Table<Arc<str>, Trace>,
    pub sweep: Option<ScheduleId>,
    pub due: Option<SimTime>,
}
/// The same fact payload is used by perception, decisions, reports and creative recollection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memories<V: Content>(
    pub game_packed::Packed<PersonId, Facts<V>>,
    pub Table<PersonId, Book>,
);
impl<V: Content> Default for Memories<V> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}
impl<V: Content> Resource for Memories<V> {
    // Preserve the evidence key and on-disk archive; ownership now belongs to memory.
    const KEY: &'static str = "knowledge.facts";
    fn validate(&self) -> Result<()> {
        self.validate_changes(&Self::default())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        require(
            self.0
                .iter()
                .all(|(p, f)| f.len() == self.1.get(p).map_or(0, |b| b.entries.len()))
                && self.1.changed(&previous.1).all(|(p, b)| {
                    self.0.get(p).is_some_and(|f| f.valid_confidence(None))
                        && b.entries.iter().all(|(k, t)| {
                            self.0.get(p).is_some_and(|f| f.contains_key(k))
                                && t.importance <= 1000
                                && t.emotion.unsigned_abs() <= 1000
                                && t.strength <= 1000
                                && t.expires.is_none_or(|at| at > t.learned)
                                && t.cues.len() <= 8
                                && t.cues.iter().all(|s| !s.is_empty() && s.len() <= 160)
                                && t.related.len() <= 4
                                && t.related
                                    .iter()
                                    .all(|x| x != k && b.entries.contains_key(x.as_ref()))
                        })
                }),
            "invalid personal memory",
        )
    }
}
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

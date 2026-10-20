//! Pure lookup of personally learned sound associations; no world inspection.
use game_core::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Association {
    pub sounds: Vec<u8>,
    pub strength: u16,
    pub source: PersonId,
    pub at: SimTime,
}
pub type Words = Records<String, Vec<Association>>;
const MAX_FORMS: usize = 32768;
const MAX_LEXICONS: usize = 128;
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// A view keeps only its immutable vocabulary text pool alive, never world state.
#[derive(Clone)]
pub struct Meaning {
    text: Arc<str>,
    range: std::ops::Range<usize>,
}
impl std::ops::Deref for Meaning {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text[self.range.clone()]
    }
}
impl AsRef<str> for Meaning {
    fn as_ref(&self) -> &str {
        self
    }
}
impl std::fmt::Debug for Meaning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}
impl PartialEq for Meaning {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}
impl Eq for Meaning {}
impl Meaning {
    fn owned(text: String) -> Self {
        let range = 0..text.len();
        Self {
            text: text.into(),
            range,
        }
    }
}
#[derive(Default)]
pub struct Decoder {
    entries: BTreeMap<PersonId, Entry>,
    clock: u64,
    forms: usize,
    bytes: usize,
}
struct Entry {
    version: game_storage::Version<String, Arc<Vec<Association>>>,
    used: u64,
    index: Option<Index>,
}
enum Forms {
    Short(Vec<(u64, u32, u16)>),
    Long(Vec<(Vec<u8>, u32, u16)>),
}
struct Index {
    text: Arc<str>,
    meanings: Vec<std::ops::Range<usize>>,
    forms: Forms,
    weight: usize,
    bytes: usize,
}
fn packed(sounds: &[u8]) -> Option<u64> {
    if sounds.len() > 7 {
        return None;
    }
    let mut bytes = [0; 8];
    bytes[..sounds.len()].copy_from_slice(sounds);
    bytes[7] = sounds.len() as u8;
    Some(u64::from_be_bytes(bytes))
}
fn scan(words: &Words, sounds: &[u8]) -> (Option<String>, u16) {
    words
        .iter()
        .flat_map(|(key, variants)| variants.iter().map(move |w| (key, w)))
        .filter(|(_, w)| w.sounds == sounds)
        .max_by_key(|(key, w)| (w.strength, std::cmp::Reverse(*key)))
        .map_or((None, 0), |(key, w)| (Some(key.clone()), w.strength))
}
impl Index {
    fn budget(words: &Words, count: usize) -> Option<(bool, usize, usize)> {
        let short = words.values().flatten().all(|w| w.sounds.len() <= 7);
        let text: usize = words
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.len())
            .sum();
        let mut bytes = text.saturating_add(count.saturating_mul(if short { 16 } else { 32 }));
        bytes = bytes.saturating_add(count.min(words.len()).saturating_mul(16));
        if !short {
            for word in words.values().flatten() {
                bytes = bytes.saturating_add(word.sounds.len().saturating_add(16));
            }
        }
        (bytes <= MAX_BYTES).then_some((short, text, bytes))
    }
    fn new(words: &Words, weight: usize, short: bool, text_bytes: usize, bytes: usize) -> Self {
        let mut text = String::with_capacity(text_bytes);
        let mut meanings = Vec::with_capacity(weight.min(words.len()));
        let mut numeric = Vec::with_capacity(if short { weight } else { 0 });
        let mut long = Vec::with_capacity(if short { 0 } else { weight });
        for (key, variants) in words {
            if variants.is_empty() {
                continue;
            }
            let id = meanings.len() as u32;
            let start = text.len();
            text.push_str(key);
            meanings.push(start..text.len());
            for w in variants {
                if short {
                    numeric.push((packed(&w.sounds).unwrap(), id, w.strength));
                } else {
                    long.push((w.sounds.clone(), id, w.strength));
                }
            }
        }
        // Meaning ordinals follow lexical order; preserve strongest, then lowest key.
        let forms = if short {
            numeric.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)).then(a.1.cmp(&b.1)));
            numeric.dedup_by(|a, b| a.0 == b.0);
            Forms::Short(numeric)
        } else {
            long.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)).then(a.1.cmp(&b.1)));
            long.dedup_by(|a, b| a.0 == b.0);
            Forms::Long(long)
        };
        Self {
            text: text.into(),
            meanings,
            forms,
            weight,
            bytes,
        }
    }
    fn lookup(&self, sounds: &[u8]) -> (Option<Meaning>, u16) {
        let found = match &self.forms {
            Forms::Short(rows) => packed(sounds).and_then(|key| {
                rows.binary_search_by_key(&key, |r| r.0)
                    .ok()
                    .map(|i| (rows[i].1, rows[i].2))
            }),
            Forms::Long(rows) => rows
                .binary_search_by(|r| r.0.as_slice().cmp(sounds))
                .ok()
                .map(|i| (rows[i].1, rows[i].2)),
        };
        found.map_or((None, 0), |(id, strength)| {
            (
                Some(Meaning {
                    text: self.text.clone(),
                    range: self.meanings[id as usize].clone(),
                }),
                strength,
            )
        })
    }
}
impl Decoder {
    fn remove(&mut self, person: PersonId) {
        if let Some(old) = self.entries.remove(&person)
            && let Some(index) = old.index
        {
            self.forms -= index.weight;
            self.bytes -= index.bytes;
        }
    }
    fn evict(&mut self, keep: PersonId) {
        if let Some(person) = self
            .entries
            .iter()
            .filter(|(p, _)| **p != keep)
            .min_by_key(|(p, e)| (e.used, **p))
            .map(|(p, _)| *p)
        {
            self.remove(person);
        }
    }
    /// Shared meanings avoid allocating text for pure title/category predicates.
    /// The budget applies across all retained lexicons, not per person.
    pub fn meaning(
        &mut self,
        person: PersonId,
        words: &Words,
        sounds: &[u8],
    ) -> (Option<Meaning>, u16) {
        if words.len() < 16 {
            let (meaning, strength) = scan(words, sounds);
            return (meaning.map(Meaning::owned), strength);
        }
        self.clock = self.clock.saturating_add(1);
        if self
            .entries
            .get(&person)
            .is_none_or(|old| !words.matches(&old.version))
        {
            self.remove(person);
            if self.entries.len() >= MAX_LEXICONS {
                self.evict(person);
            }
            self.entries.insert(
                person,
                Entry {
                    version: words.version(),
                    used: self.clock,
                    index: None,
                },
            );
            let (meaning, strength) = scan(words, sounds);
            return (meaning.map(Meaning::owned), strength);
        }
        if self.entries[&person].index.is_none() {
            let count = words.values().map(Vec::len).sum();
            if count > MAX_FORMS {
                let (meaning, strength) = scan(words, sounds);
                return (meaning.map(Meaning::owned), strength);
            }
            let Some((short, text, bytes)) = Index::budget(words, count) else {
                let (meaning, strength) = scan(words, sounds);
                return (meaning.map(Meaning::owned), strength);
            };
            while self.forms + count > MAX_FORMS || self.bytes + bytes > MAX_BYTES {
                self.evict(person);
            }
            self.entries.get_mut(&person).unwrap().index =
                Some(Index::new(words, count, short, text, bytes));
            self.forms += count;
            self.bytes += bytes;
        }
        let entry = self.entries.get_mut(&person).unwrap();
        entry.used = self.clock;
        entry.index.as_ref().unwrap().lookup(sounds)
    }
    /// Preserve the owned-string interface used for complete translations.
    pub fn decode(
        &mut self,
        person: PersonId,
        words: &Words,
        sounds: &[u8],
    ) -> (Option<String>, u16) {
        if words.len() < 16 {
            return scan(words, sounds);
        }
        let (meaning, strength) = self.meaning(person, words, sounds);
        (meaning.map(|m| m.to_string()), strength)
    }
}

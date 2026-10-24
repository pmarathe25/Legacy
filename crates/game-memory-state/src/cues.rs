use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct CueIndex(BTreeMap<String, BTreeSet<Arc<str>>>);
impl CueIndex {
    pub fn new(book: &Book) -> Self {
        let mut index = Self::default();
        for (key, trace) in book.entries.iter() {
            index.replace(key, &[], &trace.cues);
        }
        index
    }

    pub fn related(&self, key: &Arc<str>, cues: &[String]) -> Vec<Arc<str>> {
        let mut matches = BTreeSet::new();
        for cue in cues {
            if let Some(keys) = self.0.get(cue) {
                matches.extend(keys.iter().filter(|k| *k != key).take(4).cloned());
            }
        }
        matches.into_iter().take(4).collect()
    }

    pub fn replace(&mut self, key: &Arc<str>, old: &[String], new: &[String]) {
        for cue in old {
            if !new.contains(cue)
                && let Some(keys) = self.0.get_mut(cue)
            {
                keys.remove(key);
            }
        }
        for cue in new {
            if let Some(keys) = self.0.get_mut(cue) {
                keys.insert(key.clone());
            } else {
                self.0.insert(cue.clone(), [key.clone()].into());
            }
        }
    }
}

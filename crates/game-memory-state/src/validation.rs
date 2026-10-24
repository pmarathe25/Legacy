use super::*;
use rustc_hash::FxHashSet;

fn valid_trace(key: &Arc<str>, t: &Trace, contains: impl Fn(&Arc<str>) -> bool) -> bool {
    t.importance <= 1000
        && t.emotion.unsigned_abs() <= 1000
        && t.strength <= 1000
        && t.expires.is_none_or(|at| at > t.learned)
        && t.cues.len() <= 8
        && t.cues.iter().all(|s| !s.is_empty() && s.len() <= 160)
        && t.related.len() <= 4
        && t.related.iter().all(|x| x != key && contains(x))
}
impl<V: Content> Memories<V> {
    pub(super) fn keys_match(&self, person: &PersonId, facts: &Facts<V>) -> bool {
        self.1.get(person).map_or_else(
            || facts.is_empty(),
            |book| facts.len() == book.entries.len() && facts.keys().eq(book.entries.keys()),
        )
    }
    pub(super) fn valid_book(&self, person: &PersonId, book: &Book, old: Option<&Book>) -> bool {
        let Some(facts) = self.0.get(person) else {
            return false;
        };
        if facts.len() != book.entries.len() {
            return false;
        }
        let changed: Vec<_> = match old {
            Some(old) => book.entries.changed(&old.entries).collect(),
            None => book.entries.iter().collect(),
        };
        // Large edits use a merge and a hash join; small edits seek only changed keys.
        if changed.len() > 16 || old.is_none() {
            if !self.keys_match(person, facts) {
                return false;
            }
            let keys: FxHashSet<_> = book.entries.keys().collect();
            if !changed
                .iter()
                .all(|(key, t)| valid_trace(key, t, |x| keys.contains(x)))
            {
                return false;
            }
        } else if !changed.iter().all(|(key, t)| {
            facts.contains_key(key) && valid_trace(key, t, |x| book.entries.contains_key(x))
        }) {
            return false;
        }
        // A removed target can invalidate an otherwise unchanged trace.
        if let Some(old) = old {
            let removed: FxHashSet<_> = old
                .entries
                .changed(&book.entries)
                .filter(|(key, _)| !book.entries.contains_key(*key))
                .map(|(key, _)| key)
                .collect();
            if !removed.is_empty()
                && !book
                    .entries
                    .values()
                    .all(|t| t.related.iter().all(|key| !removed.contains(key)))
            {
                return false;
            }
        }
        true
    }
}

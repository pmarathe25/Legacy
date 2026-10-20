//! Supply indexes need payload deltas, not a retained copy of unrelated personal history.
use super::*;
const SCAN_LIMIT: usize = 512;
pub(super) fn changed(entry: &mut Entry, facts: &Facts) -> Vec<Arc<str>> {
    if let Some(previous) = &entry.facts {
        let mut changed = vec![];
        facts.changed_values(previous, |key, _| {
            if key.starts_with("store:") || key.starts_with("store-location:") {
                changed.push(key.clone());
            }
        });
        return changed;
    }
    let mut previous = entry.payloads.iter().peekable();
    let mut current = vec![];
    let mut changed = vec![];
    // These prefixes are disjoint and in lexical order, including numeric aliases.
    for (key, fact) in game_knowledge::with_prefix(facts, "store-location:")
        .chain(game_knowledge::with_prefix(facts, "store:"))
    {
        while previous.peek().is_some_and(|(old, _)| old < key) {
            changed.push(previous.next().unwrap().0.clone());
        }
        let old = previous.next_if(|(old, _)| old == key);
        if old.is_none_or(|(_, value)| value.as_ptr() != Arc::as_ptr(fact.value)) {
            changed.push(key.clone());
        }
        current.push((key.clone(), Arc::downgrade(fact.value)));
    }
    changed.extend(previous.map(|(key, _)| key.clone()));
    if current.len() > SCAN_LIMIT {
        // Large maps retain the existing structural tree diff rather than rescanning every supplier.
        entry.facts = Some(facts.clone());
        entry.payloads.clear();
    } else {
        entry.payloads = current;
    }
    changed
}

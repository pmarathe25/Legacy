use game_core::*;
use game_memory_state::*;
use std::sync::Arc;

#[test]
fn cue_join_matches_ordered_scan_through_overlap_replacement_and_missing_cues() {
    let mut book = Book::default();
    let mut index = CueIndex::new(&book);
    for n in (0..200).rev().chain(0..200) {
        let key: Arc<str> = format!("fact:{n:03}").into();
        let cues: Vec<_> = [format!("group:{}", n % 13), format!("actor:{}", n % 7)].into();
        let scan: Vec<_> = book
            .entries
            .iter()
            .filter(|(k, t)| **k != key && t.cues.iter().any(|c| cues.contains(c)))
            .take(4)
            .map(|(k, _)| k.clone())
            .collect();
        assert_eq!(index.related(&key, &cues), scan);
        let prior = book.entries.get(&key);
        index.replace(&key, prior.map_or(&[], |t| t.cues.as_slice()), &cues);
        book.entries.insert(key, Trace::legacy(SimTime(0), cues));
    }
    let key = Arc::from("fact:099");
    let old = book.entries[&key].cues.clone();
    index.replace(&key, &old, &["new".into()]);
    book.entries.get_mut(&key).unwrap().cues = vec!["new".into()];
    assert_eq!(
        index.related(&Arc::from("self"), &["new".into()]),
        vec![key.clone()]
    );
    assert!(
        index
            .related(&key, &["new".into(), "missing".into()])
            .is_empty()
    );
    for n in 0..200 {
        let key: Arc<str> = format!("fact:{n:03}").into();
        let cues = vec!["group:8".into(), "group:8".into(), "actor:1".into()];
        let scan: Vec<_> = book
            .entries
            .iter()
            .filter(|(k, t)| **k != key && t.cues.iter().any(|c| cues.contains(c)))
            .take(4)
            .map(|(k, _)| k.clone())
            .collect();
        assert_eq!(index.related(&key, &cues), scan);
    }
}

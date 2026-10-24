use game_core::*;
use game_memory_state::*;
use game_sim::Resource;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Value(u64);
impl game_facts::Interpretation for Value {
    fn promote(value: Arc<Self>, _: &str) -> Arc<Self> {
        value
    }
}
impl Content for Value {
    fn reconstruct(&self, _: u64) -> Option<Self> {
        None
    }
    fn cues(&self, _: &str) -> Vec<String> {
        vec![]
    }
}
fn insert(state: &mut Memories<Value>, person: PersonId, key: &str) {
    state.0.entry(person).or_default().insert(
        Arc::from(key),
        Fact {
            value: Arc::new(Value(1)),
            source: person,
            observed: SimTime(0),
            revision: 0,
            confidence: 100,
        },
    );
    state
        .1
        .entry(person)
        .or_default()
        .entries
        .insert(Arc::from(key), Trace::legacy(SimTime(0), vec![]));
}
#[test]
fn nested_journals_preserve_outer_work_and_restore_added_rows_and_associations() {
    let p = PersonId(1);
    let mut state = Memories::default();
    insert(&mut state, p, "initial");
    let outer = state.checkpoint();
    insert(&mut state, p, "outer");
    let expected = state.clone();
    let inner = state.checkpoint();
    insert(&mut state, PersonId(2), "inner");
    state
        .1
        .get_mut(&p)
        .unwrap()
        .entries
        .get_mut("outer")
        .unwrap()
        .related
        .push(Arc::from("initial"));
    state.validate_checkpoint(inner.as_ref()).unwrap();
    state.restore(inner);
    assert_eq!(state, expected);
    state.validate_checkpoint(outer.as_ref()).unwrap();
    state.commit(outer);
    state.validate().unwrap();
    assert_eq!(state, expected);
}
#[test]
fn incremental_validation_rejects_corrupt_facts_links_and_removed_books() {
    let p = PersonId(1);
    let mut state = Memories::default();
    insert(&mut state, p, "initial");
    let original = state.clone();
    for corruption in 0..3 {
        let checkpoint = state.checkpoint();
        match corruption {
            0 => {
                let facts = state.0.get_mut(&p).unwrap();
                let mut fact = facts.get("initial").unwrap().to_owned();
                fact.confidence = 101;
                facts.insert(Arc::from("initial"), fact);
            }
            1 => state
                .1
                .get_mut(&p)
                .unwrap()
                .entries
                .get_mut("initial")
                .unwrap()
                .related
                .push(Arc::from("missing")),
            _ => {
                state.1.remove(&p);
            }
        }
        assert!(state.validate_checkpoint(checkpoint.as_ref()).is_err());
        state.restore(checkpoint);
        assert_eq!(state, original);
    }
}

#[test]
fn incremental_validation_matches_full_validation_for_edits_and_deletions() {
    let p = PersonId(1);
    let mut original = Memories::default();
    for n in 0..40 {
        insert(&mut original, p, &format!("key:{n:02}"));
    }
    original
        .1
        .get_mut(&p)
        .unwrap()
        .entries
        .get_mut("key:00")
        .unwrap()
        .related
        .push("key:01".into());
    original.validate().unwrap();
    for edit in 0..11 {
        let mut state = original.clone();
        let checkpoint = state.checkpoint();
        match edit {
            0 | 1 => {
                state.0.get_mut(&p).unwrap().remove("key:01");
                state.1.get_mut(&p).unwrap().entries.remove("key:01");
                if edit == 1 {
                    state
                        .1
                        .get_mut(&p)
                        .unwrap()
                        .entries
                        .get_mut("key:00")
                        .unwrap()
                        .related
                        .clear();
                }
            }
            2 => {
                let facts = state.0.get_mut(&p).unwrap();
                let fact = facts.remove("key:01").unwrap();
                facts.insert("replacement".into(), fact);
            }
            3 => {
                let book = state.1.get_mut(&p).unwrap();
                let trace = book.entries.remove("key:01").unwrap();
                book.entries.insert("replacement".into(), (*trace).clone());
            }
            4 => {
                state
                    .1
                    .get_mut(&p)
                    .unwrap()
                    .entries
                    .get_mut("key:20")
                    .unwrap()
                    .strength = 1001
            }
            5 => {
                state.0.get_mut(&p).unwrap().remove("key:01");
            }
            6 => {
                for n in 0..25 {
                    state
                        .1
                        .get_mut(&p)
                        .unwrap()
                        .entries
                        .get_mut(format!("key:{n:02}").as_str())
                        .unwrap()
                        .importance = 700;
                }
                state
                    .1
                    .get_mut(&p)
                    .unwrap()
                    .entries
                    .get_mut("key:20")
                    .unwrap()
                    .related
                    .push("key:20".into());
            }
            7 => state.1.get_mut(&p).unwrap().due = Some(SimTime(100)),
            8 => {
                for n in 0..25 {
                    state
                        .1
                        .get_mut(&p)
                        .unwrap()
                        .entries
                        .get_mut(format!("key:{n:02}").as_str())
                        .unwrap()
                        .importance = 700;
                }
            }
            9 | 10 => {
                state.0.remove(&p);
                if edit == 10 {
                    state.1.remove(&p);
                }
            }
            _ => unreachable!(),
        }
        let valid = matches!(edit, 1 | 7 | 8 | 10);
        assert_eq!(state.validate().is_ok(), valid, "full edit {edit}");
        assert_eq!(
            state.validate_checkpoint(checkpoint.as_ref()).is_ok(),
            valid,
            "incremental edit {edit}"
        );
        state.restore(checkpoint);
        assert_eq!(state, original);
    }
}

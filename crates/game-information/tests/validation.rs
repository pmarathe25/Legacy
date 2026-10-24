use game_core::*;
use game_information::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

fn interest() -> Interest {
    Interest {
        topic: Topic::Category("technique:".into()),
        importance: 80,
        asked: Default::default(),
        read: Default::default(),
    }
}
#[test]
fn incremental_validation_matches_full_validation_after_tree_splits_edits_and_deletions() {
    let mut original = Information::default();
    for person in 1..=2048 {
        original
            .interests
            .insert(PersonId(person), vec![interest()]);
        original.inboxes.insert(PersonId(person), vec![]);
    }
    original.validate().unwrap();
    for person in [1, 32, 64, 65, 1024, 2048, 4096] {
        for variant in 0..8 {
            let mut changed = original.clone();
            match variant {
                0 => {
                    changed.interests.insert(PersonId(person), vec![interest()]);
                }
                1 => {
                    changed
                        .interests
                        .entry(PersonId(person))
                        .or_default()
                        .push(interest());
                }
                2 => {
                    changed
                        .interests
                        .entry(PersonId(person))
                        .or_default()
                        .push(Interest {
                            importance: 0,
                            ..interest()
                        });
                }
                3 => {
                    changed
                        .interests
                        .insert(PersonId(person), vec![interest(); 9]);
                }
                4 => {
                    changed.inboxes.insert(
                        PersonId(person),
                        vec![Question {
                            id: 1,
                            asker: PersonId(1),
                            topic: Topic::Category("".into()),
                            at: SimTime(0),
                        }],
                    );
                }
                5 => {
                    changed.inboxes.insert(
                        PersonId(person),
                        vec![
                            Question {
                                id: 1,
                                asker: PersonId(1),
                                topic: interest().topic,
                                at: SimTime(0)
                            };
                            17
                        ],
                    );
                }
                6 => {
                    changed.interests.remove(&PersonId(person));
                    changed.inboxes.remove(&PersonId(person));
                }
                _ => {
                    changed.interests = Default::default();
                    changed.inboxes = Default::default();
                }
            }
            assert_eq!(
                changed.validate().is_ok(),
                changed.validate_changes(&original).is_ok(),
                "{person} {variant}"
            );
            original.validate().unwrap();
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Edit(bool);
impl Request for Edit {
    const KEY: &'static str = "test.information-edit";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Nested;
impl Request for Nested {
    const KEY: &'static str = "test.information-nested";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Fail;
impl Request for Fail {
    const KEY: &'static str = "test.information-fail";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("test-information", 1).unwrap();
    b.resource::<Information>().unwrap();
    b.request::<Edit>(|tx, r| {
        let row = tx
            .edit::<Information>()?
            .interests
            .entry(PersonId(1))
            .or_default();
        row.push(Interest {
            importance: if r.0 { 0 } else { 80 },
            ..interest()
        });
        Ok(())
    })
    .unwrap();
    b.request::<Fail>(|tx, _| {
        tx.call(Edit(true))?;
        require(false, "rollback an invalid nested edit")
    })
    .unwrap();
    b.request::<Nested>(|tx, _| {
        tx.call(Edit(false))?;
        assert!(tx.attempt(Fail).is_err());
        assert_eq!(tx.get::<Information>()?.interests[&PersonId(1)].len(), 2);
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn invalid_direct_edits_roll_back_and_loaded_worlds_still_receive_full_validation() {
    let mut host = builder().build(7, 1).unwrap();
    host.dispatch(Edit(false)).unwrap();
    let before = host.save().unwrap();
    assert!(host.dispatch(Edit(true)).is_err());
    assert_eq!(host.save().unwrap(), before);
    host.dispatch(Nested).unwrap();
    let saved = host.save().unwrap();
    let loaded = builder().load(&saved, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
    let mut broken: serde_json::Value = serde_json::from_str(&saved).unwrap();
    let importance = &mut broken["resources"][Information::KEY]["interests"]["1"][0]["importance"];
    assert_eq!(*importance, 80);
    *importance = 0.into();
    assert!(builder().load(&broken.to_string(), 1).is_err());
}

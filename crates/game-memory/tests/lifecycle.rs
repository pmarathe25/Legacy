use game_core::*;
use game_knowledge::{Conclude, Knowledge};
use game_memory::{Recall, Significance};
use game_sim::*;
use serde::{Deserialize, Serialize};
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    b.plugin("test-memory", 1).unwrap();
    b.request::<Cancel>(|tx, r| tx.cancel_action(r.0)).unwrap();
    b.request::<Fail>(|tx, r| {
        tx.call(Recall {
            person: r.0,
            key: "episode:important".into(),
        })?;
        Err(Error::Rejected("rollback".into()))
    })
    .unwrap();
    b.request::<FailScene>(|tx, r| {
        tx.call(scene(r.0))?;
        Err(Error::Rejected("rollback scene".into()))
    })
    .unwrap();
    b.request::<NestedScene>(|tx, r| {
        tx.call(scene(r.0))?;
        let expected = tx.get::<Knowledge>()?.clone();
        assert!(tx.attempt(FailScene(r.0)).is_err());
        assert_eq!(&expected, tx.get::<Knowledge>()?);
        tx.call(Conclude {
            person: r.0,
            key: "zzafter:rollback".into(),
            value: serde_json::json!(1),
            confidence: 80,
        })
    })
    .unwrap();
    b
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(PersonId);
impl Request for Cancel {
    const KEY: &'static str = "test-memory.cancel";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Fail(PersonId);
impl Request for Fail {
    const KEY: &'static str = "test-memory.fail";
    type Response = ();
}
fn setup() -> (Host, PersonId, PersonId) {
    let mut h = builder().build(7, 1).unwrap();
    let mut store = game_content::Store::standard();
    h.dispatch(game_traits::Install(game_traits::load(&mut store).unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        people.push(p);
    }
    (h, people[0], people[1])
}
fn run(h: &mut Host, t: SimTime) {
    while h.advance(t, 1000).unwrap().stop != Stop::Target {}
}
fn learn(h: &mut Host, p: PersonId, key: &str) {
    h.dispatch(Conclude {
        person: p,
        key: key.into(),
        value: serde_json::json!({"quantity":100,"actor":"person:7","@predicate":"relation:fed"}),
        confidence: 80,
    })
    .unwrap();
}
fn importance(h: &mut Host, p: PersonId, key: &str, i: u16, e: i16) {
    h.dispatch(Significance {
        person: p,
        key: key.into(),
        importance: i,
        emotion: e,
    })
    .unwrap();
}
fn sleep(h: &mut Host, p: PersonId) {
    let until = h.dispatch(game_physiology::Rest { person: p }).unwrap();
    run(h, until);
}
#[test]
fn completed_sleep_selects_significance_without_making_every_observation_long_term() {
    let (mut h, p, q) = setup();
    for key in ["episode:mundane", "episode:important", "episode:emotional"] {
        learn(&mut h, p, key);
    }
    importance(&mut h, p, "episode:important", 700, 0);
    importance(&mut h, p, "episode:emotional", 50, -900);
    sleep(&mut h, p);
    let s = h.get::<Knowledge>().unwrap();
    assert!(s.1[&p].entries["episode:mundane"].expires.is_some());
    for k in ["episode:important", "episode:emotional"] {
        assert!(s.1[&p].entries[k].expires.is_none());
    }
    assert!(s.0.get(&q).is_none());
    assert!(s.1[&p].entries["episode:mundane"].accessibility(SimTime(DAY.0)) < 1000);
    run(&mut h, SimTime(3 * DAY.0));
    let s = h.get::<Knowledge>().unwrap();
    assert!(!s.0[&p].contains_key("episode:mundane"));
    assert!(s.0[&p].contains_key("episode:important"));
}
#[test]
fn interrupted_rest_and_read_only_queries_do_not_encode_rehearse_or_reconstruct() {
    let (mut h, p, _) = setup();
    learn(&mut h, p, "episode:important");
    importance(&mut h, p, "episode:important", 800, 0);
    h.dispatch(game_physiology::Rest { person: p }).unwrap();
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, SimTime(HOUR.0));
    let before = h.save().unwrap();
    for _ in 0..20 {
        let s = h.get::<Knowledge>().unwrap();
        assert!(s.0[&p].get("episode:important").is_some());
        assert!(!game_memory::retrieve(s, p, "episode").is_empty());
    }
    assert_eq!(before, h.save().unwrap());
    assert!(
        h.get::<Knowledge>().unwrap().1[&p].entries["episode:important"]
            .expires
            .is_some()
    );
    run(&mut h, SimTime(3 * DAY.0));
    assert!(!h.get::<Knowledge>().unwrap().0[&p].contains_key("episode:important"));
}
#[test]
fn genetic_potential_changes_short_retention_and_limits_long_term_capacity() {
    let (mut h, p, q) = setup();
    for (person, value) in [(p, 0), (q, 4000)] {
        h.dispatch(game_traits::Set {
            person,
            profile: [
                ("memory_capacity".into(), value),
                ("memory_durability".into(), value),
            ]
            .into(),
        })
        .unwrap();
    }
    for n in 0..40 {
        let k = format!("episode:{n}");
        for person in [p, q] {
            learn(&mut h, person, &k);
            importance(&mut h, person, &k, 700, 0);
        }
    }
    sleep(&mut h, p);
    sleep(&mut h, q);
    let s = h.get::<Knowledge>().unwrap();
    assert_eq!(s.1[&p].entries.len(), 32);
    assert_eq!(s.1[&q].entries.len(), 40);
    learn(&mut h, p, "ordinary");
    learn(&mut h, q, "ordinary");
    run(&mut h, SimTime(DAY.0));
    let s = h.get::<Knowledge>().unwrap();
    assert!(!s.0[&p].contains_key("ordinary"));
    assert!(s.0[&q].contains_key("ordinary"));
}
#[test]
fn reconstruction_is_small_private_rollback_safe_and_replayable_across_workers() {
    let (mut h, p, q) = setup();
    learn(&mut h, p, "episode:important");
    importance(&mut h, p, "episode:important", 800, 0);
    sleep(&mut h, p);
    let before = h.save().unwrap();
    assert!(h.dispatch(Fail(p)).is_err());
    assert_eq!(before, h.save().unwrap());
    let original = h.get::<Knowledge>().unwrap().0[&p]
        .get("episode:important")
        .unwrap()
        .to_owned();
    let mut other = builder().load(&before, 4).unwrap();
    let salt = "episode:important"
        .bytes()
        .fold(p.0, |s, b| s.wrapping_mul(131).wrapping_add(u64::from(b)));
    let draws = (0..2000)
        .position(|i| random(7, salt, i) % 10000 < 78)
        .unwrap()
        + 1;
    for _ in 0..draws {
        for host in [&mut h, &mut other] {
            host.dispatch(Recall {
                person: p,
                key: "episode:important".into(),
            })
            .unwrap();
        }
    }
    assert_eq!(h.save().unwrap(), other.save().unwrap());
    let s = h.get::<Knowledge>().unwrap();
    let f = s.0[&p].get("episode:important").unwrap();
    let v: serde_json::Value = f.value.decode().unwrap();
    assert!(matches!(v["quantity"].as_u64(), Some(99 | 101)));
    assert_eq!(v["actor"], "person:7");
    assert_eq!(f.source, original.source);
    assert_eq!(f.observed, original.observed);
    assert_eq!(f.revision, original.revision);
    assert_eq!(f.confidence, 79);
    assert!(s.0.get(&q).is_none());
    h.dispatch(game_testimony::Report {
        sender: p,
        recipient: q,
        key: "episode:important".into(),
        delay: HOUR,
    })
    .unwrap();
    run(&mut h, SimTime(2 * HOUR.0));
    assert_eq!(
        h.get::<Knowledge>().unwrap().0[&q]
            .get("episode:important")
            .unwrap()
            .observed,
        original.observed
    );
}
#[test]
fn forgotten_scene_entries_do_not_change_another_persons_scene_or_survive_loading() {
    let (mut h, p, q) = setup();
    for person in [p, q] {
        h.dispatch(game_knowledge::ObserveSurroundings { person })
            .unwrap();
    }
    let key = h.get::<Knowledge>().unwrap().0[&p]
        .keys()
        .next()
        .unwrap()
        .to_string();
    importance(&mut h, q, &key, 800, 0);
    sleep(&mut h, q);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    run(&mut loaded, SimTime(3 * DAY.0));
    let s = loaded.get::<Knowledge>().unwrap();
    assert!(!s.0[&p].contains_key(&key));
    assert!(s.0[&q].contains_key(&key));
}
#[test]
fn legacy_evidence_migrates_in_place_and_never_reconstructs_from_hidden_world_data() {
    let raw = serde_json::json!({"7":{"episode:old":{"value":{"quantity":12},"source":9,"observed":0,"revision":1,"confidence":60}}});
    let state: Knowledge = serde_json::from_value(raw).unwrap();
    assert_eq!(
        state.0[&PersonId(7)].get("episode:old").unwrap().source,
        PersonId(9)
    );
    assert!(
        state.1[&PersonId(7)].entries["episode:old"]
            .expires
            .is_none()
    );
    assert!(game_memory::retrieve(&state, PersonId(8), "episode").is_empty());
    let saved = serde_json::to_string(&state).unwrap();
    assert_eq!(state, serde_json::from_str(&saved).unwrap());
}
#[test]
fn inherited_reliability_changes_reconstruction_risk_without_changing_the_evidence_source() {
    let (mut low, p, _) = setup();
    learn(&mut low, p, "episode:reliability");
    importance(&mut low, p, "episode:reliability", 800, 0);
    sleep(&mut low, p);
    let mut high = builder().load(&low.save().unwrap(), 4).unwrap();
    for (host, value) in [(&mut low, 0), (&mut high, 4000)] {
        host.dispatch(game_traits::Set {
            person: p,
            profile: [("memory_reliability".into(), value)].into(),
        })
        .unwrap();
    }
    let salt = "episode:reliability"
        .bytes()
        .fold(p.0, |s, b| s.wrapping_mul(131).wrapping_add(u64::from(b)));
    let first = (0..10000)
        .position(|i| random(7, salt, i) % 10000 < 100)
        .unwrap();
    assert!(
        random(7, salt, first as u64) % 10000 >= 10,
        "fixture should isolate a risk changed by reliability"
    );
    for _ in 0..=first {
        for h in [&mut low, &mut high] {
            h.dispatch(Recall {
                person: p,
                key: "episode:reliability".into(),
            })
            .unwrap();
        }
    }
    let a = low.get::<Knowledge>().unwrap().0[&p]
        .get("episode:reliability")
        .unwrap()
        .confidence;
    let b = high.get::<Knowledge>().unwrap().0[&p]
        .get("episode:reliability")
        .unwrap()
        .confidence;
    assert_eq!((a, b), (79, 80));
}
#[test]
fn reobserving_changed_content_needs_fresh_consolidation_and_expiration_removes_links() {
    let (mut h, p, _) = setup();
    learn(&mut h, p, "episode:important");
    learn(&mut h, p, "episode:related");
    importance(&mut h, p, "episode:important", 800, 0);
    sleep(&mut h, p);
    h.dispatch(Conclude {
        person: p,
        key: "episode:important".into(),
        value: serde_json::json!({"quantity":20}),
        confidence: 90,
    })
    .unwrap();
    assert!(
        h.get::<Knowledge>().unwrap().1[&p].entries["episode:important"]
            .expires
            .is_some()
    );
    run(&mut h, SimTime(3 * DAY.0));
    let s = h.get::<Knowledge>().unwrap();
    assert!(s.1[&p].entries.values().all(|t| t.related.is_empty()));
}
#[test]
fn long_term_accessibility_fades_without_rehearsal_while_copied_records_remain_unchanged() {
    let (mut h, p, _) = setup();
    learn(&mut h, p, "episode:important");
    importance(&mut h, p, "episode:important", 500, 0);
    sleep(&mut h, p);
    let record = h.get::<Knowledge>().unwrap().0[&p]
        .get("episode:important")
        .unwrap()
        .to_owned();
    run(&mut h, SimTime(100 * 30 * DAY.0));
    assert!(
        h.get::<Knowledge>().unwrap().1[&p].entries["episode:important"].accessibility(h.now())
            < 500
    );
    run(&mut h, SimTime(501 * 30 * DAY.0));
    assert!(!h.get::<Knowledge>().unwrap().0[&p].contains_key("episode:important"));
    assert_eq!(
        record.value.decode::<serde_json::Value>().unwrap()["quantity"],
        100
    );
}
#[test]
fn short_term_crowding_preserves_an_encoded_memory_and_bounds_personal_associations() {
    let (mut h, p, _) = setup();
    learn(&mut h, p, "episode:important");
    importance(&mut h, p, "episode:important", 900, 0);
    sleep(&mut h, p);
    for n in 0..520 {
        learn(&mut h, p, &format!("topic:{n}"));
    }
    let s = h.get::<Knowledge>().unwrap();
    assert_eq!(s.0[&p].len(), 513);
    assert!(s.0[&p].contains_key("episode:important"));
    assert!(game_memory::retrieve(s, p, "topic").len() <= 16);
    assert!(s.1[&p].entries.values().all(|t| t.related.len() <= 4));
}

#[derive(Clone, Serialize, Deserialize)]
struct FailScene(PersonId);
impl Request for FailScene {
    const KEY: &'static str = "test-memory.fail-scene";
    type Response = ();
}
fn scene(person: PersonId) -> game_memory::ObserveScene<game_knowledge::FactValue> {
    let entries = (0..700)
        .map(|n| {
            (
                std::sync::Arc::from(format!("episode:{n:04}")),
                std::sync::Arc::new(game_knowledge::FactValue::from(
                    serde_json::json!({"quantity":n}),
                )),
            )
        })
        .collect();
    game_memory::ObserveScene {
        person,
        scene: std::sync::Arc::new(game_facts::Scene::new(entries, None).unwrap()),
    }
}
#[test]
fn oversized_observation_is_atomic_bounded_and_replayable() {
    let (mut h, p, q) = setup();
    let before = h.save().unwrap();
    assert!(h.dispatch(FailScene(p)).is_err());
    assert_eq!(before, h.save().unwrap());
    h.dispatch(scene(q)).unwrap();
    let mut other = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut other] {
        host.dispatch(scene(p)).unwrap();
        host.dispatch(scene(p)).unwrap();
        let s = host.get::<Knowledge>().unwrap();
        assert_eq!(s.0[&p].len(), 512);
        assert_eq!(s.1[&p].entries.len(), 512);
        assert_eq!(s.0[&q].len(), 512);
        for (k, t) in s.1[&p].entries.iter() {
            assert!(s.0[&p].contains_key(k));
            assert!(t.related.iter().all(|k| s.1[&p].entries.contains_key(k)));
        }
    }
    assert_eq!(h.save().unwrap(), other.save().unwrap());
    run(&mut h, SimTime(3 * DAY.0));
    assert!(h.get::<Knowledge>().unwrap().0[&p].is_empty());
}
#[test]
fn oversized_initialization_records_before_crowding_and_preserves_tie_breaking() {
    let (mut h, p, _) = setup();
    let facts = scene(p)
        .scene
        .entries()
        .map(|(k, value)| {
            (
                k.clone(),
                game_memory::Fact {
                    value: value.clone(),
                    source: p,
                    observed: SimTime(0),
                    revision: h.revision(),
                    confidence: 100,
                },
            )
        })
        .collect();
    h.dispatch(game_memory::Initialize { person: p, facts })
        .unwrap();
    let s = h.get::<Knowledge>().unwrap();
    assert_eq!(s.0[&p].len(), 512);
    assert_eq!(s.0[&p].keys().next().unwrap().as_ref(), "episode:0188");
    assert_eq!(s.0[&p].keys().last().unwrap().as_ref(), "episode:0699");
}

#[derive(Clone, Serialize, Deserialize)]
struct NestedScene(PersonId);
impl Request for NestedScene {
    const KEY: &'static str = "test-memory.nested-scene";
    type Response = ();
}
#[test]
fn failed_inner_batch_restores_book_and_fact_journals_without_discarding_outer_changes() {
    let (mut h, p, _) = setup();
    h.dispatch(NestedScene(p)).unwrap();
    assert!(h.get::<Knowledge>().unwrap().0[&p].contains_key("zzafter:rollback"));
    assert_eq!(h.get::<Knowledge>().unwrap().0[&p].len(), 512);
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}

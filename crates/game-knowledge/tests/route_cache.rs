use game_core::*;
use game_knowledge::*;
use std::{collections::BTreeMap, sync::Arc};

fn roads(facts: &mut Facts, from: u64, links: &[(u64, u64)], stamp: u64) {
    facts.insert(
        roads_key(AreaId(from)).into(),
        Fact {
            value: Arc::new(FactValue::Roads(Arc::new(game_land::Area {
                name: format!("area {from}"),
                links: links
                    .iter()
                    .map(|(id, d)| (AreaId(*id), SimDuration(*d)))
                    .collect(),
            }))),
            source: PersonId(stamp),
            observed: SimTime(stamp),
            revision: stamp,
            confidence: 50,
        },
    );
}

#[test]
fn remembered_routes_invalidate_on_learning_revision_rollback_and_world_switches() {
    let mut facts = Facts::default();
    let from = AreaId(1);
    let to = AreaId(3);
    assert!(route(&facts, from, to).is_err());
    roads(&mut facts, 1, &[(2, 4)], 1);
    assert!(route(&facts, from, to).is_err());
    let old = facts.clone();
    roads(&mut facts, 2, &[(3, 7)], 2);
    // The endpoint's outgoing roads need not be known.
    assert_eq!(route(&facts, from, to).unwrap().duration, SimDuration(11));
    assert!(route(&old, from, to).is_err());
    let learned = facts.clone();
    roads(&mut facts, 1, &[(3, 2)], 3);
    assert_eq!(route(&facts, from, to).unwrap().areas.as_ref(), &[from, to]);
    assert_eq!(route(&learned, from, to).unwrap().duration, SimDuration(11));
    let loaded: Facts = serde_json::from_str(&serde_json::to_string(&learned).unwrap()).unwrap();
    assert_eq!(route(&loaded, from, to), route(&learned, from, to));
    let mut different_world = Facts::default();
    roads(&mut different_world, 1, &[(3, 99)], 1);
    assert_eq!(
        route(&different_world, from, to).unwrap().duration,
        SimDuration(99)
    );
    assert_eq!(route(&learned, from, to).unwrap().duration, SimDuration(11));
}

#[test]
fn non_road_evidence_and_metadata_do_not_change_remembered_routes() {
    let mut facts = Facts::default();
    roads(&mut facts, 1, &[(2, 8)], 1);
    let before = facts.clone();
    let version = facts.namespace_version("roads");
    let key = roads_key(AreaId(1));
    let mut fact = facts.get(&key).unwrap().to_owned();
    fact.observed = SimTime(100);
    fact.confidence = 1;
    facts.insert(key.into(), fact);
    assert!(version.ptr_eq(&facts.namespace_version("roads")));
    facts.insert(
        "unrelated".into(),
        Fact {
            value: Arc::new(FactValue::Other(serde_json::json!({"other": 3}))),
            source: PersonId(9),
            observed: SimTime(10),
            revision: 11,
            confidence: 20,
        },
    );
    assert_eq!(
        route(&facts, AreaId(1), AreaId(2)),
        route(&before, AreaId(1), AreaId(2))
    );
    assert_eq!(
        route(&facts, AreaId(1), AreaId(1)).unwrap().duration,
        SimDuration(0)
    );
    // A malformed road record is remembered evidence, not access to the true map.
    facts.insert(
        roads_key(AreaId(1)).into(),
        Fact {
            value: Arc::new(FactValue::Other(serde_json::json!(BTreeMap::from([(
                "closed", true
            )])))),
            source: PersonId(1),
            observed: SimTime(11),
            revision: 12,
            confidence: 100,
        },
    );
    assert!(route(&facts, AreaId(1), AreaId(2)).is_err());
}

use game_core::*;
use game_knowledge::{Fact, FactValue, Facts};
use std::{collections::BTreeMap, sync::Arc};
fn remember(facts: &mut Facts, id: u64, links: &[u64]) {
    facts.insert(
        format!("roads:{id}").into(),
        Fact {
            value: Arc::new(FactValue::Roads(Arc::new(game_land::Area {
                name: format!("area {id}"),
                links: links.iter().map(|id| (AreaId(*id), MINUTE)).collect(),
            }))),
            source: PersonId(1),
            observed: SimTime(0),
            revision: 0,
            confidence: 100,
        },
    );
}
fn reference(facts: &Facts) -> Vec<AreaId> {
    let mut found = BTreeMap::new();
    for (_, f) in game_knowledge::with_prefix(facts, "roads:") {
        if let Some(area) = f.value.decode::<game_land::Area>() {
            for next in area.links.keys() {
                if !facts.contains_key(&game_knowledge::roads_key(*next)) {
                    found.insert(*next, ());
                }
            }
        }
    }
    found.into_keys().collect()
}
#[test]
fn cached_frontiers_match_personal_knowledge_after_learning_rollback_eviction_and_loading() {
    let mut views = Vec::new();
    for id in 1..150 {
        let mut f = Facts::default();
        remember(&mut f, id, &[id + 1, id + 2]);
        views.push(f);
    }
    for _ in 0..3 {
        for facts in &views {
            assert_eq!(game_provisioning::frontiers(facts), reference(facts));
        }
    }
    let before = views[0].clone();
    remember(&mut views[0], 2, &[3, 4]);
    for facts in [&views[0], &before, &views[0]] {
        assert_eq!(game_provisioning::frontiers(facts), reference(facts));
    }
    let loaded: Facts = serde_json::from_str(&serde_json::to_string(&views[0]).unwrap()).unwrap();
    assert_eq!(game_provisioning::frontiers(&loaded), reference(&loaded));
    let mut large = Facts::default();
    remember(&mut large, 1, &(2..400).collect::<Vec<_>>());
    for _ in 0..3 {
        assert_eq!(game_provisioning::frontiers(&large), reference(&large));
    }
}

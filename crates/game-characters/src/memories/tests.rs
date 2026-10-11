use super::*;
use crate::decisions::{Aspiration, Explanation, Goal, Personality};
use game_core::{Good, InventoryId, PersonId, Quantity};
use std::collections::BTreeMap;
fn mind() -> Mind<u8> {
    Mind {
        interests: BTreeMap::new(),
        routes: BTreeMap::new(),
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality: Personality::industrious(),
        dream: Aspiration::Independence,
        knowledge: BTreeMap::new(),
        sites: BTreeMap::new(),
        relationships: BTreeMap::new(),
        values: BTreeMap::new(),
        goal: Goal::Practice,
        plan: None,
        explanation: Explanation {
            goal: Goal::Practice,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        },
        retry_after: SimTime(0),
    }
}
fn fact(observed_at: u64, accessible: bool) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(1),
        area: AreaId(2),
        good: Good::Food,
        quantity: Quantity(1000),
        observed_at: SimTime(observed_at),
        learned_at: SimTime(observed_at),
        source: KnowledgeSource::Observed(PersonId(3)),
        accessible,
    }
}
#[test]
fn remembered_stock_keeps_current_rights_and_rejects_older_information() {
    let mut mind = mind();
    let source = KnowledgeSource::Recorded(game_core::RecordId(4));
    assert!(mind.learn_memory_stock(SimTime(30), fact(10, true), source.clone()));
    assert!(!mind.knowledge[&InventoryId(1)].accessible);
    assert_eq!(mind.knowledge[&InventoryId(1)].learned_at, SimTime(30));
    assert_eq!(mind.knowledge[&InventoryId(1)].source, source);
    mind.knowledge.insert(InventoryId(1), fact(20, true));
    let before = mind.clone();
    assert!(!mind.learn_memory_stock(
        SimTime(40),
        fact(10, false),
        KnowledgeSource::Reported(PersonId(5))
    ));
    assert_eq!(mind, before);
    assert!(mind.learn_memory_stock(
        SimTime(40),
        fact(20, false),
        KnowledgeSource::Reported(PersonId(5))
    ));
    assert!(mind.knowledge[&InventoryId(1)].accessible);
}
#[test]
fn historical_routes_fill_missing_edges_while_current_routes_keep_priority() {
    let mut mind = mind();
    mind.routes
        .insert(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(100))]));
    mind.learn_memory_route(AreaId(1), AreaId(2), SimDuration(200));
    mind.learn_memory_route(AreaId(1), AreaId(3), SimDuration(300));
    assert_eq!(mind.routes[&AreaId(1)][&AreaId(2)], SimDuration(100));
    assert_eq!(mind.routes[&AreaId(1)][&AreaId(3)], SimDuration(300));
}

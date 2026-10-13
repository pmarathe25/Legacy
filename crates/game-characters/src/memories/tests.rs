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
    let mut mind = crate::decisions::Stocks::default();
    let source = KnowledgeSource::Recorded(game_core::RecordId(4));
    assert!(mind.learn_memory_stock(SimTime(30), fact(10, true), source.clone()));
    assert!(!mind.known[&InventoryId(1)].accessible);
    assert_eq!(mind.known[&InventoryId(1)].learned_at, SimTime(30));
    assert_eq!(mind.known[&InventoryId(1)].source, source);
    mind.known.insert(InventoryId(1), fact(20, true));
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
    assert!(mind.known[&InventoryId(1)].accessible);
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

#[test]
fn prepared_learning_preserves_current_priority_and_unrelated_mind_updates() {
    let actor = PersonId(10);
    let mut schema = game_ecs::Schema::new();
    crate::decisions::register_minds::<u8>(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    crate::decisions::insert_mind(&mut store, actor, mind()).unwrap();
    let receipt = prepare_memory_stock::<u8>(
        &store,
        actor,
        SimTime(30),
        fact(10, true),
        KnowledgeSource::Reported(PersonId(3)),
    )
    .unwrap();
    crate::decisions::mind_mut::<u8>(&mut store, actor)
        .unwrap()
        .dream = Aspiration::Comfort;
    let mut batch = game_ecs::Batch::new();
    let receipt = receipt.stage_into::<u8>(&store, &mut batch).unwrap();
    receipt.validate_batch::<u8>(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    receipt.finish::<u8>(&mut store);
    let row = crate::decisions::mind::<u8>(&store, actor).unwrap();
    assert_eq!(row.dream, Aspiration::Comfort);
    assert!(!crate::decisions::stocks(&store, actor).unwrap().known[&InventoryId(1)].accessible);
    crate::decisions::stocks_mut(&mut store, actor)
        .unwrap()
        .known
        .insert(InventoryId(1), fact(20, true));
    let receipt = prepare_memory_stock::<u8>(
        &store,
        actor,
        SimTime(40),
        fact(10, false),
        KnowledgeSource::Reported(PersonId(3)),
    )
    .unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = receipt.stage_into::<u8>(&store, &mut batch).unwrap();
    receipt.validate_batch::<u8>(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    receipt.finish::<u8>(&mut store);
    assert_eq!(
        crate::decisions::stocks(&store, actor).unwrap().known[&InventoryId(1)],
        fact(20, true)
    );
    for (origin, destination, duration) in [
        (AreaId(0), AreaId(2), SimDuration(1)),
        (AreaId(1), AreaId(1), SimDuration(1)),
        (AreaId(1), AreaId(2), SimDuration(0)),
    ] {
        assert!(prepare_memory_route::<u8>(&store, actor, origin, destination, duration).is_err());
    }
}

#[test]
fn route_learning_is_canonical_in_the_source_batch_and_rejects_late_replacement() {
    let actor = PersonId(10);
    let mut schema = game_ecs::Schema::new();
    crate::decisions::register_minds::<u8>(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    crate::decisions::insert_mind(&mut store, actor, mind()).unwrap();
    let plan =
        prepare_memory_route::<u8>(&store, actor, AreaId(1), AreaId(2), SimDuration(300)).unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = plan.stage_into::<u8>(&store, &mut batch).unwrap();
    assert!(
        crate::decisions::mind::<u8>(&store, actor)
            .unwrap()
            .routes
            .is_empty()
    );
    assert_eq!(
        batch
            .projected::<Mind<u8>>(&store, game_ecs::StableId(actor.0))
            .unwrap()
            .unwrap()
            .routes[&AreaId(1)][&AreaId(2)],
        SimDuration(300)
    );
    receipt.validate_batch::<u8>(&store, &batch).unwrap();
    batch.insert(game_ecs::StableId(actor.0), mind());
    assert_eq!(
        receipt.validate_batch::<u8>(&store, &batch),
        Err(game_core::Error::Unavailable)
    );
    assert!(
        crate::decisions::mind::<u8>(&store, actor)
            .unwrap()
            .routes
            .is_empty()
    );
    let plan =
        prepare_memory_route::<u8>(&store, actor, AreaId(1), AreaId(2), SimDuration(300)).unwrap();
    let mut batch = game_ecs::Batch::new();
    let receipt = plan.stage_into::<u8>(&store, &mut batch).unwrap();
    receipt.validate_batch::<u8>(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        crate::decisions::mind::<u8>(&store, actor).unwrap().routes[&AreaId(1)][&AreaId(2)],
        SimDuration(300)
    );
}

use super::*;
use game_characters::decisions::{Aspiration, Explanation, Goal, Mind, Personality};
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
#[test]
fn historical_content_reaches_current_character_through_the_receiving_owner() {
    let mut schema = game_ecs::Schema::new();
    game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    let actor = PersonId(10);
    game_characters::decisions::insert_mind(&mut store, actor, mind()).unwrap();
    let content = RecordedMemory::Stock {
        life: PersonId(1),
        fact: StockKnowledge {
            inventory: InventoryId(2),
            area: AreaId(3),
            good: Good::Food,
            quantity: Quantity(1000),
            accessible: true,
            observed_at: SimTime(50),
            learned_at: SimTime(50),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
    };
    assert_eq!(content.life(), PersonId(1));
    content.teach_into(
        game_characters::decisions::mind_mut::<u8>(&mut store, actor).unwrap(),
        SimTime(100),
        KnowledgeSource::Recorded(RecordId(4)),
    );
    let learned = &game_characters::decisions::mind::<u8>(&store, actor)
        .unwrap()
        .knowledge[&InventoryId(2)];
    assert_eq!(learned.learned_at, SimTime(100));
    assert_eq!(learned.observed_at, SimTime(50));
    assert_eq!(learned.source, KnowledgeSource::Recorded(RecordId(4)));
    assert!(!learned.accessible);
    let route = RecordedMemory::Route {
        life: PersonId(1),
        origin: AreaId(3),
        destination: AreaId(5),
        duration: SimDuration(1000),
    };
    route.teach_into(
        game_characters::decisions::mind_mut::<u8>(&mut store, actor).unwrap(),
        SimTime(100),
        KnowledgeSource::Reported(PersonId(6)),
    );
    let restored =
        game_ecs::Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored, store);
    assert_eq!(
        game_characters::decisions::mind::<u8>(&restored, actor)
            .unwrap()
            .routes[&AreaId(3)][&AreaId(5)],
        SimDuration(1000)
    );
}

use super::*;
use crate::memory_start::tests::{ACTOR, LIFE, fixture, request};
use game_characters::decisions::{Mind, mind, mind_mut, stocks, stocks_mut};

fn start(
    runtime: &Runtime,
    store: &mut Store,
    facts: &impl MemoryFacts,
    request: &InvocationAction,
) -> MemoryAction {
    start_at(runtime, store, facts, request, SimTime(0))
}
fn start_at(
    runtime: &Runtime,
    store: &mut Store,
    facts: &impl MemoryFacts,
    request: &InvocationAction,
    now: SimTime,
) -> MemoryAction {
    let mut batch = Batch::new();
    let action = runtime
        .prepare_memory_start::<u8>(store, facts, ACTOR, request, now)
        .unwrap()
        .stage_into::<u8>(runtime, store, facts, now, &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    action
}
fn complete(
    runtime: &Runtime,
    store: &mut Store,
    facts: &impl MemoryFacts,
    now: SimTime,
    next_id: u64,
) -> MemoryEffect {
    let plan = runtime
        .prepare_memory_completion::<u8>(store, facts, ACTOR, now, next_id)
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into::<u8>(runtime, store, facts, now, &mut batch)
        .unwrap();
    finish.validate_batch::<u8>(store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    let effect = finish.effect.clone();
    finish.finish_learning::<u8>(store);
    effect
}
#[test]
fn written_native_record_completes_at_deadline_and_reading_survives_dismissal() {
    let (runtime, mut store, facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    assert_eq!(
        runtime.due_memory_actions(&store, SimTime(writing.completes.0 - 1)),
        Vec::new()
    );
    assert_eq!(
        runtime.memory_boundary(&store, SimTime(0), SimTime(u64::MAX)),
        writing.completes
    );
    assert_eq!(
        runtime.memory_allocation_bound(&store, writing.completes),
        2
    );
    assert_eq!(
        complete(&runtime, &mut store, &facts, writing.completes, 40),
        MemoryEffect::Recorded {
            record: RecordId(40),
            actor: ACTOR,
            inventory: InventoryId(41)
        }
    );
    assert!(
        runtime.record_views(&store, &facts, ACTOR)[0]
            .content
            .is_none()
    );
    assert!(
        !stocks(&store, ACTOR)
            .unwrap()
            .known
            .contains_key(&InventoryId(2))
    );
    store.remove::<ControlSession>(StableId(ACTOR.0)).unwrap();
    let reading = start_at(
        &runtime,
        &mut store,
        &facts,
        &InvocationAction::ReadRecord {
            record: RecordId(40),
        },
        writing.completes,
    );
    assert!(!runtime.recollection_is_active(&store, ACTOR));
    assert_eq!(
        complete(&runtime, &mut store, &facts, reading.completes, 42),
        MemoryEffect::Read {
            record: RecordId(40),
            actor: ACTOR
        }
    );
    let fact = &stocks(&store, ACTOR).unwrap().known[&InventoryId(2)];
    assert_eq!(fact.source, KnowledgeSource::Recorded(RecordId(40)));
    assert!(!fact.accessible);
    assert_eq!(fact.learned_at, reading.completes);
    assert!(
        runtime.record_views(&store, &facts, ACTOR)[0]
            .content
            .is_some()
    );
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored, store);
}
#[test]
fn completion_rechecks_tools_capacity_control_and_teaching_collateral() {
    let (runtime, mut store, mut facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    facts.tool = false;
    assert_eq!(
        complete(&runtime, &mut store, &facts, writing.completes, 40),
        MemoryEffect::Interrupted { actor: ACTOR }
    );
    assert!(records(&store).next().is_none());
    facts.tool = true;
    let writing = start_at(&runtime, &mut store, &facts, &request(), writing.completes);
    facts.held = 10_000;
    assert_eq!(
        complete(&runtime, &mut store, &facts, writing.completes, 40),
        MemoryEffect::Interrupted { actor: ACTOR }
    );
    facts.held = 0;
    let teaching = start_at(
        &runtime,
        &mut store,
        &facts,
        &InvocationAction::TeachMemory {
            life: LIFE,
            inventory: InventoryId(2),
            recipient: PersonId(11),
        },
        writing.completes,
    );
    facts
        .people
        .get_mut(&PersonId(11))
        .unwrap()
        .teaching_completes = Some(teaching.completes);
    let plan = runtime
        .prepare_memory_completion::<u8>(&store, &facts, ACTOR, teaching.completes, 40)
        .unwrap();
    assert_eq!(plan.participants(), &[ACTOR, PersonId(11)]);
    store.remove::<ControlSession>(StableId(ACTOR.0)).unwrap();
    assert!(
        plan.stage_into::<u8>(
            &runtime,
            &store,
            &facts,
            teaching.completes,
            &mut Batch::new()
        )
        .is_err()
    );
    assert_eq!(
        complete(&runtime, &mut store, &facts, teaching.completes, 40),
        MemoryEffect::Interrupted { actor: ACTOR }
    );
    assert!(stocks(&store, PersonId(11)).unwrap().known.is_empty());
}
#[test]
fn chosen_record_moves_with_cargo_without_relocating_its_sibling() {
    let (runtime, mut store, facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    complete(&runtime, &mut store, &facts, writing.completes, 40);
    let mut second = record(&store, RecordId(40)).unwrap().clone();
    second.id = RecordId(42);
    store.insert_or_spawn(StableId(42), second).unwrap();
    game_cargo::inventory_mut(&mut store, InventoryId(41))
        .unwrap()
        .quantity = Quantity(2000);
    let moving = start_at(
        &runtime,
        &mut store,
        &facts,
        &InvocationAction::MoveRecord {
            record: RecordId(42),
            destination: StockPosition::Area(AreaId(3)),
        },
        writing.completes,
    );
    assert_eq!(
        complete(&runtime, &mut store, &facts, moving.completes, 43),
        MemoryEffect::Transferred {
            actor: ACTOR,
            source: InventoryId(41),
            destination: InventoryId(43),
            quantity: Quantity(1000)
        }
    );
    assert_eq!(
        record(&store, RecordId(40)).unwrap().inventory,
        InventoryId(41)
    );
    assert_eq!(
        record(&store, RecordId(42)).unwrap().inventory,
        InventoryId(43)
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(41))
            .unwrap()
            .quantity,
        Quantity(1000)
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(43))
            .unwrap()
            .quantity,
        Quantity(1000)
    );
    assert!(
        runtime
            .prepare_record_receipt(&store, InventoryId(41), InventoryId(43), Quantity(1000))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        record(&store, RecordId(40)).unwrap().inventory,
        InventoryId(41)
    );
}
#[test]
fn final_foreign_mind_or_stock_edits_reject_before_publishing_readers_or_knowledge() {
    let (runtime, mut store, facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    complete(&runtime, &mut store, &facts, writing.completes, 40);
    let reading = start_at(
        &runtime,
        &mut store,
        &facts,
        &InvocationAction::ReadRecord {
            record: RecordId(40),
        },
        writing.completes,
    );
    for attack in 0..4 {
        let before = store.clone();
        let plan = runtime
            .prepare_memory_completion::<u8>(&store, &facts, ACTOR, reading.completes, 42)
            .unwrap();
        let mut batch = Batch::new();
        let finish = plan
            .stage_into::<u8>(&runtime, &store, &facts, reading.completes, &mut batch)
            .unwrap();
        match attack {
            0 => {
                batch.remove::<Mind<u8>>(StableId(ACTOR.0));
            }
            1 => {
                batch.insert(
                    StableId(ACTOR.0),
                    mind::<u8>(&store, ACTOR).unwrap().clone(),
                );
            }
            2 => {
                batch.remove::<game_characters::decisions::Stocks>(StableId(ACTOR.0));
            }
            _ => {
                batch.insert(StableId(ACTOR.0), stocks(&store, ACTOR).unwrap().clone());
            }
        }
        assert_eq!(
            finish.validate_batch::<u8>(&store, &batch),
            Err(Error::Unavailable)
        );
        assert_eq!(store, before);
        assert!(record(&store, RecordId(40)).unwrap().readers.is_empty());
    }
    let plan = runtime
        .prepare_memory_completion::<u8>(&store, &facts, ACTOR, reading.completes, 42)
        .unwrap();
    let fact = archive::<u8>(&store, LIFE).unwrap().stocks.known[&InventoryId(2)].clone();
    stocks_mut(&mut store, ACTOR)
        .unwrap()
        .known
        .insert(InventoryId(2), fact);
    let before = store.clone();
    assert!(
        plan.stage_into::<u8>(
            &runtime,
            &store,
            &facts,
            reading.completes,
            &mut Batch::new()
        )
        .is_err()
    );
    assert_eq!(store, before);
}

#[test]
fn archives_capture_once_and_only_refresh_skills_in_the_death_tick() {
    struct Facts<'a> {
        store: &'a Store,
        death: SimTime,
        level: u16,
    }
    impl ArchiveFacts<u8> for Facts<'_> {
        fn incarnations(&self) -> impl Iterator<Item = (PersonId, Option<SimTime>)> {
            [(ACTOR, Some(self.death))].into_iter()
        }
        fn mind(&self, person: PersonId) -> Option<&Mind<u8>> {
            mind::<u8>(self.store, person)
        }
        fn stocks(&self, person: PersonId) -> Option<&game_characters::decisions::Stocks> {
            game_characters::decisions::stocks(self.store, person)
        }
        fn skills(&self, _: PersonId) -> Vec<SkillView> {
            vec![SkillView {
                skill: Skill::Craft,
                level: self.level,
                practice_ms: 0,
                progress_ms: 0,
            }]
        }
        fn institutions(&self, _: PersonId) -> Vec<game_society::institutions::InstitutionView> {
            Vec::new()
        }
        fn cases(&self, _: PersonId) -> Vec<game_society::institutions::SocialCaseView> {
            Vec::new()
        }
    }
    let (runtime, mut store, _) = fixture();
    let facts = Facts {
        store: &store,
        death: SimTime(60),
        level: 10,
    };
    let plan = runtime
        .prepare_archives(&store, &facts, SimTime(60))
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(&store, &facts, SimTime(60), &mut batch)
        .unwrap();
    finish.validate_batch(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut store);
    let archived = archive::<u8>(&store, ACTOR).unwrap().mind.clone();
    let archived_stocks = archive::<u8>(&store, ACTOR).unwrap().stocks.clone();
    let fact = archive::<u8>(&store, LIFE).unwrap().stocks.known[&InventoryId(2)].clone();
    stocks_mut(&mut store, ACTOR)
        .unwrap()
        .known
        .insert(fact.inventory, fact);
    mind_mut::<u8>(&mut store, ACTOR).unwrap().dream =
        game_characters::decisions::Aspiration::Comfort;
    let facts = Facts {
        store: &store,
        death: SimTime(60),
        level: 12,
    };
    let plan = runtime
        .prepare_archives(&store, &facts, SimTime(60))
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(&store, &facts, SimTime(60), &mut batch)
        .unwrap();
    finish.validate_batch(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut store);
    assert_eq!(archive::<u8>(&store, ACTOR).unwrap().mind, archived);
    assert_eq!(
        archive::<u8>(&store, ACTOR).unwrap().stocks,
        archived_stocks
    );
    assert_eq!(archive::<u8>(&store, ACTOR).unwrap().skills[0].level, 12);
    let facts = Facts {
        store: &store,
        death: SimTime(60),
        level: 99,
    };
    let plan = runtime
        .prepare_archives(&store, &facts, SimTime(61))
        .unwrap();
    let mut batch = Batch::new();
    let finish = plan
        .stage_into(&store, &facts, SimTime(61), &mut batch)
        .unwrap();
    finish.validate_batch(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut store);
    assert_eq!(archive::<u8>(&store, ACTOR).unwrap().skills[0].level, 12);
}

fn dismiss(runtime: &Runtime, store: &mut Store, now: SimTime) -> Vec<PersonId> {
    let facts = AdmissionFacts {
        actor: ACTOR,
        now,
        current: Some(ACTOR),
        alive: true,
        health: 100000,
        independent: true,
        learning: 1000,
        required_learning: 1,
        fatigue: 0,
        fatigue_limit: u64::MAX,
        archived: BTreeSet::from([LIFE]),
    };
    let control = runtime
        .control
        .prepare_action(store, ACTOR, &ControlAction::Dismiss, &facts, None)
        .unwrap();
    let ending = runtime.prepare_memory_control_reactions(store, &control.effects);
    let mut batch = Batch::new();
    let receipt = control
        .stage_into(&runtime.control, store, &mut batch)
        .unwrap();
    let participants = ending.stage_into(runtime, store, None, &mut batch).unwrap();
    receipt
        .validate_batch(&runtime.control, store, &batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    participants
}
#[test]
fn composed_control_dismissal_ends_writing_but_reading_still_learns() {
    let (runtime, mut store, facts) = fixture();
    start(&runtime, &mut store, &facts, &request());
    assert_eq!(dismiss(&runtime, &mut store, SimTime(1)), vec![ACTOR]);
    assert!(memory_action(&store, ACTOR).is_none());
    assert_eq!(
        game_cargo::inventory(&store, crate::memory_start::tests::MATERIAL)
            .unwrap()
            .quantity,
        Quantity(1000)
    );
    let (runtime, mut store, facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    complete(&runtime, &mut store, &facts, writing.completes, 40);
    let reading = start_at(
        &runtime,
        &mut store,
        &facts,
        &InvocationAction::ReadRecord {
            record: RecordId(40),
        },
        writing.completes,
    );
    assert!(dismiss(&runtime, &mut store, SimTime(reading.completes.0 - 1)).is_empty());
    assert_eq!(memory_action(&store, ACTOR), Some(&reading));
    assert_eq!(
        complete(&runtime, &mut store, &facts, reading.completes, 42),
        MemoryEffect::Read {
            record: RecordId(40),
            actor: ACTOR
        }
    );
    assert!(matches!(
        stocks(&store, ACTOR).unwrap().known[&InventoryId(2)].source,
        KnowledgeSource::Recorded(RecordId(40))
    ));
}
#[test]
fn record_creation_requires_current_carrier_and_transport_capacity() {
    let (runtime, mut store, mut facts) = fixture();
    let writing = start(&runtime, &mut store, &facts, &request());
    facts.creation_capacity = false;
    assert_eq!(
        complete(&runtime, &mut store, &facts, writing.completes, 40),
        MemoryEffect::Interrupted { actor: ACTOR }
    );
    assert!(records(&store).next().is_none());
    assert_eq!(
        game_cargo::inventory(&store, crate::memory_start::tests::MATERIAL)
            .unwrap()
            .quantity,
        Quantity(1000)
    );
}

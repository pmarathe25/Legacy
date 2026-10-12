use super::*;
use game_characters::decisions::{Aspiration, Explanation, Goal, Mind, Personality};
use game_ecs::Schema;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const ACTOR: PersonId = PersonId(10);
pub(crate) const LIFE: PersonId = PersonId(1);
pub(crate) const MATERIAL: InventoryId = InventoryId(20);
pub(crate) const TOOL: InventoryId = InventoryId(21);
#[derive(Clone)]
pub(crate) struct Facts {
    pub(crate) people: BTreeMap<PersonId, MemoryPerson>,
    pub(crate) tool: bool,
    pub(crate) held: u128,
    pub(crate) capacity: u64,
    pub(crate) creation_capacity: bool,
}
impl MemoryFacts for Facts {
    fn person(&self, id: PersonId, _: PersonId) -> Option<MemoryPerson> {
        self.people.get(&id).cloned()
    }
    fn stock_area(&self, _: InventoryId) -> Option<AreaId> {
        Some(AreaId(3))
    }
    fn has_access(&self, _: PersonId, _: InventoryId) -> bool {
        true
    }
    fn tool_available(&self, _: PersonId, _: InventoryId) -> bool {
        self.tool
    }
    fn unreserved(&self, _: PersonId, _: InventoryId) -> u64 {
        2000
    }
    fn carried_quantity(&self, _: PersonId) -> u128 {
        self.held
    }
    fn carrying_capacity(&self, _: PersonId) -> u64 {
        self.capacity
    }
    fn creation_body_capacity(&self, _: PersonId) -> u64 {
        self.capacity
    }
    fn creation_transport_capacity(&self, _: PersonId, _: Quantity) -> bool {
        self.creation_capacity
    }
}
pub(crate) fn fixture() -> (Runtime, Store, Facts) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    register_memory_components::<u8>(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
    schema
        .register::<u64>("test/rejected", |value| {
            if *value > 10 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    let fact = StockKnowledge {
        inventory: InventoryId(2),
        area: AreaId(3),
        good: Good::Food,
        quantity: Quantity(1000),
        accessible: true,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(LIFE),
    };
    let mind = Mind::<u8> {
        interests: BTreeMap::new(),
        routes: BTreeMap::from([(AreaId(3), BTreeMap::from([(AreaId(4), SimDuration(1000))]))]),
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality: Personality::industrious(),
        dream: Aspiration::Independence,
        knowledge: BTreeMap::from([(InventoryId(2), fact)]),
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
    };
    store
        .insert_or_spawn(
            StableId(LIFE.0),
            PastLife {
                person: LIFE,
                died_at: SimTime(0),
                mind,
                skills: Vec::new(),
                institutions: Vec::new(),
                cases: Vec::new(),
            },
        )
        .unwrap();
    let mut current = archive::<u8>(&store, LIFE).unwrap().mind.clone();
    current.knowledge.clear();
    current.routes.clear();
    game_characters::decisions::insert_mind(&mut store, ACTOR, current.clone()).unwrap();
    game_characters::decisions::insert_mind(&mut store, PersonId(11), current).unwrap();
    for (id, good, amount) in [
        (MATERIAL, Good::Timber, 2000),
        (TOOL, Good::Tool, 1000),
        (InventoryId(2), Good::Food, 1000),
    ] {
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id,
                good,
                quantity: Quantity(amount),
                capacity: Quantity(5000),
                location: game_cargo::InventoryLocation::Area(AreaId(3)),
                access: BTreeSet::from([ACTOR]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    let mut control = ControlState::new(Rules::fixture()).unwrap();
    control.active = Some(Active {
        actor: ACTOR,
        lives: BTreeSet::from([LIFE]),
        started: SimTime(0),
        accepted: BTreeMap::new(),
        episode: None,
        next_loss_burden: 300000,
        next_controller: SimTime(0),
        dominant: None,
    });
    let person = MemoryPerson {
        location: Location::AtArea(AreaId(3)),
        independent: true,
        in_combat: false,
        restricted: false,
        alive: true,
        health: 100000,
        idle_or_resting: true,
        teaching_completes: None,
        sociability: 150,
        trust: 50,
        hunger: 0,
        meal_relief: 120000,
    };
    let facts = Facts {
        people: BTreeMap::from([(ACTOR, person.clone()), (PersonId(11), person)]),
        tool: true,
        held: 0,
        capacity: 10000,
        creation_capacity: true,
    };
    (Runtime { control }, store, facts)
}
pub(crate) fn request() -> InvocationAction {
    InvocationAction::RecordMemory {
        life: LIFE,
        inventory: InventoryId(2),
        material: MATERIAL,
        tool: TOOL,
    }
}
#[test]
fn writing_stages_real_material_consumption_and_native_work_in_one_batch() {
    let (runtime, mut store, facts) = fixture();
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(30))
        .unwrap();
    assert_eq!(plan.action().completes, SimTime(60030));
    let mut batch = Batch::new();
    let action = plan
        .stage_into::<u8>(&runtime, &store, &facts, SimTime(30), &mut batch)
        .unwrap();
    assert_eq!(
        game_cargo::inventory(&store, MATERIAL).unwrap().quantity,
        Quantity(2000)
    );
    store.apply_batch(batch).unwrap();
    assert_eq!(
        game_cargo::inventory(&store, MATERIAL).unwrap().quantity,
        Quantity(1000)
    );
    assert_eq!(memory_action(&store, ACTOR), Some(&action));
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored, store);
}
#[test]
fn rejected_combined_work_and_stale_physical_admission_preserve_materials() {
    let (runtime, mut store, mut facts) = fixture();
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(0))
        .unwrap();
    let before = store.clone();
    let mut batch = Batch::new();
    plan.stage_into::<u8>(&runtime, &store, &facts, SimTime(0), &mut batch)
        .unwrap();
    batch.insert(StableId(30), 11_u64);
    assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
    assert_eq!(store, before);
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(0))
        .unwrap();
    facts.tool = false;
    assert!(
        plan.stage_into::<u8>(&runtime, &store, &facts, SimTime(0), &mut Batch::new())
            .is_err()
    );
    assert_eq!(store, before);
    facts.tool = true;
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(0))
        .unwrap();
    assert!(
        plan.stage_into::<u8>(&runtime, &store, &facts, SimTime(1), &mut Batch::new())
            .is_err()
    );
    assert_eq!(store, before);
}
#[test]
fn teaching_uses_recipient_judgment_and_current_physical_availability() {
    let (runtime, store, mut facts) = fixture();
    let request = InvocationAction::TeachMemory {
        life: LIFE,
        inventory: InventoryId(2),
        recipient: PersonId(11),
    };
    assert!(
        runtime
            .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request, SimTime(0))
            .is_ok()
    );
    facts.people.get_mut(&PersonId(11)).unwrap().trust = 49;
    assert!(
        runtime
            .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request, SimTime(0))
            .is_err()
    );
    facts.people.get_mut(&PersonId(11)).unwrap().trust = 50;
    facts.people.get_mut(&PersonId(11)).unwrap().hunger = 120000;
    assert!(
        runtime
            .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request, SimTime(0))
            .is_err()
    );
    facts.people.get_mut(&PersonId(11)).unwrap().hunger = 0;
    facts.people.get_mut(&PersonId(11)).unwrap().idle_or_resting = false;
    assert!(
        runtime
            .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request, SimTime(0))
            .is_err()
    );
}

#[test]
fn ending_work_coalesces_shared_participants_and_rejects_new_membership() {
    let (runtime, mut store, facts) = fixture();
    let request = InvocationAction::TeachMemory {
        life: LIFE,
        inventory: InventoryId(2),
        recipient: PersonId(11),
    };
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request, SimTime(0))
        .unwrap();
    let mut batch = Batch::new();
    plan.stage_into::<u8>(&runtime, &store, &facts, SimTime(0), &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    let ending = runtime.prepare_memory_ending(&store, &[PersonId(11), ACTOR]);
    assert_eq!(ending.participants(), &[ACTOR, PersonId(11)]);
    let mut other = memory_action(&store, ACTOR).unwrap().clone();
    other.actor = PersonId(12);
    store.insert_or_spawn(StableId(12), other).unwrap();
    let before = store.clone();
    assert!(
        ending
            .stage_into(&runtime, &store, None, &mut Batch::new())
            .is_err()
    );
    assert_eq!(store, before);
    let ending = runtime.prepare_memory_ending(&store, &[PersonId(11)]);
    let mut batch = Batch::new();
    ending
        .stage_into(&runtime, &store, None, &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    assert!(memory_actions(&store).next().is_none());
}

#[test]
fn memory_validation_checks_native_keys_foreign_references_and_activity_reciprocity() {
    struct References<'a>(&'a Store, &'a Facts);
    impl MemoryReferences for References<'_> {
        fn person(&self, id: PersonId) -> Option<MemoryValidationPerson> {
            if id == LIFE {
                return Some(MemoryValidationPerson {
                    alive: false,
                    activity: MemoryActivity::Other,
                });
            }
            let person = self.1.people.get(&id)?;
            let activity = memory_action(self.0, id).map_or(MemoryActivity::Other, |action| {
                match action.work {
                    MemoryWork::Write { .. } => MemoryActivity::Write(action.completes),
                    MemoryWork::Read { .. } => MemoryActivity::Read(action.completes),
                    MemoryWork::Move { .. } => MemoryActivity::Move(action.completes),
                    MemoryWork::Teach { .. } => MemoryActivity::Teach(action.completes),
                }
            });
            Some(MemoryValidationPerson {
                alive: person.alive,
                activity,
            })
        }
        fn death(&self, id: PersonId) -> Option<SimTime> {
            (id == LIFE).then_some(SimTime(0))
        }
        fn area_exists(&self, id: AreaId) -> bool {
            [AreaId(3), AreaId(4)].contains(&id)
        }
        fn memory_activities(&self) -> impl Iterator<Item = (PersonId, MemoryActivity)> {
            self.1
                .people
                .keys()
                .copied()
                .filter_map(|id| self.person(id).map(|person| (id, person.activity)))
        }
    }
    let (mut runtime, mut store, facts) = fixture();
    runtime
        .control
        .active
        .as_mut()
        .unwrap()
        .accepted
        .insert(InvocationCost::Strain, Consent::default());
    insert_burden(&mut store, ACTOR, Burden::default()).unwrap();
    game_cargo::insert_inventory(
        &mut store,
        game_cargo::Inventory {
            id: InventoryId(2),
            location: game_cargo::InventoryLocation::Area(AreaId(3)),
            quantity: Quantity(1000),
            capacity: Quantity(5000),
            access: BTreeSet::from([ACTOR]),
            last_granted: None,
            good: Good::Food,
            spoils_at: None,
        },
    )
    .unwrap();
    let incarnations = BTreeSet::from([LIFE, ACTOR]);
    let check = |store: &Store| {
        runtime.validate_memory::<u8>(
            store,
            &References(store, &facts),
            MemoryValidation {
                now: SimTime(0),
                next_id: 100,
                current: Some(ACTOR),
                incarnations: &incarnations,
            },
        )
    };
    assert!(check(&store).is_ok());
    assert!(validate_absent_memory::<u8>(&store).is_err());
    let plan = runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(0))
        .unwrap();
    let mut batch = Batch::new();
    plan.stage_into::<u8>(&runtime, &store, &facts, SimTime(0), &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    assert!(check(&store).is_ok());
    archive_mut::<u8>(&mut store, LIFE).unwrap().person = ACTOR;
    assert!(check(&store).is_err());
    archive_mut::<u8>(&mut store, LIFE).unwrap().person = LIFE;
    game_cargo::inventory_mut(&mut store, InventoryId(2))
        .unwrap()
        .good = Good::Water;
    assert!(check(&store).is_err());
}

#[test]
fn typed_staged_replacement_preserves_native_work_and_rejects_foreign_removal() {
    let (runtime, mut store, facts) = fixture();
    let teaching = InvocationAction::TeachMemory {
        life: LIFE,
        inventory: InventoryId(2),
        recipient: PersonId(11),
    };
    let mut batch = Batch::new();
    runtime
        .prepare_memory_start::<u8>(&store, &facts, ACTOR, &teaching, SimTime(0))
        .unwrap()
        .stage_into::<u8>(&runtime, &store, &facts, SimTime(0), &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    for foreign_removal in [false, true] {
        let before = store.clone();
        let ending = runtime.prepare_memory_ending(&store, &[ACTOR]);
        let start = runtime
            .prepare_memory_start::<u8>(&store, &facts, ACTOR, &request(), SimTime(1))
            .unwrap();
        let mut batch = Batch::new();
        let (action, replacement) = start
            .stage_into_with_replacement::<u8>(&runtime, &store, &facts, SimTime(1), &mut batch)
            .unwrap();
        let participants = ending
            .stage_into_composed(&runtime, &store, Some(&replacement), &mut batch)
            .unwrap();
        assert_eq!(participants, vec![ACTOR, PersonId(11)]);
        if foreign_removal {
            batch.remove::<MemoryAction>(StableId(ACTOR.0));
            assert!(store.validate_batch(&batch).is_err());
            assert_eq!(store, before);
        } else {
            store.apply_batch(batch).unwrap();
            assert_eq!(memory_action(&store, ACTOR), Some(&action));
            store = before;
        }
    }
}

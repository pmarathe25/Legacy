use super::*;
use game_cargo::{Inventory, InventoryLocation};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Input {
    people: BTreeMap<PersonId, PersonFacts>,
    custody: BTreeMap<PersonId, PersonId>,
    equipment: BTreeMap<PersonId, Vec<InventoryId>>,
    wills: BTreeMap<PersonId, Willingness>,
    fights: BTreeSet<(PersonId, PersonId)>,
}
impl Facts for Input {
    fn person(&self, actor: PersonId) -> Option<PersonFacts> {
        self.people.get(&actor).cloned()
    }
    fn same_place(&self, actor: PersonId, target: PersonId) -> bool {
        self.people
            .get(&actor)
            .zip(self.people.get(&target))
            .is_some_and(|(a, b)| a.location == b.location)
    }
    fn combat_available(&self) -> bool {
        true
    }
    fn incapacitation(&self) -> u64 {
        10
    }
    fn ruler(&self) -> PersonId {
        PersonId(1)
    }
    fn captor(&self, actor: PersonId) -> Option<PersonId> {
        self.custody.get(&actor).copied()
    }
    fn same_encounter(&self, target: PersonId, actor: PersonId) -> bool {
        self.fights.contains(&(target, actor))
    }
    fn carried(&self, actor: PersonId) -> Vec<InventoryId> {
        self.equipment.get(&actor).cloned().unwrap_or_default()
    }
    fn people(&self) -> impl Iterator<Item = PersonId> {
        self.people.keys().copied()
    }
    fn willingness(&self, actor: PersonId, _: PersonId) -> Option<Willingness> {
        self.wills.get(&actor).copied()
    }
    fn positions(&self, _: PersonId, _: AreaId) -> Vec<Point> {
        vec![]
    }
}
fn fixture() -> (Runtime, Store, Input) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    crate::register_components(&mut schema).unwrap();
    schema
        .register::<Location>("test/location", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema);
    let location = Location::AtArea(AreaId(5));
    let person = PersonFacts {
        alive: true,
        health: 100,
        independent: true,
        in_combat: false,
        restricted: false,
        idle_or_resting: true,
        raw_location: location.clone(),
        location: location.clone(),
    };
    let people = (1..=4)
        .map(|id| (PersonId(id), person.clone()))
        .collect::<BTreeMap<_, _>>();
    for actor in people.keys() {
        store
            .insert_or_spawn(StableId(actor.0), location.clone())
            .unwrap();
    }
    for (id, actor, good) in [(10, 2, Good::Tool), (11, 1, Good::Binding)] {
        game_cargo::insert_inventory(
            &mut store,
            Inventory {
                id: InventoryId(id),
                location: InventoryLocation::Carried(PersonId(actor)),
                quantity: Quantity(2000),
                capacity: Quantity(2000),
                access: BTreeSet::new(),
                last_granted: None,
                good,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    let input = Input {
        people,
        custody: BTreeMap::new(),
        equipment: BTreeMap::from([
            (PersonId(1), vec![InventoryId(11)]),
            (PersonId(2), vec![InventoryId(10)]),
        ]),
        wills: BTreeMap::new(),
        fights: BTreeSet::new(),
    };
    (Runtime::new(0), store, input)
}
#[test]
fn fresh_yield_and_contact_admit_real_binding_work_and_event_reactions_revoke_it() {
    let (runtime, mut store, facts) = fixture();
    let captor = PersonId(1);
    let target = PersonId(2);
    assert!(
        runtime
            .prepare_restraint(&store, &facts, captor, target, SimTime(0))
            .is_err()
    );
    runtime
        .prepare_yield(&store, &facts, target, captor, SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let plan = runtime
        .prepare_restraint(&store, &facts, captor, target, SimTime(0))
        .unwrap();
    let work = plan.work().cloned().unwrap();
    plan.commit(&mut store).unwrap();
    assert!(
        matches!(work,Work::Restrain {row,..} if row.binding==InventoryId(11) && row.completes==SimTime(6000))
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(11))
            .unwrap()
            .quantity,
        Quantity(2000)
    );
    runtime
        .prepare_update(&store, &facts, [ObservedEvent::Departed { actor: captor }])
        .commit(&mut store)
        .unwrap();
    assert!(yielded(&store, target).is_none());
    assert!(!restrainable(&store, &facts, captor, target));
    prepare_interrupt(&store, captor)
        .commit(&mut store)
        .unwrap();
    assert!(restraint(&store, captor).is_none());
}
#[test]
fn save_at_partial_escape_and_equivalent_slices_preserve_attempt_and_tools() {
    let (runtime, mut whole, mut facts) = fixture();
    let target = PersonId(2);
    facts.custody.insert(target, PersonId(1));
    runtime
        .prepare_escape(&whole, &facts, target, SimTime(0))
        .unwrap()
        .commit(&mut whole)
        .unwrap();
    assert!(
        runtime
            .completion(&whole, &facts, target, SimTime(1733))
            .is_none()
    );
    let mut resumed = Store::restore(whole.schema().clone(), whole.snapshot().unwrap()).unwrap();
    for store in [&mut whole, &mut resumed] {
        for now in [SimTime(2000), SimTime(4000), SimTime(5999)] {
            assert!(runtime.completion(store, &facts, target, now).is_none());
        }
        let Completion::Escape {
            successful,
            removal,
        } = runtime
            .completion(store, &facts, target, SimTime(6000))
            .unwrap()
        else {
            panic!("escape")
        };
        assert!(successful);
        removal.commit(store).unwrap();
        assert!(escape(store, target).is_none());
        assert_eq!(
            game_cargo::inventory(store, InventoryId(10))
                .unwrap()
                .quantity,
            Quantity(2000)
        );
    }
    assert_eq!(whole, resumed);
}
#[test]
fn owner_projections_reject_foreign_equipment_or_work_before_state_and_publication() {
    let (runtime, store, mut facts) = fixture();
    facts.custody.insert(PersonId(2), PersonId(1));
    for attack in 0..3 {
        let plan = runtime
            .prepare_escape(&store, &facts, PersonId(2), SimTime(0))
            .unwrap();
        let before = store.clone();
        let mut batch = Batch::new();
        let receipt = plan.stage_into(&store, &mut batch).unwrap();
        match attack {
            0 => {
                batch.remove::<Inventory>(StableId(10));
            }
            1 => {
                batch.remove::<Escape>(StableId(2));
            }
            _ => {
                batch.insert(StableId(2), Location::AtArea(AreaId(6)));
            }
        }
        let mut next_id = 20;
        let mut published = vec![];
        let result = (|| -> Result<(), Error> {
            receipt.validate_batch(&store, &batch)?;
            let mut candidate = store.clone();
            candidate.apply_batch(batch)?;
            next_id += 1;
            published.extend(receipt.effects);
            Ok(())
        })();
        assert!(result.is_err());
        assert_eq!(store, before);
        assert_eq!(next_id, 20);
        assert!(published.is_empty());
    }
}
#[test]
fn actual_npc_willingness_selects_bounded_initiative_opponents_and_preserves_contested_work() {
    let (runtime, mut store, mut facts) = fixture();
    let target = PersonId(2);
    let captor = PersonId(3);
    facts.custody.insert(target, captor);
    runtime
        .prepare_escape(&store, &facts, target, SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert!(
        runtime
            .prepare_intervention(&store, &facts, target)
            .unwrap()
            .is_none()
    );
    facts.wills.insert(
        captor,
        Willingness {
            aggression: 500,
            risk_tolerance: 500,
            trust: 0,
        },
    );
    facts.wills.insert(
        PersonId(4),
        Willingness {
            aggression: 700,
            risk_tolerance: 700,
            trust: 500,
        },
    );
    let (plan, intervention) = runtime
        .prepare_intervention(&store, &facts, target)
        .unwrap()
        .unwrap();
    assert_eq!(
        intervention.participants,
        vec![(target, 0), (captor, 1), (PersonId(4), 1)]
    );
    plan.commit(&mut store).unwrap();
    prepare_interrupt(&store, target)
        .commit(&mut store)
        .unwrap();
    assert!(escape(&store, target).unwrap().contested);
    facts.fights.insert((target, target));
    runtime
        .prepare_update(&store, &facts, [])
        .commit(&mut store)
        .unwrap();
    assert!(escape(&store, target).is_some());
    facts.fights.clear();
    runtime
        .prepare_update(&store, &facts, [])
        .commit(&mut store)
        .unwrap();
    assert!(escape(&store, target).is_none());
}

#[test]
fn completed_restraint_composes_with_confinement_owner_and_conserves_physical_rope() {
    let (runtime, mut store, facts) = fixture();
    let captor = PersonId(1);
    let target = PersonId(2);
    runtime
        .prepare_yield(&store, &facts, target, captor, SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    runtime
        .prepare_restraint(&store, &facts, captor, target, SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let Completion::Restrain { row, removal } = runtime
        .completion(&store, &facts, captor, SimTime(6000))
        .unwrap()
    else {
        panic!("restraint")
    };
    let actual = crate::ConfinementFacts {
        captor,
        target,
        captor_alive: true,
        captor_health: 100,
        independent: true,
        captor_confined: false,
        target_alive: true,
        target_health: 100,
        target_confined: false,
        surrendered_to_captor: surrendered(&store, &facts, captor, target),
        contact: true,
        admitted_site_contact: true,
        captor_location: row.location.clone(),
        target_location: row.location.clone(),
    };
    let rules = crate::Rules {
        encounter_limit: 128,
        round: SimDuration(6000),
        movement: 5000,
        reach: 2000,
        damage: 50,
        defence: 30,
        incapacitation: 10,
    };
    let plan = crate::prepare_confinement(
        &rules,
        actual.clone(),
        game_cargo::inventory(&store, row.binding).unwrap(),
        InventoryId(12),
        crate::ConfinementContext {
            map: CombatMapId(40),
            area: row.area,
            location: row.location.clone(),
            target_position: row.position,
        },
        crate::ConfinementMode::Timed {
            completes: row.completes,
            admitted_location: row.location,
        },
        SimTime(6000),
    )
    .unwrap();
    removal.commit(&mut store).unwrap();
    let receipt = plan.apply(&mut store, &actual).unwrap();
    assert_eq!(receipt.binding, InventoryId(12));
    assert_eq!(
        crate::captivity(&store, target).unwrap().binding,
        InventoryId(12)
    );
    assert_eq!(
        game_cargo::inventories(&store)
            .filter(|(_, row)| row.good == Good::Binding)
            .map(|(_, row)| row.quantity.0)
            .sum::<u64>(),
        2000
    );
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(12))
            .unwrap()
            .location,
        InventoryLocation::Carried(target)
    );
}

#[test]
fn invalid_owner_duration_rejects_before_work_or_tool_publication() {
    let (mut runtime, mut store, mut facts) = fixture();
    facts.custody.insert(PersonId(2), PersonId(1));
    runtime.rules.escape_time = SimDuration(0);
    let before = store.clone();
    assert!(
        runtime
            .prepare_escape(&store, &facts, PersonId(2), SimTime(0))
            .is_err()
    );
    assert_eq!(store, before);
    runtime.rules.escape_time = SimDuration(6000);
    runtime
        .prepare_yield(&store, &facts, PersonId(4), PersonId(1), SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    runtime.rules.restraint_time = SimDuration(0);
    let before = store.clone();
    assert!(
        runtime
            .prepare_restraint(&store, &facts, PersonId(1), PersonId(4), SimTime(0))
            .is_err()
    );
    assert_eq!(store, before);
}

#[test]
fn scene_schema_rejection_preserves_combined_force_rope_custody_and_host_cursors() {
    let (runtime, _, mut facts) = fixture();
    for person in facts.people.values_mut() {
        person.location = Location::AtArea(AreaId(3));
        person.raw_location = person.location.clone();
    }
    facts.equipment.insert(PersonId(1), vec![InventoryId(4)]);
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    schema.register::<u64>("test/entity", |_| Ok(())).unwrap();
    schema
        .register::<crate::Captivity>("conflict/captivity", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::MapRef>("conflict/map_ref", |_| Ok(()))
        .unwrap();
    schema
        .register::<crate::Map>("conflict/map", |row| {
            row.validate()?;
            if row.id == CombatMapId(5) {
                return Err(Error::Unavailable);
            }
            Ok(())
        })
        .unwrap();
    let mut host = game_world::GenericHost::<(), u8>::new(schema, 1, ());
    for expected in 1..=4 {
        let id = host.allocate_id().unwrap();
        assert_eq!(id, StableId(expected));
        host.storage.insert_or_spawn(id, 0_u64).unwrap();
    }
    game_cargo::insert_inventory(
        &mut host.storage,
        Inventory {
            id: InventoryId(4),
            location: InventoryLocation::Carried(PersonId(1)),
            quantity: Quantity(2000),
            capacity: Quantity(2000),
            access: BTreeSet::new(),
            last_granted: None,
            good: Good::Binding,
            spoils_at: None,
        },
    )
    .unwrap();
    runtime
        .prepare_yield(&host.storage, &facts, PersonId(2), PersonId(1), SimTime(0))
        .unwrap()
        .commit(&mut host.storage)
        .unwrap();
    runtime
        .prepare_restraint(&host.storage, &facts, PersonId(1), PersonId(2), SimTime(0))
        .unwrap()
        .commit(&mut host.storage)
        .unwrap();
    host.parts_mut().clock.time = SimTime(6000);
    let before = host.snapshot().unwrap();
    let opening = host.transaction_opening();
    let Completion::Restrain { row, removal } = runtime
        .completion(&host.storage, &facts, PersonId(1), SimTime(6000))
        .unwrap()
    else {
        panic!("restraint")
    };
    let actual = crate::ConfinementFacts {
        captor: PersonId(1),
        target: PersonId(2),
        captor_alive: true,
        captor_health: 100,
        independent: true,
        captor_confined: false,
        target_alive: true,
        target_health: 100,
        target_confined: false,
        surrendered_to_captor: true,
        contact: true,
        admitted_site_contact: true,
        captor_location: row.location.clone(),
        target_location: row.location.clone(),
    };
    let rules = crate::Rules {
        encounter_limit: 128,
        round: SimDuration(6000),
        movement: 5000,
        reach: 2000,
        damage: 50,
        defence: 30,
        incapacitation: 10,
    };
    let plan = crate::prepare_confinement(
        &rules,
        actual.clone(),
        game_cargo::inventory(&host.storage, InventoryId(4)).unwrap(),
        InventoryId(6),
        crate::ConfinementContext {
            map: CombatMapId(5),
            area: row.area,
            location: row.location.clone(),
            target_position: row.position,
        },
        crate::ConfinementMode::Timed {
            completes: row.completes,
            admitted_location: row.location.clone(),
        },
        SimTime(6000),
    )
    .unwrap()
    .begin(&host.storage, &actual)
    .unwrap();
    let scene = crate::PreparedMap::New(crate::Map {
        id: CombatMapId(5),
        version: 1,
        area: row.area,
        location: row.location,
        width: 24000,
        height: 16000,
        obstacles: vec![],
        exits: vec![],
    });
    let result = (|| -> Result<(), Error> {
        let mut batch = Batch::new();
        let scene_guard = scene.stage_into(&host.storage, &mut batch)?;
        let custody_guard = plan.stage_into(&host.storage, &mut batch)?;
        let force_guard = removal.stage_into(&host.storage, &mut batch)?;
        custody_guard.validate(&host.storage, &batch)?;
        force_guard.validate_batch(&host.storage, &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = 2;
        transaction.events = vec![1];
        transaction.guards.push(scene_guard);
        host.commit_transaction(transaction)
    })();
    assert!(result.is_err());
    assert_eq!(host.snapshot().unwrap(), before);
    assert_eq!(host.next_id(), 5);
    assert!(host.events().is_empty());
    assert_eq!(
        game_cargo::inventory(&host.storage, InventoryId(4))
            .unwrap()
            .quantity,
        Quantity(2000)
    );
    assert!(crate::captivity(&host.storage, PersonId(2)).is_none());
    assert!(restraint(&host.storage, PersonId(1)).is_some());
}

#[test]
fn pending_escape_prepares_the_intervention_without_publishing_timed_work() {
    let (runtime, mut store, mut facts) = fixture();
    facts.custody.insert(PersonId(2), PersonId(3));
    facts.wills.insert(
        PersonId(3),
        Willingness {
            aggression: 1000,
            risk_tolerance: 1000,
            trust: 1000,
        },
    );
    let before = store.snapshot().unwrap();
    let (plan, intervention) = runtime
        .prepare_escape_start(&store, &facts, PersonId(2), SimTime(0))
        .unwrap();
    assert_eq!(
        intervention.unwrap().participants,
        vec![(PersonId(2), 0), (PersonId(3), 1)]
    );
    assert!(plan.scheduled_work().is_none());
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(escape(&store, PersonId(2)).is_none());
    let mut batch = Batch::new();
    let receipt = plan.stage_into(&store, &mut batch).unwrap();
    receipt.validate_batch(&store, &batch).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    store.apply_batch(batch).unwrap();
    assert!(escape(&store, PersonId(2)).unwrap().contested);
}
#[test]
fn release_cleanup_stages_captivity_and_force_attempts_and_rejects_foreign_replacement() {
    let (runtime, mut store, mut facts) = fixture();
    facts.custody.insert(PersonId(2), PersonId(1));
    let captured = crate::Captivity {
        actor: PersonId(2),
        captor: PersonId(1),
        area: AreaId(5),
        position: Point { x: 0, y: 0 },
        map: CombatMapId(20),
        binding: InventoryId(11),
    };
    crate::insert_captivity(&mut store, PersonId(2), captured.clone()).unwrap();
    runtime
        .prepare_escape(&store, &facts, PersonId(2), SimTime(0))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    let before = store.snapshot().unwrap();
    let plan = prepare_release_cleanup(&store, PersonId(2)).unwrap();
    let mut batch = Batch::new();
    let receipt = plan.stage_into(&store, &mut batch).unwrap();
    assert_eq!(
        receipt.released_targets().collect::<Vec<_>>(),
        vec![PersonId(2)]
    );
    receipt.validate_batch(&store, &batch).unwrap();
    batch.insert(StableId(2), captured);
    assert!(receipt.validate_batch(&store, &batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    prepare_release_cleanup(&store, PersonId(2))
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert!(crate::captivity(&store, PersonId(2)).is_none());
    assert!(escape(&store, PersonId(2)).is_none());
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(11))
            .unwrap()
            .quantity,
        Quantity(2000)
    );
}

#[test]
fn a_willing_moving_captor_is_selected_before_the_connected_stop_is_published() {
    let (runtime, mut store, mut facts) = fixture();
    let target = PersonId(2);
    let captor = PersonId(3);
    facts.custody.insert(target, captor);
    facts.wills.insert(
        captor,
        Willingness {
            aggression: 1000,
            risk_tolerance: 1000,
            trust: 0,
        },
    );
    let transit = Location::InTransit {
        from: AreaId(5),
        to: AreaId(6),
        departed: SimTime(0),
        arrives: SimTime(120_000),
        road_start: None,
    };
    let captive = facts.people.get_mut(&target).unwrap();
    captive.raw_location = Location::Carried { carrier: captor };
    captive.location = transit.clone();
    let bearer = facts.people.get_mut(&captor).unwrap();
    bearer.raw_location = transit.clone();
    bearer.location = transit.clone();
    store
        .insert(StableId(target.0), Location::Carried { carrier: captor })
        .unwrap();
    store.insert(StableId(captor.0), transit).unwrap();
    let opening = store.snapshot().unwrap();
    let (plan, intervention) = runtime
        .prepare_escape_start(&store, &facts, target, SimTime(30_000))
        .unwrap();
    assert_eq!(
        intervention.unwrap().participants,
        vec![(target, 0), (captor, 1)]
    );
    let Some(Work::Escape { row, .. }) = plan.work() else {
        panic!("escape work required")
    };
    assert!(row.contested);
    assert_eq!(store.snapshot().unwrap(), opening);
}

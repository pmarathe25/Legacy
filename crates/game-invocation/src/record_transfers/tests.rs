use super::*;
use game_ecs::Schema;
use std::collections::BTreeSet;
fn fixture() -> (Runtime, Store) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    register_memory_components::<u8>(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for (id, amount) in [(InventoryId(20), 2000), (InventoryId(30), 0)] {
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id,
                location: game_cargo::InventoryLocation::Area(AreaId(3)),
                quantity: Quantity(amount),
                capacity: Quantity(5000),
                access: BTreeSet::from([PersonId(10)]),
                last_granted: None,
                good: Good::Record,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    for id in [RecordId(41), RecordId(42)] {
        store
            .insert_or_spawn(
                StableId(id.0),
                Record {
                    id,
                    inventory: InventoryId(20),
                    content: RecordedMemory::Route {
                        life: PersonId(1),
                        origin: AreaId(3),
                        destination: AreaId(4),
                        duration: SimDuration(1000),
                    },
                    author: PersonId(10),
                    written_at: SimTime(0),
                    readers: BTreeSet::new(),
                    intact: true,
                },
            )
            .unwrap();
    }
    (
        Runtime {
            control: ControlState::new(Rules::fixture()).unwrap(),
        },
        store,
    )
}
#[test]
fn selected_record_and_actual_cargo_move_once_in_a_shared_batch() {
    let (runtime, mut store) = fixture();
    let source = InventoryId(20);
    let destination = InventoryId(30);
    let quantity = Quantity(1000);
    let records = runtime
        .prepare_record_transfer(&store, source, destination, quantity, Some(RecordId(42)))
        .unwrap();
    let cargo = game_cargo::prepare_transfers(
        &store,
        None,
        vec![game_cargo::TransferRequest {
            intent: game_cargo::TransferIntent {
                actor: PersonId(10),
                source,
                destination,
                quantity,
            },
            facts: game_cargo::ClosingFacts {
                alive: true,
                health: 1000,
                source_contact: true,
                source_access: true,
                receiver_contact: true,
                receiver_access: true,
                receiver_accepted: false,
                record_permission: true,
            },
        }],
        None,
        |_| true,
    )
    .unwrap();
    let mut batch = Batch::new();
    let mut phase = cargo.begin(&store).unwrap();
    let receipt = phase.stage_next_into(&store, &mut batch).unwrap().unwrap();
    assert!(receipt.1);
    assert_eq!(receipt.0.source, source);
    records.stage_into(&store, &mut batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(
        phase
            .stage_next_into(&store, &mut Batch::new())
            .unwrap()
            .is_none()
    );
    assert_eq!(record(&store, RecordId(41)).unwrap().inventory, source);
    assert_eq!(record(&store, RecordId(42)).unwrap().inventory, destination);
    assert_eq!(
        game_cargo::inventory(&store, source).unwrap().quantity,
        Quantity(1000)
    );
    assert_eq!(
        game_cargo::inventory(&store, destination).unwrap().quantity,
        Quantity(1000)
    );
    let before = store.clone();
    runtime
        .prepare_record_receipt(&store, source, destination, quantity)
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert_eq!(store, before);
}
#[test]
fn partial_units_or_changed_source_record_membership_are_rejected() {
    let (runtime, mut store) = fixture();
    assert!(!runtime.record_transfer_allowed(&store, InventoryId(20), Quantity(999)));
    assert!(
        runtime
            .prepare_record_transfer(
                &store,
                InventoryId(20),
                InventoryId(30),
                Quantity(999),
                None
            )
            .is_err()
    );
    let plan = runtime
        .prepare_record_transfer(
            &store,
            InventoryId(20),
            InventoryId(30),
            Quantity(1000),
            None,
        )
        .unwrap();
    record_mut(&mut store, RecordId(42)).unwrap().intact = false;
    let before = store.clone();
    assert_eq!(plan.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store, before);
}

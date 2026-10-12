use game_core::*;
use game_ecs::{Batch, Schema, StableId, Store};
use game_invocation::{ControlState, Record, RecordedMemory, Rules, Runtime};
use std::{collections::BTreeSet, time::Instant};
fn main() {
    let n = std::env::args()
        .nth(1)
        .map_or(20000, |s| s.parse::<u64>().unwrap());
    let unrelated = std::env::args()
        .nth(2)
        .map_or(1000, |s| s.parse::<u64>().unwrap());
    for divisor in [1, 2] {
        let quantity = n / divisor;
        let mut schema = Schema::new();
        game_cargo::register_components(&mut schema).unwrap();
        game_invocation::register_memory_components::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for (id, good, amount, cap) in [
            (InventoryId(1), Good::Water, quantity, n),
            (InventoryId(2), Good::Water, 0, n),
            (
                InventoryId(3),
                Good::Record,
                unrelated * 1000,
                unrelated * 1000,
            ),
        ] {
            game_cargo::insert_inventory(
                &mut store,
                game_cargo::Inventory {
                    id,
                    location: game_cargo::InventoryLocation::Area(AreaId(1)),
                    quantity: Quantity(amount),
                    capacity: Quantity(cap),
                    access: BTreeSet::new(),
                    last_granted: None,
                    good,
                    spoils_at: None,
                },
            )
            .unwrap();
        }
        for id in 100..100 + unrelated {
            store
                .insert_or_spawn(
                    StableId(id),
                    Record {
                        id: RecordId(id),
                        inventory: InventoryId(3),
                        content: RecordedMemory::Route {
                            life: PersonId(10),
                            origin: AreaId(1),
                            destination: AreaId(2),
                            duration: SimDuration(100),
                        },
                        author: PersonId(10),
                        written_at: SimTime(0),
                        readers: BTreeSet::new(),
                        intact: true,
                    },
                )
                .unwrap();
        }
        let requests = (0..n)
            .map(|id| game_cargo::TransferRequest {
                intent: game_cargo::TransferIntent {
                    actor: PersonId(id + 10000),
                    source: InventoryId(1),
                    destination: InventoryId(2),
                    quantity: Quantity(1),
                },
                facts: game_cargo::ClosingFacts {
                    alive: true,
                    health: 100,
                    source_contact: true,
                    source_access: true,
                    receiver_contact: true,
                    receiver_access: true,
                    receiver_accepted: false,
                    record_permission: true,
                },
            })
            .collect();
        let prep = Instant::now();
        let mut phase = game_cargo::prepare_transfers(&store, None, requests, None, |_| true)
            .unwrap()
            .begin(&store)
            .unwrap();
        let prepare_ms = prep.elapsed().as_secs_f64() * 1000.;
        let runtime = Runtime {
            control: ControlState::new(Rules::fixture()).unwrap(),
        };
        let start = Instant::now();
        let mut grants = 0;
        loop {
            let mut batch = Batch::new();
            let Some((intent, granted)) = phase.stage_next_into(&store, &mut batch).unwrap() else {
                break;
            };
            if granted {
                grants += 1;
            }

            if granted {
                if let Some(record) = runtime
                    .prepare_record_transfer(
                        &store,
                        intent.source,
                        intent.destination,
                        intent.quantity,
                        None,
                    )
                    .unwrap()
                {
                    record.stage_into(&store, &mut batch).unwrap();
                }
                store.apply_batch(batch).unwrap();
                if let Some(record) = runtime
                    .prepare_record_receipt(
                        &store,
                        intent.source,
                        intent.destination,
                        intent.quantity,
                    )
                    .unwrap()
                {
                    record.commit(&mut store).unwrap();
                }
            }
        }
        let commit_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(
            game_cargo::inventory(&store, InventoryId(1))
                .unwrap()
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&store, InventoryId(2))
                .unwrap()
                .quantity,
            Quantity(quantity)
        );
        assert_eq!(grants, quantity);
        println!(
            "transfers requests={n} grants={grants} unrelated_records={unrelated} prepare_ms={prepare_ms:.3} commit_ms={commit_ms:.3}"
        );
        std::hint::black_box(store);
    }
}

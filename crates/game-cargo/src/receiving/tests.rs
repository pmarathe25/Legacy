use super::*;
use crate::facts::*;
use game_core::{AreaId, Location, SimDuration, SimTime, Vitals};
use game_ecs::Schema;
use game_physiology::{Body, Rules, services::BodyFacts};
use game_world::{GenericHost, OwnerReadTxn};
use std::collections::BTreeSet;

struct Peers;
impl OwnerSystem<()> for Peers {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, ()>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
struct AllocateInventoryId;
impl Request for AllocateInventoryId {
    type Response = InventoryId;
}
fn rules() -> Rules {
    Rules {
        hunger_harm: 1000,
        fatigue_limit: 1000,
        max_health: 100,
        meal_relief: 200,
        meal_size: Quantity(200),
        eating_time: SimDuration(10),
        loading_time: SimDuration(10),
        carrying_capacity: Quantity(1000),
    }
}
fn body(
    _: &Peers,
    tx: &mut OwnerReadTxn<'_, '_, Peers, ()>,
    request: game_physiology::services::ReadBody,
) -> Result<Option<BodyFacts>, Error> {
    Ok(tx
        .read::<Body>(StableId(request.0.0))
        .map(|body| BodyFacts {
            vitals: body.vitals.clone(),
            rules: rules(),
        }))
}
fn fixture() -> (GenericHost<(), ()>, Registry<(), ()>) {
    let mut schema = Schema::new();
    crate::register_components(&mut schema).unwrap();
    crate::facts::register_components(&mut schema).unwrap();
    game_physiology::register_components(&mut schema).unwrap();
    game_physiology::services::register_task_components(&mut schema).unwrap();
    game_transport::services::register_task_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut host = GenericHost::new(schema, 1, ());
    for _ in 0..7 {
        host.allocate_id().unwrap();
    }
    for actor in [1, 2] {
        host.storage
            .insert_or_spawn(
                StableId(actor),
                Body {
                    vitals: Vitals {
                        alive: true,
                        health: 100,
                        hunger: 0,
                        fatigue: 0,
                    },
                    fatigue_remainder: 0,
                    health_remainder: 0,
                },
            )
            .unwrap();
        host.storage
            .insert(StableId(actor), Location::AtArea(AreaId(9)))
            .unwrap();
    }
    for (id, location, good, amount, capacity) in [
        (
            3,
            crate::InventoryLocation::Area(AreaId(9)),
            Good::Food,
            1000,
            2000,
        ),
        (
            4,
            crate::InventoryLocation::Carried(PersonId(1)),
            Good::Food,
            0,
            1000,
        ),
        (
            5,
            crate::InventoryLocation::Area(AreaId(9)),
            Good::Food,
            0,
            2000,
        ),
        (
            6,
            crate::InventoryLocation::Carried(PersonId(1)),
            Good::Water,
            900,
            1000,
        ),
        (
            7,
            crate::InventoryLocation::Area(AreaId(9)),
            Good::Tool,
            1000,
            2000,
        ),
    ] {
        host.storage
            .insert_or_spawn(
                StableId(id),
                Inventory {
                    id: InventoryId(id),
                    location,
                    good,
                    quantity: Quantity(amount),
                    capacity: Quantity(capacity),
                    access: BTreeSet::from([PersonId(1), PersonId(2)]),
                    last_granted: None,
                    spoils_at: None,
                },
            )
            .unwrap();
    }
    let mut registry = Registry::new();
    registry.register_stateless(Peers).unwrap();
    registry
        .register_request::<Peers, AllocateInventoryId>(|_, tx, _| {
            tx.reserve_ids(1).map(|id| InventoryId(id.0))
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::services::ReadBody>(body)
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::services::ReadIntakeReservations>(
            |_, tx, request| {
                Ok(tx
                    .iter::<game_physiology::services::IntakeTask>()
                    .into_iter()
                    .filter(|(_, row)| request.source.is_none_or(|source| source == row.source))
                    .map(|(actor, row)| game_physiology::PhysicalReservation {
                        actor: PersonId(actor.0),
                        source: row.source,
                        quantity: rules().meal_size,
                    })
                    .collect())
            },
        )
        .unwrap();
    registry
        .register_read_request::<Peers, game_transport::services::ReadLoadingReservations>(
            |_, tx, request| {
                Ok(tx
                    .iter::<game_transport::services::Loading>()
                    .into_iter()
                    .filter(|(_, row)| request.source.is_none_or(|source| source == row.source))
                    .map(
                        |(actor, row)| game_transport::services::LoadingReservation {
                            actor: PersonId(actor.0),
                            source: row.source,
                            quantity: row.quantity,
                        },
                    )
                    .collect())
            },
        )
        .unwrap();
    registry
        .register_read_request::<Peers, game_transport::services::ReadPresence>(|_, tx, request| {
            Ok(tx
                .read::<Location>(StableId(request.0.0))
                .cloned()
                .map(|location| game_transport::services::Presence {
                    resolved: location.clone(),
                    location,
                }))
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_transport::systems::ReadContact>(|_, tx, request| {
            let first = tx.read::<Location>(StableId(request.first.0)).cloned();
            let second = tx.read::<Location>(StableId(request.second.0)).cloned();
            Ok(first.is_some() && first == second)
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_transport::systems::CheckCargoCapacity>(|_, _, _| {
            Ok(true)
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::systems::ReadAgency>(|_, _, _| {
            Ok(game_physiology::systems::AgencyFacts {
                independent: true,
                hunger_exempt: false,
            })
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::systems::ReadConflict>(|_, _, _| {
            Ok(game_physiology::systems::ConflictFacts {
                allowed: true,
                incapacitation: None,
            })
        })
        .unwrap();
    registry
        .register_read_request::<Peers, game_physiology::services::ReadCareReceived>(|_, _, _| {
            Ok(false)
        })
        .unwrap();
    registry
        .register_read_request::<Peers, ReadStockPermission>(|_, _, _| Ok(false))
        .unwrap();
    registry
        .register_read_request::<Peers, ReadRecordPermission>(|_, _, _| Ok(true))
        .unwrap();
    registry
        .register_read_request::<Peers, ReadAgreementAcceptance>(|_, _, _| Ok(false))
        .unwrap();
    registry
        .register_read_request::<Peers, ReadGuardianAcceptance>(|_, _, _| Ok(false))
        .unwrap();
    crate::facts::register_services(&mut registry, RuntimeSlot::stateless()).unwrap();
    register_cargo_receiving(&mut registry, RuntimeSlot::stateless()).unwrap();
    (host, registry)
}
fn transfer_request(
    source: u64,
    destination: u64,
    quantity: u64,
) -> game_transport::systems::TransferCargo {
    game_transport::systems::TransferCargo {
        actor: PersonId(1),
        source: InventoryId(source),
        destination: InventoryId(destination),
        quantity: Quantity(quantity),
    }
}
#[test]
fn native_transfer_conserves_quantity_and_cursor() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(transfer_request(3, 5, 300))
        })
        .unwrap();
    let receipt = registry.commit(&mut host, prepared).unwrap();
    assert_eq!(receipt.quantity, Quantity(300));
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(700)
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(5))
            .unwrap()
            .quantity,
        Quantity(300)
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .last_granted,
        Some(PersonId(1))
    );
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            let id = tx.request(AllocateInventoryId)?;
            let receiving = Inventory {
                id,
                location: crate::InventoryLocation::Area(AreaId(9)),
                good: Good::Food,
                quantity: Quantity(0),
                capacity: Quantity(250),
                access: BTreeSet::from([PersonId(2)]),
                last_granted: None,
                spoils_at: None,
            };
            tx.request(SplitInventory {
                source: InventoryId(3),
                receiving,
                quantity: Quantity(250),
            })
        })
        .unwrap();
    let split = registry.commit(&mut host, prepared).unwrap();
    assert_eq!(split.destination, InventoryId(8));
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(450)
    );
    assert_eq!(
        crate::inventory(&host.storage, split.destination)
            .unwrap()
            .quantity,
        Quantity(250)
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .last_granted,
        Some(PersonId(1))
    );
    let before = host.clone();
    assert!(
        registry
            .prepare(&host, SimTime(0), None, |tx| tx.request(CreateInventory {
                row: Inventory {
                    id: InventoryId(1),
                    location: crate::InventoryLocation::Area(AreaId(9)),
                    good: Good::Food,
                    quantity: Quantity(0),
                    capacity: Quantity(250),
                    access: BTreeSet::new(),
                    last_granted: None,
                    spoils_at: None,
                },
            }))
            .is_err()
    );
    assert_eq!(host, before);
}
#[test]
fn native_load_admission_accounts_for_other_carried_compartments() {
    let (mut host, registry) = fixture();
    assert!(
        registry
            .prepare(&host, SimTime(0), None, |tx| tx.request(
                game_transport::systems::AdmitCargoLoad {
                    actor: PersonId(1),
                    source: InventoryId(3),
                    destination: InventoryId(4),
                    quantity: Quantity(101)
                }
            ))
            .is_err()
    );
    let admitted = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(game_transport::systems::AdmitCargoLoad {
                actor: PersonId(1),
                source: InventoryId(3),
                destination: InventoryId(4),
                quantity: Quantity(100),
            })
        })
        .unwrap();
    assert_eq!(registry.commit(&mut host, admitted).unwrap(), SimTime(10));
}
#[test]
fn reservation_claim_and_native_intake_exclude_other_actors_only() {
    let (mut host, registry) = fixture();
    host.storage
        .insert(
            StableId(2),
            game_physiology::services::IntakeTask {
                source: InventoryId(3),
                completes: SimTime(10),
            },
        )
        .unwrap();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(SetReservationClaim {
                actor: PersonId(2),
                claim: Some(ReservationClaim {
                    stock: vec![ReservedStock {
                        source: InventoryId(3),
                        quantity: Quantity(300),
                    }],
                }),
            })?;
            tx.request(game_physiology::systems::ConsumeStock {
                actor: PersonId(1),
                source: InventoryId(3),
                good: Good::Food,
                quantity: Quantity(500),
            })
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(500)
    );
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(game_physiology::systems::ConsumeStock {
                actor: PersonId(2),
                source: InventoryId(3),
                good: Good::Food,
                quantity: Quantity(500),
            })
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(0)
    );
}
#[test]
fn late_failed_credit_rolls_back_previous_debit_and_claims() {
    let (mut host, registry) = fixture();
    let before = host.clone();
    let rejected = registry.prepare(&host, SimTime(0), None, |tx| {
        tx.request(SetToolClaim {
            actor: PersonId(1),
            claim: Some(ToolClaim {
                tool: InventoryId(7),
            }),
        })?;
        tx.request(transfer_request(3, 5, 300))?;
        tx.request(CargoDeposit {
            inventory: InventoryId(5),
            good: Good::Water,
            quantity: Quantity(10),
        })
    });
    assert!(rejected.is_err());
    assert_eq!(host, before);
    // A subsequent valid transaction proves that failed preparation left no lease or cursor.
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(transfer_request(3, 5, 300))
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert!(host.storage.get::<ToolClaim>(StableId(1)).is_none());

    let (host, mut registry) = fixture();
    registry
        .register_subscriber::<Peers, CargoEvent>(|_, _, _| Err(Error::Unavailable))
        .unwrap();
    let before = host.clone();
    assert!(
        registry
            .prepare(&host, SimTime(0), None, |tx| tx
                .request(transfer_request(3, 5, 300)))
            .is_err()
    );
    assert_eq!(host, before);
}
#[test]
fn simultaneous_group_freezes_budgets_while_sequential_receivers_use_current_rows() {
    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(CargoTransferGroup {
                transfers: vec![transfer_request(3, 5, 300), transfer_request(5, 3, 300)],
            })
        })
        .unwrap();
    let decisions = registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        decisions
            .iter()
            .map(|decision| decision.receipt.is_some())
            .collect::<Vec<_>>(),
        vec![true, false]
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(3))
            .unwrap()
            .quantity,
        Quantity(700)
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(5))
            .unwrap()
            .quantity,
        Quantity(300)
    );

    let (mut host, registry) = fixture();
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(CargoDeposit {
                inventory: InventoryId(5),
                good: Good::Food,
                quantity: Quantity(300),
            })?;
            tx.request(CargoConsume {
                inventory: InventoryId(5),
                quantity: Quantity(300),
            })
        })
        .unwrap();
    registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(5))
            .unwrap()
            .quantity,
        Quantity(0)
    );

    let (mut host, registry) = fixture();
    crate::inventory_mut(&mut host.storage, InventoryId(3))
        .unwrap()
        .good = Good::Water;
    crate::inventory_mut(&mut host.storage, InventoryId(5))
        .unwrap()
        .good = Good::Water;
    let prepared = registry
        .prepare(&host, SimTime(0), None, |tx| {
            tx.request(CargoTransferGroup {
                transfers: vec![transfer_request(6, 5, 900), transfer_request(3, 6, 101)],
            })
        })
        .unwrap();
    let decisions = registry.commit(&mut host, prepared).unwrap();
    assert_eq!(
        decisions
            .iter()
            .map(|decision| decision.receipt.is_some())
            .collect::<Vec<_>>(),
        vec![false, true]
    );
    assert_eq!(
        crate::inventory(&host.storage, InventoryId(6))
            .unwrap()
            .quantity,
        Quantity(0)
    );
}

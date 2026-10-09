use game_api::*;
use game_sim::*;
use std::collections::BTreeSet;

fn fixture() -> (Simulation, InventoryId) {
    let mut sim = Simulation::new("health-fixture", 91, Execution::default()).unwrap();
    let actor = sim.canonical_state().ruler;
    let area = match sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == actor)
        .unwrap()
        .location
    {
        Location::AtArea(area) => area,
        _ => unreachable!(),
    };
    let water = sim
        .debug_add_inventory(
            Good::Water,
            InventoryLocation::Area(area),
            Quantity(2_000),
            Quantity(2_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
    (sim, water)
}
fn drink(sim: &mut Simulation, source: InventoryId) {
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Health {
            action: HealthAction::Drink { source },
        },
    })
    .unwrap();
}

#[test]
fn drinking_requires_time_and_real_water_and_does_not_feed_the_body() {
    let (mut sim, source) = fixture();
    sim.advance(SimDuration(50_000)).unwrap();
    let before = sim.observe();
    drink(&mut sim, source);
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    sim.advance(SimDuration(9_999)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    sim.advance(SimDuration(1)).unwrap();
    let after = sim.observe();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(1_000)
    );
    assert_eq!(after.health.as_ref().unwrap().thirst, 0);
    assert_eq!(
        after.physical.as_ref().unwrap().vitals.hunger,
        before.physical.as_ref().unwrap().vitals.hunger + 10_000
    );
}

#[test]
fn an_interrupted_or_removed_drink_cannot_deliver_water() {
    let (mut sim, source) = fixture();
    drink(&mut sim, source);
    sim.advance(SimDuration(5_000)).unwrap();
    sim.submit(Submission {
        caller_sequence: 2,
        command: Command::Cancel,
    })
    .unwrap();
    sim.advance(SimDuration(5_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(2_000)
    );
    assert_eq!(sim.observe().health.unwrap().thirst, 10_000);
    let (mut sim, source) = fixture();
    let actor = sim.canonical_state().ruler;
    let destination = sim
        .debug_add_inventory(
            Good::Water,
            InventoryLocation::Carried(actor),
            Quantity(0),
            Quantity(2_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
    sim.debug_schedule_transfers(
        SimTime(5_000),
        vec![TransferIntent {
            actor,
            source,
            destination,
            quantity: Quantity(2_000),
        }],
    )
    .unwrap();
    drink(&mut sim, source);
    sim.advance(SimDuration(10_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert_eq!(sim.observe().health.unwrap().thirst, 10_000);
}

#[test]
fn pending_drinking_and_clinical_state_survive_current_save_restore_and_slice_changes() {
    let (mut whole, source) = fixture();
    drink(&mut whole, source);
    whole.advance(SimDuration(4_003)).unwrap();
    let mut restored = Simulation::load(
        &whole.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    whole.advance(SimDuration(5_997)).unwrap();
    for elapsed in [1, 996, 5_000] {
        restored.advance(SimDuration(elapsed)).unwrap();
    }
    assert_eq!(whole.canonical_state(), restored.canonical_state());
}

#[test]
fn malformed_clinical_actor_is_rejected_by_the_save_boundary() {
    let (sim, _) = fixture();
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    saved["state"]["health"]["clinical"]["9999999"] =
        serde_json::to_value(game_sim::health::Clinical::default()).unwrap();
    assert!(matches!(
        Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()),
        Err(Error::InvalidSave(_))
    ));
}

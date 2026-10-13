use super::*;

fn two_pending() -> (Simulation, PersonId, InventoryId) {
    let mut sim = Simulation::property_fixture(9, Execution::default()).unwrap();
    let people = sim.people().map(|person| person.id).collect::<Vec<_>>();
    for actor in &people {
        sim.debug_actor_command(*actor, Command::Cancel).unwrap();
        sim.debug_set_property_attitude(
            *actor,
            Attitude {
                exclusive_claims: 900,
                sharing: 0,
                willingness_to_defend: 0,
            },
        )
        .unwrap();
    }
    let first = people[0];
    let second = people[1];
    let first_defender = people[3];
    let second_defender = people[4];
    let origin_actor = people[6];
    for actor in [first, second, first_defender, second_defender, origin_actor] {
        if sim.property_area(actor) != Some(AreaId(1)) {
            sim.debug_move_actor(actor, AreaId(1)).unwrap();
        }
    }
    let source = game_cargo::inventories(&sim.host.storage)
        .map(|(_, row)| row)
        .find(|row| row.good == Good::Food && row.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id;
    let other_source = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(2_000),
            Quantity(2_000),
            BTreeSet::from([second_defender]),
        )
        .unwrap();
    game_property::declare_custody(&mut sim.host.storage, other_source, second_defender).unwrap();
    for (actor, stock) in [(first, source), (second, other_source)] {
        let destination = sim.person(actor).inventory;
        sim.debug_property_action(
            actor,
            PropertyAction::Take {
                source: stock,
                destination,
                quantity: Quantity(500),
            },
        )
        .unwrap();
    }
    assert_eq!(game_property::pending_rows(&sim.host.storage).count(), 2);
    for actor in [first_defender, second_defender] {
        sim.debug_set_property_attitude(
            actor,
            Attitude {
                exclusive_claims: 900,
                sharing: 0,
                willingness_to_defend: 900,
            },
        )
        .unwrap();
    }
    (sim, origin_actor, source)
}

#[test]
fn starting_take_reviews_both_preexisting_pending_thieves_in_original_order() {
    let (mut sim, actor, source) = two_pending();
    let first = sim.person_at(0).id;
    let second = sim.person_at(1).id;
    let destination = sim.person(actor).inventory;
    sim.debug_property_action(
        actor,
        PropertyAction::Take {
            source,
            destination,
            quantity: Quantity(500),
        },
    )
    .unwrap();
    assert!(game_property::pending(&sim.host.storage, first).is_none());
    assert!(game_property::pending(&sim.host.storage, second).is_none());
    let interrupted = sim
        .host
        .events()
        .iter()
        .filter_map(|event| match event.event {
            EventKind::TakingInterrupted { actor, .. } => Some(actor),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(&interrupted[..2], &[first, second]);
}

#[test]
fn later_pending_encounter_revision_failure_preserves_entire_property_origin() {
    let (mut sim, actor, source) = two_pending();
    let destination = sim.person(actor).inventory;
    sim.host
        .bump_revision(u64::MAX - sim.host.clock().revision - 2)
        .unwrap();
    let before = sim.host.clone();
    assert_eq!(
        sim.debug_property_action(
            actor,
            PropertyAction::Take {
                source,
                destination,
                quantity: Quantity(500),
            }
        ),
        Err(Error::Overflow)
    );
    assert_eq!(sim.host, before);
}

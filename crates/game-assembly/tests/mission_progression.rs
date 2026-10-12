use game_api::*;
use game_assembly::{
    Execution, Simulation,
    decisions::{Aspiration, Personality},
};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn autonomous_member_collects_real_supplies_then_completes_the_route_across_save_and_workers() {
    let mut serial = Simulation::mission_fixture(129, Execution::default()).unwrap();
    let people = serial
        .canonical_state()
        .state
        .people
        .iter()
        .map(|p| p.id)
        .collect::<Vec<_>>();
    for person in &people {
        serial
            .debug_actor_command(*person, Command::Cancel)
            .unwrap();
        serial
            .debug_set_personality(*person, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    let state = serial.canonical_state();
    let leader = state.state.people[5].id;
    let ruler = state.state.ruler;
    let home = state.state.areas[0].id;
    let forest = state.state.areas[2].id;
    let cargo = state.state.people[5].inventory;
    let food = game_cargo::inventories(state.components)
        .map(|(_, row)| row)
        .find(|stock| {
            stock.good == Good::Food
                && matches!(stock.location, game_cargo::InventoryLocation::Area(_))
        })
        .unwrap()
        .id;
    serial
        .debug_set_personality(leader, Personality::industrious(), Aspiration::Independence)
        .unwrap();
    serial
        .debug_actor_command(
            ruler,
            Command::Mission {
                action: MissionAction::Propose {
                    proposal: MissionProposal {
                        leader,
                        recruits: BTreeSet::new(),
                        objective: MissionObjective::Visit {
                            destination: forest,
                        },
                        home,
                        supplies: vec![MissionSupply {
                            carrier: leader,
                            source: food,
                            quantity: Quantity(2000),
                        }],
                        compensation: BTreeMap::new(),
                        deadline: SimTime(720_000),
                    },
                },
            },
        )
        .unwrap();
    let state = serial.canonical_state();
    let mission = game_missions::missions(state.components)
        .map(|(id, _)| id)
        .last()
        .unwrap();
    let recruitment = game_missions::mission(state.components, mission)
        .unwrap()
        .members[&leader]
        .recruitment;
    serial
        .debug_actor_command(
            leader,
            Command::Mission {
                action: MissionAction::Accept { recruitment },
            },
        )
        .unwrap();
    let mut reordered = Simulation::load(
        &serial.save().unwrap(),
        Execution {
            workers: 3,
            reverse_jobs: true,
        },
    )
    .unwrap();
    serial.advance(SimDuration(60_000)).unwrap();
    for duration in [17_000, 43_000] {
        reordered.observe();
        reordered.advance(SimDuration(duration)).unwrap();
    }
    assert_eq!(serial.canonical_state(), reordered.canonical_state());
    assert!(matches!(
        serial.canonical_state().components.activity(leader),
        Some(Activity::Load { .. })
    ));
    assert_eq!(
        game_cargo::inventory(serial.canonical_state().components, cargo)
            .unwrap()
            .quantity,
        Quantity(0)
    );
    // The saved active pickup continues through its actual closing receipt.
    reordered = Simulation::load(
        &reordered.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    serial.advance(SimDuration(60_000)).unwrap();
    reordered.advance(SimDuration(60_000)).unwrap();
    assert_eq!(serial.canonical_state(), reordered.canonical_state());
    assert_eq!(
        game_cargo::inventory(serial.canonical_state().components, cargo)
            .unwrap()
            .quantity,
        Quantity(2000)
    );
    assert_eq!(
        game_missions::mission(serial.canonical_state().components, mission)
            .unwrap()
            .members[&leader]
            .supply_cargo[&food]
            .values()
            .sum::<u64>(),
        2000
    );
    serial.advance(SimDuration(600_000)).unwrap();
    for duration in [91_003, 108_997, 143_001, 56_999, 200_000] {
        reordered.observe();
        reordered.advance(SimDuration(duration)).unwrap();
    }
    assert_eq!(serial.canonical_state(), reordered.canonical_state());
    assert_eq!(
        game_missions::mission(serial.canonical_state().components, mission)
            .unwrap()
            .status,
        MissionStatus::Completed
    );
    assert!(serial.canonical_state().events.iter().any(
        |event| matches!(event.event,EventKind::Arrived{actor,area}if actor==leader && area==forest)
    ));
}

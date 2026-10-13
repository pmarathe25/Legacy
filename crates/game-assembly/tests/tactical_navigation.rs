use game_api::*;
use game_assembly::*;
use std::collections::{BTreeMap, BTreeSet};

fn submit(sim: &mut Simulation, command: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}
fn saved_component<T: serde::de::DeserializeOwned>(
    state: &CanonicalSnapshot,
    key: &str,
    id: u64,
) -> T {
    let row = state.components.columns[key]
        .iter()
        .find(|row| row.entity == id)
        .expect("authored component exists");
    serde_json::from_str(row.value.get()).unwrap()
}
fn replace_saved_component<T: serde::Serialize>(
    state: &mut CanonicalSnapshot,
    key: &str,
    id: u64,
    value: &T,
) {
    let row = state
        .components
        .columns
        .get_mut(key)
        .unwrap()
        .iter_mut()
        .find(|row| row.entity == id)
        .expect("authored component exists");
    row.value = serde_json::value::to_raw_value(value).unwrap();
}
fn author_positions(
    sim: &Simulation,
    id: EncounterId,
    positions: &[(PersonId, Point)],
    remembered: bool,
) -> Simulation {
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let mut state: CanonicalSnapshot = serde_json::from_value(envelope["state"].take()).unwrap();
    let mut encounter: game_conflict::Encounter =
        saved_component(&state, "conflict/encounter", id.0);
    let mut map: game_conflict::Map = saved_component(&state, "conflict/map", encounter.map.0);
    map.obstacles = vec![ObstacleView {
        index: 0,
        minimum: Point { x: 9_000, y: 4_000 },
        maximum: Point {
            x: 11_000,
            y: 16_000,
        },
        integrity: 180_000,
    }];
    for (actor, position) in positions {
        let fighter = encounter.fighters.get_mut(actor).unwrap();
        fighter.position = *position;
        fighter.known.clear();
    }
    if remembered {
        encounter
            .fighters
            .get_mut(&positions[1].0)
            .unwrap()
            .known
            .insert(positions[0].0, positions[0].1);
    }
    replace_saved_component(&mut state, "conflict/map", map.id.0, &map);
    replace_saved_component(&mut state, "conflict/encounter", id.0, &encounter);
    envelope["state"] = serde_json::to_value(state).unwrap();
    Simulation::load(
        &serde_json::to_vec(&envelope).unwrap(),
        Execution::default(),
    )
    .unwrap()
}
fn hidden_battle(goal: Point, remembered: bool) -> (Simulation, EncounterId, PersonId, PersonId) {
    let mut sim = Simulation::combat_fixture(1, Execution::default()).unwrap();
    let ruler = sim.canonical_state().state.ruler;
    let npc = sim.canonical_state().person_at(3).id;
    submit(&mut sim, Command::Engage { target: npc });
    let id = sim.observe().combat.unwrap().encounter;
    sim = author_positions(
        &sim,
        id,
        &[
            (ruler, goal),
            (
                npc,
                Point {
                    x: 2_000,
                    y: 10_000,
                },
            ),
        ],
        remembered,
    );
    (sim, id, ruler, npc)
}
fn wait(sim: &mut Simulation, id: EncounterId) {
    submit(
        sim,
        Command::CombatTurn {
            encounter: id,
            movement: None,
            action: CombatAction::Defend,
        },
    );
}

#[test]
fn npc_pursues_a_remembered_position_around_a_wall_and_attacks_only_in_actual_reach() {
    let (mut sim, id, ruler, npc) = hidden_battle(
        Point {
            x: 18_000,
            y: 10_000,
        },
        true,
    );
    let start = game_conflict::encounter(sim.canonical_state().components, id)
        .unwrap()
        .fighters[&npc]
        .position;
    let mut attacked = false;
    for _ in 0..10 {
        wait(&mut sim, id);
        let combat = sim.canonical_state().state.combat.as_ref().unwrap();
        let encounter = game_conflict::encounter(sim.canonical_state().components, id).unwrap();
        let from = encounter.fighters[&npc].position;
        let target = encounter.fighters[&ruler].position;
        if sim.canonical_state().events.iter().any(|e| matches!(e.event, EventKind::Struck { actor, target, .. } if actor == npc && target == ruler)) {
            assert!(game_conflict::map(sim.canonical_state().components,encounter.map).unwrap().clear(from,target));
            assert!(game_conflict::distance(from, target) <= u128::from(combat.rules.reach).pow(2));
            attacked = true;
            break;
        }
        sim.advance(SimDuration(6_000)).unwrap();
    }
    assert!(attacked);
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .filter(|e| matches!(e.event, EventKind::CombatMoved { actor, .. } if actor == npc))
            .count()
            > 2
    );
    assert_ne!(
        game_conflict::encounter(sim.canonical_state().components, id)
            .unwrap()
            .fighters[&npc]
            .position,
        start
    );
}

#[test]
fn blind_search_does_not_depend_on_hidden_enemy_coordinates_and_survives_save_workers() {
    let (mut first, id, _, npc) = hidden_battle(
        Point {
            x: 18_000,
            y: 10_000,
        },
        false,
    );
    let (mut second, second_id, _, second_npc) = hidden_battle(
        Point {
            x: 20_000,
            y: 11_000,
        },
        false,
    );
    wait(&mut first, id);
    wait(&mut second, second_id);
    let one = &game_conflict::encounter(first.canonical_state().components, id)
        .unwrap()
        .fighters[&npc];
    let two = &game_conflict::encounter(second.canonical_state().components, second_id)
        .unwrap()
        .fighters[&second_npc];
    assert_eq!(one.position, two.position);
    assert_eq!(one.search, two.search);
    assert_ne!(
        one.position,
        Point {
            x: 2_000,
            y: 10_000
        }
    );
    assert!(one.search.goal.is_some());
    assert!(one.known.is_empty());
    assert!(one.defending);
    let mut loaded = Simulation::load(
        &first.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    for _ in 0..3 {
        first.advance(SimDuration(6_000)).unwrap();
        loaded.advance(SimDuration(6_000)).unwrap();
        wait(&mut first, id);
        wait(&mut loaded, id);
    }
    assert_eq!(first.canonical_state(), loaded.canonical_state());
}

#[test]
fn an_observed_empty_last_position_does_not_pull_search_back_each_round() {
    let (sim, id, ruler, npc) = hidden_battle(
        Point {
            x: 18_000,
            y: 10_000,
        },
        false,
    );
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let mut state: CanonicalSnapshot = serde_json::from_value(envelope["state"].take()).unwrap();
    let mut encounter: game_conflict::Encounter =
        saved_component(&state, "conflict/encounter", id.0);
    encounter.fighters.get_mut(&npc).unwrap().known.insert(
        ruler,
        Point {
            x: 2_000,
            y: 10_000,
        },
    );
    replace_saved_component(&mut state, "conflict/encounter", id.0, &encounter);
    envelope["state"] = serde_json::to_value(state).unwrap();
    let mut sim = Simulation::load(
        &serde_json::to_vec(&envelope).unwrap(),
        Execution::default(),
    )
    .unwrap();
    wait(&mut sim, id);
    let first = &game_conflict::encounter(sim.canonical_state().components, id)
        .unwrap()
        .fighters[&npc];
    let goal = first.search.goal;
    let position = first.position;
    assert!(!first.known.contains_key(&ruler));
    sim.advance(SimDuration(6_000)).unwrap();
    wait(&mut sim, id);
    let second = &game_conflict::encounter(sim.canonical_state().components, id)
        .unwrap()
        .fighters[&npc];
    assert_eq!(second.search.goal, goal);
    assert!(
        game_conflict::distance(second.position, goal.unwrap())
            < game_conflict::distance(position, goal.unwrap())
    );
}

#[test]
fn a_recruited_companion_searches_instead_of_stalling_without_visible_enemies() {
    let mut sim = Simulation::mission_fixture(29, Execution::default()).unwrap();
    let ruler = sim.canonical_state().state.ruler;
    let leader = sim.canonical_state().person_at(3).id;
    let enemy = sim.canonical_state().person_at(6).id;
    let home = sim.canonical_state().state.areas[0].id;
    let destination = sim.canonical_state().state.areas[2].id;
    submit(
        &mut sim,
        Command::Mission {
            action: MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::new(),
                    objective: MissionObjective::Visit { destination },
                    home,
                    supplies: vec![],
                    compensation: BTreeMap::new(),
                    deadline: SimTime(720_000),
                },
            },
        },
    );
    let mission = game_missions::missions(sim.canonical_state().components)
        .map(|(id, _)| id)
        .last()
        .unwrap();
    let recruitment = game_missions::mission(sim.canonical_state().components, mission)
        .unwrap()
        .members[&leader]
        .recruitment;
    sim.debug_actor_command(
        leader,
        Command::Mission {
            action: MissionAction::Accept { recruitment },
        },
    )
    .unwrap();
    sim.debug_actor_command(
        leader,
        Command::Mission {
            action: MissionAction::Depart { mission },
        },
    )
    .unwrap();
    let id = sim
        .debug_start_encounter(vec![(ruler, 0), (leader, 0), (enemy, 1)])
        .unwrap();
    sim = author_positions(
        &sim,
        id,
        &[
            (ruler, Point { x: 2_000, y: 2_000 }),
            (
                leader,
                Point {
                    x: 2_000,
                    y: 10_000,
                },
            ),
            (
                enemy,
                Point {
                    x: 18_000,
                    y: 10_000,
                },
            ),
        ],
        false,
    );
    wait(&mut sim, id);
    let companion = &game_conflict::encounter(sim.canonical_state().components, id)
        .unwrap()
        .fighters[&leader];
    assert_ne!(
        companion.position,
        Point {
            x: 2_000,
            y: 10_000
        }
    );
    assert!(companion.search.goal.is_some());
    assert!(companion.defending);
    assert_eq!(
        game_missions::mission(sim.canonical_state().components, mission)
            .unwrap()
            .members[&leader]
            .status,
        CompanionStatus::Accepted
    );
}

#[test]
fn a_nearby_exit_cannot_be_used_through_an_intact_wall() {
    let mut sim = Simulation::combat_fixture(1, Execution::default()).unwrap();
    let ruler = sim.canonical_state().state.ruler;
    let npc = sim.canonical_state().person_at(3).id;
    submit(&mut sim, Command::Engage { target: npc });
    let id = sim.observe().combat.unwrap().encounter;
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let mut state: CanonicalSnapshot = serde_json::from_value(envelope["state"].take()).unwrap();
    let mut encounter: game_conflict::Encounter =
        saved_component(&state, "conflict/encounter", id.0);
    let mut map: game_conflict::Map = saved_component(&state, "conflict/map", encounter.map.0);
    let destination = map.exits[0].0;
    map.exits[0].1 = Point { x: 0, y: 10_000 };
    map.obstacles = vec![ObstacleView {
        index: 0,
        minimum: Point { x: 500, y: 9_000 },
        maximum: Point {
            x: 1_000,
            y: 11_000,
        },
        integrity: 180_000,
    }];
    encounter.fighters.get_mut(&ruler).unwrap().position = Point {
        x: 1_500,
        y: 10_000,
    };
    encounter.fighters.get_mut(&npc).unwrap().position = Point {
        x: 18_000,
        y: 10_000,
    };
    for fighter in encounter.fighters.values_mut() {
        fighter.known.clear();
    }
    replace_saved_component(&mut state, "conflict/map", map.id.0, &map);
    replace_saved_component(&mut state, "conflict/encounter", id.0, &encounter);
    envelope["state"] = serde_json::to_value(state).unwrap();
    sim = Simulation::load(
        &serde_json::to_vec(&envelope).unwrap(),
        Execution::default(),
    )
    .unwrap();
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter: id,
                movement: None,
                action: CombatAction::Retreat { destination }
            }
        }),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
}

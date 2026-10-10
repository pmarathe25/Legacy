use game_api::*;
use game_world::*;
use std::collections::{BTreeMap, BTreeSet};

fn submit(sim: &mut Simulation, command: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}
fn advance_until(sim: &mut Simulation, mut ready: impl FnMut(&CanonicalState) -> bool) {
    for _ in 0..40 {
        if ready(sim.canonical_state()) {
            return;
        }
        sim.advance(MINUTE).unwrap();
    }
    panic!(
        "The actual parties did not reach the expected stage at {:?}: battles={:?}, approaches={:?}, orders={:?}, talks={:?}, actors={:?}",
        sim.time(),
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .encounters
            .values()
            .map(|e| (e.active, e.fighters.len()))
            .collect::<Vec<_>>(),
        sim.canonical_state().armies.as_ref().unwrap().approaches,
        sim.canonical_state()
            .armies
            .as_ref()
            .unwrap()
            .kernel
            .armies
            .values()
            .flat_map(|a| a.orders.values().map(|o| (&o.objective, &o.recipients)))
            .collect::<Vec<_>>(),
        sim.canonical_state().armies.as_ref().unwrap().talks,
        sim.canonical_state()
            .missions
            .as_ref()
            .unwrap()
            .missions
            .values()
            .map(|m| m
                .members
                .values()
                .map(|p| (
                    p.person,
                    p.status,
                    &p.directive,
                    &sim.canonical_state().physical.as_ref().unwrap().actors[&p.person].activity
                ))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );
}
fn prepared() -> (Simulation, ArmyId, Vec<MissionId>, Vec<PersonId>, PersonId) {
    let mut sim = Simulation::new("army-fixture", 7, Execution::default()).unwrap();
    let village = sim.canonical_state().areas[0].id;
    let forest = sim.canonical_state().areas[2].id;
    let enemy = sim.canonical_state().people[3].id;
    sim.debug_move_actor(enemy, forest).unwrap();
    let people: Vec<_> = sim.canonical_state().people[4..]
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(people.len(), 65);
    let food = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(village))
        .unwrap()
        .id;
    let binding = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Binding && i.location == InventoryLocation::Area(village))
        .unwrap()
        .id;
    let mut missions = Vec::new();
    for crew in [&people[..22], &people[22..44], &people[44..]] {
        let leader = crew[0];
        let mut supplies: Vec<_> = crew
            .iter()
            .map(|person| MissionSupply {
                carrier: *person,
                source: food,
                quantity: Quantity(2_000),
            })
            .collect();
        supplies.push(MissionSupply {
            carrier: leader,
            source: binding,
            quantity: Quantity(1_000),
        });
        submit(
            &mut sim,
            Command::Mission {
                action: MissionAction::Propose {
                    proposal: MissionProposal {
                        leader,
                        recruits: crew[1..].iter().copied().collect(),
                        objective: MissionObjective::Patrol {
                            areas: BTreeSet::from([village]),
                            until: SimTime(12_000_000),
                        },
                        home: village,
                        supplies,
                        compensation: BTreeMap::new(),
                        deadline: SimTime(15_000_000),
                    },
                },
            },
        );
        missions.push(
            *sim.canonical_state()
                .missions
                .as_ref()
                .unwrap()
                .missions
                .keys()
                .next_back()
                .unwrap(),
        );
    }
    sim.advance(SimDuration(180_000)).unwrap();
    assert!(missions.iter().all(|m| {
        sim.canonical_state().missions.as_ref().unwrap().missions[m]
            .members
            .values()
            .all(|p| p.status == CompanionStatus::Accepted)
    }));
    submit(
        &mut sim,
        Command::Army {
            action: ArmyAction::Form {
                missions: missions.iter().copied().collect(),
            },
        },
    );
    let army = *sim
        .canonical_state()
        .armies
        .as_ref()
        .unwrap()
        .kernel
        .armies
        .keys()
        .next_back()
        .unwrap();
    advance_until(&mut sim, |state| {
        state
            .armies
            .as_ref()
            .unwrap()
            .kernel
            .view(state.ruler, army)
            .unwrap()
            .detachments
            .values()
            .all(|d| d.value.participation == game_armies::Participation::Accepted)
    });
    submit(
        &mut sim,
        Command::Army {
            action: ArmyAction::Order {
                army,
                missions: missions.iter().copied().collect(),
                objective: MissionObjective::Patrol {
                    areas: BTreeSet::from([forest]),
                    until: SimTime(12_000_000),
                },
                expires: SimTime(9_000_000),
            },
        },
    );
    advance_until(&mut sim, |state| {
        state.missions.as_ref().unwrap().missions.values().all(|m| {
            m.proposal.objective
                == MissionObjective::Patrol {
                    areas: BTreeSet::from([forest]),
                    until: SimTime(12_000_000),
                }
        }) && state.armies.as_ref().unwrap().talks.is_empty()
    });
    submit(
        &mut sim,
        Command::Move {
            destination: forest,
        },
    );
    advance_until(&mut sim, |state| {
        state
            .people
            .iter()
            .filter(|p| people.contains(&p.id))
            .all(|p| p.location == Location::AtArea(forest))
            && state
                .people
                .iter()
                .find(|p| p.id == state.ruler)
                .unwrap()
                .location
                == Location::AtArea(forest)
    });
    (sim, army, missions, people, enemy)
}
fn battle(
    injured_opponent: bool,
) -> (
    Simulation,
    ArmyId,
    Vec<MissionId>,
    Vec<PersonId>,
    PersonId,
    EncounterId,
) {
    let (mut sim, army, missions, people, enemy) = prepared();
    let village = sim.canonical_state().areas[0].id;
    let forest = sim.canonical_state().areas[2].id;
    // The opposing person physically withdraws while all named leaders hear
    // the force objective. There is no command that freezes the opponent.
    sim.debug_move_actor(enemy, village).unwrap();
    submit(
        &mut sim,
        Command::Army {
            action: ArmyAction::Order {
                army,
                missions: missions.iter().copied().collect(),
                objective: MissionObjective::Confine {
                    person: enemy,
                    until: SimTime(8_000_000),
                },
                expires: SimTime(9_000_000),
            },
        },
    );
    advance_until(&mut sim, |state| {
        state.missions.as_ref().unwrap().missions.values().all(|m|matches!(m.proposal.objective,MissionObjective::Confine {person,..} if person==enemy)) && state.armies.as_ref().unwrap().talks.is_empty()
    });
    if injured_opponent {
        let mut traits = game_world::decisions::Personality::industrious();
        traits.risk_tolerance = 0;
        sim.debug_set_personality(enemy, traits, game_world::decisions::Dream::Comfort)
            .unwrap();
        sim.debug_injure(enemy, 100_000).unwrap();
    }
    // An authored forest barrier makes the actual encounter require detours.
    // Every person, recruitment, provision and journey remains native state.
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let mut state: CanonicalState = serde_json::from_value(envelope["state"].take()).unwrap();
    let map = CombatMapId(state.next_id);
    state.next_id += 1;
    state.combat.as_mut().unwrap().maps.insert(
        forest,
        combat::Map {
            id: map,
            version: 1,
            area: forest,
            location: Location::AtArea(forest),
            width: 24_000,
            height: 16_000,
            obstacles: vec![ObstacleView {
                index: 0,
                minimum: Point { x: 5_000, y: 0 },
                maximum: Point {
                    x: 7_000,
                    y: 15_000,
                },
                integrity: 180_000,
            }],
            exits: vec![(village, Point { x: 0, y: 8_000 })],
        },
    );
    envelope["state"] = serde_json::to_value(state).unwrap();
    sim = Simulation::load(
        &serde_json::to_vec(&envelope).unwrap(),
        Execution::default(),
    )
    .unwrap();
    sim.debug_move_actor(enemy, forest).unwrap();
    advance_until(&mut sim, |state| {
        state
            .combat
            .as_ref()
            .unwrap()
            .encounters
            .values()
            .any(|e| e.active && e.fighters.len() == 66)
    });
    let encounter = sim
        .canonical_state()
        .combat
        .as_ref()
        .unwrap()
        .encounters
        .values()
        .find(|e| e.active && e.fighters.len() == 66)
        .unwrap()
        .id;
    (sim, army, missions, people, enemy, encounter)
}
#[test]
fn sixty_five_real_recruits_carry_provisions_travel_and_share_one_initiative_clock() {
    let (mut sim, _, missions, people, enemy, encounter) = battle(false);
    let state = sim.canonical_state();
    assert_eq!(state.people.len(), 69);
    let roster: BTreeSet<_> = missions
        .iter()
        .flat_map(|m| {
            state.missions.as_ref().unwrap().missions[m]
                .members
                .keys()
                .copied()
        })
        .collect();
    assert_eq!(roster, people.iter().copied().collect());
    assert_eq!(
        state
            .events
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::MissionRecruitmentChanged {
                    status: CompanionStatus::Accepted,
                    ..
                }
            ))
            .count(),
        65
    );
    for person in &people {
        assert!(
            state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Departed {actor,..} if actor==*person))
        );
        assert!(
            state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Arrived {actor,..} if actor==*person))
        );
        assert_eq!(
            state
                .inventories
                .values()
                .filter(
                    |i| i.good == Good::Food && i.location == InventoryLocation::Carried(*person)
                )
                .map(|i| i.quantity.0)
                .sum::<u64>(),
            2_000
        );
    }
    let actual: BTreeSet<_> = state.combat.as_ref().unwrap().encounters[&encounter]
        .fighters
        .keys()
        .copied()
        .collect();
    assert_eq!(actual, roster.iter().copied().chain([enemy]).collect());
    assert!(!actual.contains(&state.ruler));
    let before = sim.time();
    let round = sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter].round;
    sim.advance(SimDuration(6_000)).unwrap();
    assert_eq!(sim.time(), SimTime(before.0 + 6_000));
    assert_eq!(
        sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter].round,
        round + 1
    );
    let mut restored = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(18_000)).unwrap();
    for slice in [1_001, 5_003, 2_777, 9_219] {
        restored.advance(SimDuration(slice)).unwrap();
    }
    assert_eq!(sim.canonical_state(), restored.canonical_state());
}
#[test]
fn individual_death_surrender_and_real_binding_capture_persist_without_replacing_the_army() {
    let (mut sim, army, missions, people, enemy, encounter) = battle(true);
    let lost = people[1];
    sim.debug_injure(lost, 180_000).unwrap();
    advance_until(&mut sim, |s| {
        s.combat.as_ref().unwrap().captivity.contains_key(&enemy)
    });
    let state = sim.canonical_state();
    assert!(!state.physical.as_ref().unwrap().actors[&lost].vitals.alive);
    assert!(
        state
            .events
            .iter()
            .any(|e| matches!(e.kind,EventKind::Died {actor} if actor==lost))
    );
    let captive = state
        .combat
        .as_ref()
        .unwrap()
        .captivity
        .get(&enemy)
        .expect("a real NPC surrender permits the supplied leader to capture");
    assert!(people.contains(&captive.captor));
    assert_eq!(
        state.inventories[&captive.binding].quantity,
        Quantity(1_000)
    );
    assert_eq!(
        state.inventories[&captive.binding].location,
        InventoryLocation::Carried(enemy)
    );
    assert!(
        state
            .events
            .iter()
            .any(|e| matches!(e.kind,EventKind::Captured {target,..} if target==enemy))
    );
    assert_eq!(
        state.missions.as_ref().unwrap().missions[&missions[0]].members[&lost].status,
        CompanionStatus::Dead
    );
    assert_eq!(
        state.armies.as_ref().unwrap().kernel.armies[&army]
            .detachments
            .len(),
        3
    );
    assert_eq!(
        state.combat.as_ref().unwrap().encounters[&encounter]
            .fighters
            .len(),
        66
    );
    let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(state, restored.canonical_state());
}

#[test]
fn commander_personally_approaches_existing_battle_and_gets_an_individual_turn() {
    let (mut sim, army, missions, _, _, encounter) = battle(false);
    let ruler = sim.canonical_state().ruler;
    assert!(missions.iter().all(|m| {
        !sim.canonical_state().missions.as_ref().unwrap().missions[m]
            .members
            .contains_key(&ruler)
    }));
    assert!(
        !sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter]
            .fighters
            .contains_key(&ruler)
    );
    let action=sim.observe().available_commands.into_iter().find(|c|matches!(c,Command::Army {action:ArmyAction::Approach {army:a,encounter:e,..}} if *a==army && *e==encounter))
        .expect("a known physically local allied battle offers personal entry");
    let started = sim.time();
    let opening_round = sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter].round;
    submit(&mut sim, action);
    sim.advance(SimDuration(3_000)).unwrap();
    assert!(
        !sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter]
            .fighters
            .contains_key(&ruler)
    );
    assert!(matches!(
        sim.observe().physical.unwrap().activity,
        Activity::ArmyApproach { .. }
    ));
    let mut restored = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(3_000)).unwrap();
    restored.advance(SimDuration(3_000)).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    assert_eq!(sim.time(), SimTime(started.0 + 6_000));
    let state = sim.canonical_state();
    let scene = &state.combat.as_ref().unwrap().encounters[&encounter];
    assert_eq!(scene.fighters[&ruler].joins_round, opening_round + 1);
    assert_eq!(sim.observe().combat.unwrap().turn, Some(ruler));
    assert_eq!(state.people.len(), 69);
    assert!(missions.iter().all(|m| {
        !state.missions.as_ref().unwrap().missions[m]
            .members
            .contains_key(&ruler)
    }));
    let step = Point { x: 1_000, y: 8_000 };
    submit(
        &mut sim,
        Command::CombatTurn {
            encounter,
            movement: Some(step),
            action: CombatAction::Defend,
        },
    );
    assert_eq!(
        sim.canonical_state().combat.as_ref().unwrap().encounters[&encounter].fighters[&ruler]
            .position,
        step
    );
}

#[test]
fn a_leader_refuses_a_new_force_objective_and_remote_orders_are_atomic() {
    let (mut sim, army, missions, _, enemy) = prepared();
    let leader = sim.canonical_state().missions.as_ref().unwrap().missions[&missions[0]]
        .proposal
        .leader;
    let old = sim.canonical_state().missions.as_ref().unwrap().missions[&missions[0]]
        .proposal
        .objective
        .clone();
    let mut traits = game_world::decisions::Personality::restful();
    traits.ambition = 0;
    traits.sociability = 0;
    traits.risk_tolerance = 0;
    traits.empathy = 1_000;
    sim.debug_set_personality(leader, traits, game_world::decisions::Dream::Comfort)
        .unwrap();
    submit(
        &mut sim,
        Command::Army {
            action: ArmyAction::Order {
                army,
                missions: BTreeSet::from([missions[0]]),
                objective: MissionObjective::Confine {
                    person: enemy,
                    until: SimTime(8_000_000),
                },
                expires: SimTime(9_000_000),
            },
        },
    );
    sim.advance(SimDuration(120_000)).unwrap();
    let state = sim.canonical_state();
    let order = state.armies.as_ref().unwrap().kernel.armies[&army]
        .orders
        .values()
        .next_back()
        .unwrap();
    assert_eq!(
        order.recipients[&missions[0]].status,
        game_armies::OrderStatus::Refused
    );
    assert_eq!(
        state.missions.as_ref().unwrap().missions[&missions[0]]
            .proposal
            .objective,
        old
    );
    assert_eq!(
        state.missions.as_ref().unwrap().missions[&missions[0]].members[&leader].status,
        CompanionStatus::Accepted
    );
    assert!(state.combat.as_ref().unwrap().encounters.is_empty());
    let village = state.areas[0].id;
    sim.debug_move_actor(leader, village).unwrap();
    let before = sim.canonical_state().clone();
    assert!(
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::Army {
                action: ArmyAction::Order {
                    army,
                    missions: BTreeSet::from([missions[0]]),
                    objective: old,
                    expires: SimTime(9_000_000)
                }
            }
        })
        .is_err()
    );
    assert_eq!(&before, sim.canonical_state());
}

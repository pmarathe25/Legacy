use game_parallel::{MissionEventKind, MissionPhase, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/missions.toml")).unwrap()
}
#[test]
fn accepted_expedition_uses_real_travel_and_returns_after_the_visit() {
    let mut kernel = Kernel::compile(world()).unwrap();
    kernel.step().unwrap();
    let departed = kernel.export();
    assert_eq!(
        departed.missions.as_ref().unwrap().entries[0].phase,
        MissionPhase::Outbound
    );
    assert_eq!(
        departed.missions.as_ref().unwrap().events[0].kind,
        MissionEventKind::Departure
    );
    assert_eq!(departed.travelers[0].person.id, 1);
    assert!(
        departed
            .cargo
            .as_ref()
            .unwrap()
            .holdings
            .iter()
            .any(|holding| holding.person == 1 && holding.food > 0)
    );
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&departed).unwrap()).unwrap())
        .unwrap();
    let mut saw_arrival = false;
    let mut saw_return = false;
    for _ in 0..14 {
        kernel.step().unwrap();
        let state = kernel.export();
        let mission = state.missions.as_ref().unwrap();
        saw_arrival |= mission
            .events
            .iter()
            .any(|event| event.kind == MissionEventKind::Arrival);
        saw_return |= mission
            .events
            .iter()
            .any(|event| event.kind == MissionEventKind::Return);
        if mission.entries[0].phase == MissionPhase::Completed {
            assert!(state.regions[0].people.iter().any(|person| person.id == 1));
            break;
        }
    }
    assert!(saw_arrival && saw_return);
    assert_eq!(
        kernel.export().missions.unwrap().entries[0].phase,
        MissionPhase::Completed
    );
}
#[test]
fn a_dead_person_does_not_depart_after_the_physical_tick() {
    let mut state = world();
    state.cargo = None;
    state.rules.food_growth = 0;
    state.rules.recovery = 0;
    state.rules.hunger_per_tick = 1;
    state.rules.starvation_threshold = 1;
    state.rules.starvation_damage = 2;
    state.regions[0].food = 0;
    state.regions[0].people[0].health = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let after = kernel.export();
    assert!(after.travelers.is_empty());
    assert_eq!(
        after.missions.as_ref().unwrap().events[0].kind,
        MissionEventKind::Unavailable
    );
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().missions.unwrap().entries[0].phase,
        MissionPhase::Failed
    );
}
#[test]
fn an_expedition_departure_reserves_the_actor_before_automatic_work() {
    let mut state = world();
    let work = World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    state.production = work.production;
    state.autowork = work.autowork;
    state.goals = work.goals;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert!(state.production.unwrap().jobs.is_empty());
    assert_eq!(state.travelers[0].person.id, 1);
}
#[test]
fn a_player_can_start_and_cancel_an_expedition_without_teleporting() {
    use game_parallel::MissionStart;
    let mut state = world();
    state.missions.as_mut().unwrap().entries.clear();
    state.missions.as_mut().unwrap().next_id = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    let draft = kernel.prepare().unwrap();
    let request = MissionStart {
        person: 1,
        destination: 2,
        visit_ticks: 2,
    };
    assert_eq!(kernel.start_mission(request).unwrap(), 1);
    assert!(kernel.commit(draft).is_err());
    let before = kernel.export();
    assert!(kernel.start_mission(request).is_err());
    assert!(
        kernel
            .start_mission(MissionStart {
                person: 2,
                destination: 999,
                visit_ticks: 2
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    assert_eq!(kernel.export().travelers[0].person.id, 1);
    assert!(kernel.cancel_mission(2, 1).is_err());
    kernel.cancel_mission(1, 1).unwrap();
    let after = kernel.export();
    assert_eq!(
        after.missions.unwrap().entries[0].phase,
        MissionPhase::Failed
    );
    assert_eq!(after.travelers[0].person.id, 1);
}
#[test]
fn a_controlled_ruler_can_follow_an_explicitly_started_expedition() {
    use game_parallel::MissionStart;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    state.missions = world().missions;
    state.missions.as_mut().unwrap().entries.clear();
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    kernel.step().unwrap();
    assert!(
        kernel
            .export()
            .travelers
            .iter()
            .any(|traveler| traveler.person.id == 1)
    );
}
#[test]
fn children_cannot_start_unaccompanied_expeditions_when_age_is_modeled() {
    use game_parallel::{Mission, MissionStart, Missions, Person};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.regions[0].people.push(Person {
        id: 9,
        hunger: 0,
        health: 100,
    });
    let family = state.families.as_mut().unwrap();
    let mut child = family.members[0].clone();
    child.id = 9;
    child.age = 0;
    child.born = Some(0);
    child.parents = vec![1, 2].into();
    child.partner = None;
    child.pregnancy = None;
    child.reproduction_allowed = false;
    family.members.push(child);
    family.next_id = 10;
    state.missions = Some(Missions {
        next_id: 1,
        pressure_threshold: None,
        entries: vec![],
        events: vec![],
    });
    let mut kernel = Kernel::compile(state.clone()).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .start_mission(MissionStart {
                person: 9,
                destination: 2,
                visit_ticks: 1
            })
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    state.missions = Some(Missions {
        next_id: 2,
        pressure_threshold: None,
        entries: vec![Mission {
            id: 1,
            person: 9,
            party: None,
            party_opt_in: false,
            party_compliant: false,
            objective: None,
            objective_met: false,
            origin: 1,
            destination: 2,
            visit_ticks: 1,
            remaining: 0,
            phase: MissionPhase::Ready,
        }],
        events: vec![],
    });
    assert!(state.validate().is_err());
}
#[test]
fn mission_history_must_reference_the_correct_person_and_mission() {
    use game_parallel::MissionEvent;
    let mut state = world();
    state.missions.as_mut().unwrap().events.push(MissionEvent {
        tick: 0,
        mission: 1,
        person: 2,
        kind: MissionEventKind::Arrival,
    });
    assert!(state.validate().is_err());
}
#[test]
fn lifecycle_failure_does_not_publish_an_expedition_departure() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.missions = world().missions;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn many_independent_expeditions_are_worker_invariant_and_survive_reload() {
    use game_parallel::{Mission, Missions, Person};
    let count = 5_000u64;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/example.toml")).unwrap();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id: id * 13,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    state.missions = Some(Missions {
        next_id: count + 1,
        pressure_threshold: None,
        entries: (1..=count)
            .rev()
            .map(|id| Mission {
                id,
                person: id * 13,
                party: None,
                party_opt_in: false,
                party_compliant: false,
                objective: None,
                objective_met: false,
                origin: 1,
                destination: 2,
                visit_ticks: 1,
                remaining: 0,
                phase: MissionPhase::Ready,
            })
            .collect(),
        events: vec![],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=10 {
                    kernel.step().unwrap();
                    if tick == 3 {
                        kernel = Kernel::compile(
                            World::from_json(&serde_json::to_string(&kernel.export()).unwrap())
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
                kernel.export()
            })
    };
    let one = run(1);
    let four = run(4);
    assert_eq!(one, four);
    assert!(
        one.missions
            .unwrap()
            .entries
            .iter()
            .all(|mission| mission.phase == MissionPhase::Completed)
    );
    assert_eq!(one.regions[0].people.len(), count as usize);
}
#[test]
fn an_expedition_learns_about_the_destination_only_after_real_arrival() {
    let mut state = world();
    state.knowledge = World::from_toml(include_str!("../../../../content/parallel/knowledge.toml"))
        .unwrap()
        .knowledge;
    state.knowledge.as_mut().unwrap().minds[0].records.clear();
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    let arrived = kernel.export();
    assert!(
        !arrived
            .knowledge
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 1)
            .unwrap()
            .records
            .iter()
            .any(|record| record.region == 2)
    );
    kernel.step().unwrap();
    let observed = kernel.export();
    let record = observed
        .knowledge
        .as_ref()
        .unwrap()
        .minds
        .iter()
        .find(|mind| mind.person == 1)
        .unwrap()
        .records
        .iter()
        .find(|record| record.region == 2)
        .unwrap();
    assert_eq!(record.witness, 1);
    assert!(record.observed_at > 0);
    for _ in 0..7 {
        kernel.step().unwrap();
    }
    let returned = kernel.export();
    assert!(
        returned
            .knowledge
            .unwrap()
            .minds
            .iter()
            .find(|mind| mind.person == 1)
            .unwrap()
            .records
            .iter()
            .any(|record| record.region == 2 && record.witness == 1)
    );
}

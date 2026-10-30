use game_parallel::{
    Mission, MissionEventKind, MissionPhase, MissionStart, Missions, Person, StartWork, World,
};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/missions.toml")).unwrap()
}

#[test]
fn independently_accepted_party_waits_for_all_members_and_travels_together() {
    let mut state = world();
    let work = World::from_toml(include_str!("../../../../content/parallel/goals.toml")).unwrap();
    state.production = work.production;
    let mut kernel = Kernel::compile(state).unwrap();
    let second = kernel
        .start_mission(MissionStart {
            person: 2,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 2,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = kernel.export();
    assert!(kernel.form_mission_party(&[second, 1]).is_err());
    assert_eq!(kernel.export(), before);
    kernel.set_mission_party_consent(1, 1, true).unwrap();
    kernel.set_mission_party_consent(2, second, true).unwrap();
    let draft = kernel.prepare().unwrap();
    assert_eq!(kernel.form_mission_party(&[second, 1]).unwrap(), 1);
    assert!(kernel.commit(draft).is_err());
    let before = kernel.export();
    assert!(kernel.form_mission_party(&[1, second]).is_err());
    assert!(kernel.form_mission_party(&[1, 1]).is_err());
    assert_eq!(kernel.export(), before);
    kernel.step().unwrap();
    assert!(kernel.export().travelers.is_empty());
    kernel.cancel_work(1).unwrap();
    kernel.step().unwrap();
    let state = kernel.export();
    assert_eq!(state.travelers.len(), 2);
    assert!(
        state
            .missions
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|m| m.phase == MissionPhase::Outbound && m.party == Some(1))
    );
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    for _ in 0..14 {
        kernel.step().unwrap();
    }
    let state = kernel.export();
    assert!(
        state
            .missions
            .unwrap()
            .entries
            .iter()
            .all(|m| m.phase == MissionPhase::Completed)
    );
    assert!(state.regions[0].people.iter().any(|p| p.id == 1));
    assert!(state.regions[0].people.iter().any(|p| p.id == 2));
}

#[test]
fn unavailable_party_member_delays_only_their_party() {
    let mut state = world();
    state.cargo = None;
    state.rules.food_growth = 0;
    state.rules.recovery = 0;
    state.rules.starvation_threshold = 1;
    state.rules.starvation_damage = 2;
    state.regions[0].food = 0;
    state.regions[0].people[1].health = 1;
    let third = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(third);
    let mut kernel = Kernel::compile(state).unwrap();
    let second = kernel
        .start_mission(MissionStart {
            person: 2,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    kernel.set_mission_party_consent(1, 1, true).unwrap();
    kernel.set_mission_party_consent(2, second, true).unwrap();
    kernel.form_mission_party(&[1, second]).unwrap();
    kernel
        .start_mission(MissionStart {
            person: 3,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    assert_eq!(
        first
            .travelers
            .iter()
            .map(|p| p.person.id)
            .collect::<Vec<_>>(),
        vec![3]
    );
    assert_eq!(
        first.missions.as_ref().unwrap().entries[0].phase,
        MissionPhase::Ready
    );
    assert!(
        first
            .missions
            .unwrap()
            .events
            .iter()
            .any(|e| e.person == 1 && e.kind == MissionEventKind::Unavailable)
    );
    kernel.step().unwrap();
    let second = kernel.export();
    assert!(second.travelers.iter().any(|p| p.person.id == 1));
    assert_eq!(
        second.missions.unwrap().entries[1].phase,
        MissionPhase::Failed
    );
}

#[test]
fn independent_parties_are_worker_invariant_across_save_reload() {
    let count = 2000u64;
    let mut state = world();
    state.cargo = None;
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=count)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    state.regions[1].people.clear();
    state.missions = Some(Missions {
        next_id: count + 1,
        pressure_threshold: None,
        events: vec![],
        entries: (1..=count)
            .rev()
            .map(|id| Mission {
                id,
                person: id,
                party: Some(id - (id - 1) % 2),
                party_opt_in: true,
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
    let result = run(1);
    assert_eq!(result, run(4));
    assert!(
        result
            .missions
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|row| row.phase == MissionPhase::Completed)
    );
    result.validate().unwrap();
}

#[test]
fn mismatched_or_unformed_parties_are_not_valid_save_states() {
    let mut state = world();
    state.missions.as_mut().unwrap().entries[0].party = Some(1);
    assert!(state.validate().is_err());
    let mut kernel = Kernel::compile(world()).unwrap();
    let other = kernel
        .start_mission(MissionStart {
            person: 2,
            destination: 2,
            visit_ticks: 1,
        })
        .unwrap();
    let before = kernel.export();
    assert!(kernel.form_mission_party(&[1, other]).is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn invitation_creates_consenting_companions_and_preserves_real_travel() {
    let state =
        World::from_toml(include_str!("../../../../content/parallel/invitation.toml")).unwrap();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let id = kernel
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 2,
                        visit_ticks: 2,
                    })
                    .unwrap();
                let stale = kernel.prepare().unwrap();
                let companion = kernel.invite_mission(1, id, 2).unwrap().unwrap();
                assert!(kernel.commit(stale).is_err());
                let before = kernel.export();
                assert!(kernel.invite_mission(1, id, 3).is_err());
                assert_eq!(kernel.export(), before);
                let mut kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let departed = kernel.export();
                assert_eq!(departed.travelers.len(), 2);
                let missions = departed.missions.as_ref().unwrap();
                assert!(
                    missions
                        .entries
                        .iter()
                        .all(|row| row.party == Some(id.min(companion))
                            && row.party_opt_in
                            && row.phase == MissionPhase::Outbound)
                );
                for _ in 0..30 {
                    kernel.step().unwrap();
                    if kernel
                        .export()
                        .missions
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .all(|row| row.phase == MissionPhase::Completed)
                    {
                        break;
                    }
                }
                let result = kernel.export();
                assert!(
                    result
                        .missions
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .all(|row| row.phase == MissionPhase::Completed)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn refusing_or_unavailable_companions_do_not_mutate_the_expedition() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/invitation.toml")).unwrap();
    state.social.as_mut().unwrap().participants[1].willing = false;
    let mut kernel = Kernel::compile(state).unwrap();
    let id = kernel
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    let before = kernel.export();
    let draft = kernel.prepare().unwrap();
    assert_eq!(kernel.invite_mission(1, id, 2).unwrap(), None);
    assert!(kernel.invite_mission(1, id, 5).is_err());
    assert!(kernel.invite_mission(1, id, 1).is_err());
    assert_eq!(kernel.export(), before);
    kernel.commit(draft).unwrap();
}

#[test]
fn fear_can_motivate_travel_without_private_approval_or_binding_the_traveler() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/invitation.toml")).unwrap();
    state.social.as_mut().unwrap().participants[1].willing = false;
    state.missions.as_mut().unwrap().pressure_threshold = Some(4);
    state.opinions = Some(Opinions {
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        sensitivities: vec![],
        events: vec![],
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: -3,
            fear: 4,
            obligation: 0,
            affinity: 0,
        }],
    });
    let mut low_pressure = state.clone();
    low_pressure.opinions.as_mut().unwrap().records[0].fear = 3;
    let mut refused = Kernel::compile(low_pressure).unwrap();
    let id = refused
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    let before = refused.export();
    assert_eq!(refused.invite_mission(1, id, 2).unwrap(), None);
    assert_eq!(refused.export(), before);
    let mut obligated = state.clone();
    let feeling = &mut obligated.opinions.as_mut().unwrap().records[0];
    feeling.fear = 0;
    feeling.obligation = 4;
    let mut obligated = Kernel::compile(obligated).unwrap();
    let id = obligated
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    let companion = obligated.invite_mission(1, id, 2).unwrap().unwrap();
    assert!(
        obligated
            .export()
            .missions
            .unwrap()
            .entries
            .iter()
            .any(|row| row.id == companion && row.party_compliant && !row.party_opt_in)
    );
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                let id = kernel
                    .start_mission(MissionStart {
                        person: 1,
                        destination: 2,
                        visit_ticks: 2,
                    })
                    .unwrap();
                let other = kernel.invite_mission(1, id, 2).unwrap().unwrap();
                let before = kernel.export();
                let companion = before
                    .missions
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .find(|row| row.id == other)
                    .unwrap();
                assert!(!companion.party_opt_in);
                assert!(companion.party_compliant);
                assert_eq!(before.opinions.as_ref().unwrap().records[0].trust, -3);
                let mut kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                assert_eq!(kernel.export().travelers.len(), 2);
                kernel.cancel_mission(2, other).unwrap();
                let result = kernel.export();
                assert!(
                    result
                        .missions
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .any(|row| row.id == other && row.phase == MissionPhase::Failed)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn witnessed_harm_can_lead_to_later_pressured_participation() {
    use game_parallel::{CombatEncounter, CombatFighter, CombatOrder, Conflict};
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/invitation.toml")).unwrap();
    state.opinions.as_mut().unwrap().records.clear();
    state.conflict = Some(Conflict {
        damage: 4,
        next_id: 2,
        events: vec![],
        encounters: vec![CombatEncounter {
            id: 1,
            region: 1,
            fighters: vec![
                CombatFighter {
                    person: 1,
                    side: 1,
                    order: CombatOrder::Attack { target: 3 },
                },
                CombatFighter {
                    person: 3,
                    side: 2,
                    order: CombatOrder::Retreat,
                },
            ],
        }],
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let after_harm = kernel.export();
    let feelings = after_harm.opinions.as_ref().unwrap().index();
    assert_eq!(feelings.fear(3, 1), 4);
    assert_eq!(feelings.trust(3, 1), 0);
    let id = kernel
        .start_mission(MissionStart {
            person: 1,
            destination: 2,
            visit_ticks: 2,
        })
        .unwrap();
    let companion = kernel.invite_mission(1, id, 3).unwrap().unwrap();
    let state = kernel.export();
    assert!(
        state
            .missions
            .unwrap()
            .entries
            .iter()
            .any(|row| row.id == companion && row.party_compliant && !row.party_opt_in)
    );
    assert_eq!(state.opinions.unwrap().index().trust(3, 1), 0);
}

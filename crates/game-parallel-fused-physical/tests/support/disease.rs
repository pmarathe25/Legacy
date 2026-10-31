use game_parallel::{CareStart, DiseaseEventKind, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/disease.toml")).unwrap()
}
#[test]
fn exposure_incubation_damage_and_reload_share_one_world() {
    let mut state = world();
    state.rules.recovery = 0;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().disease.as_ref().unwrap().cases.len(), 1);
    kernel.step().unwrap();
    let infected = kernel.export();
    assert_eq!(infected.regions[0].people[0].health, 95);
    let case = infected
        .disease
        .as_ref()
        .unwrap()
        .cases
        .iter()
        .find(|case| case.person == 2)
        .unwrap();
    assert_eq!((case.infected_at, case.source), (2, Some(1)));
    assert!(
        infected
            .disease
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.kind == DiseaseEventKind::Infected && event.person == 2)
    );
    let json = serde_json::to_string(&infected).unwrap();
    let mut reloaded = Kernel::compile(World::from_json(&json).unwrap()).unwrap();
    for _ in 0..5 {
        kernel.step().unwrap();
        reloaded.step().unwrap();
    }
    assert_eq!(kernel.export(), reloaded.export());
    kernel.export().validate().unwrap();
}
#[test]
fn treatment_uses_post_disease_health_and_failed_lifecycle_rolls_back() {
    let mut state = world();
    let care = World::from_toml(include_str!("../../../../content/parallel/care.toml")).unwrap();
    state.production = care.production;
    state.care = care.care;
    state.rules.recovery = 0;
    state.disease.as_mut().unwrap().strains[0].incubation = 0;
    state.care.as_mut().unwrap().duration = 1;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 2,
            patient: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert_eq!(result.regions[0].people[0].health, 100);
    assert_eq!(result.care.unwrap().events[0].healed, 5);
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let disease = world();
    state.social = disease.social;
    state.disease = disease.disease;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn fatal_disease_is_seen_by_the_family_lifecycle() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let disease = world();
    state.social = disease.social;
    state.disease = disease.disease;
    state.rules.recovery = 0;
    state.regions[0].people[0].health = 1;
    state.disease.as_mut().unwrap().strains[0].incubation = 0;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    result.validate().unwrap();
    assert!(
        !result
            .families
            .unwrap()
            .members
            .iter()
            .find(|member| member.id == 1)
            .unwrap()
            .alive
    );
}

#[test]
fn many_infections_are_worker_invariant_across_reload() {
    use game_parallel::{DiseaseCase, Person, SocialParticipant};
    let mut state = world();
    state.regions[0].people = (1..=6000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.rules.hunger_per_tick = 0;
    state.social.as_mut().unwrap().participants = (1..=6000)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    let disease = state.disease.as_mut().unwrap();
    disease.strains[0].incubation = 0;
    disease.cases = (1..=3000)
        .map(|person| DiseaseCase {
            person,
            strain: 1,
            infected_at: 0,
            source: None,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for tick in 1..=4 {
                    kernel.step().unwrap();
                    if tick == 2 {
                        let save = serde_json::to_string(&kernel.export()).unwrap();
                        kernel = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
                    }
                }
                kernel.export()
            })
    };
    let serial = run(1);
    let parallel = run(4);
    assert_eq!(serial, parallel);
    parallel.validate().unwrap();
}

#[test]
fn staged_disease_matches_validated_reference_for_sparse_shuffled_cases() {
    use game_parallel::{DiseaseCase, DiseaseStrain, Person, SocialParticipant};
    use game_parallel_disease::Person as DiseasePerson;
    let mut state = world();
    state.rules.recovery = 0;
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (0..400)
        .map(|i| Person {
            id: 10 + i * 3,
            health: if i % 17 == 0 { 0 } else { 100 },
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.social.as_mut().unwrap().participants = (0..400)
        .map(|i| SocialParticipant {
            person: 10 + i * 3,
            willing: true,
        })
        .collect();
    let disease = state.disease.as_mut().unwrap();
    disease.strains = vec![
        DiseaseStrain {
            id: 9,
            name: "short".into(),
            incubation: 0,
            infectious_ticks: 1,
            immunity_ticks: 0,
            damage_per_tick: 3,
            transmission_per_million: 800_000,
            combat_transmission_per_million: 0,
        },
        DiseaseStrain {
            id: 2,
            name: "long".into(),
            incubation: 1,
            infectious_ticks: 3,
            immunity_ticks: 2,
            damage_per_tick: 1,
            transmission_per_million: 600_000,
            combat_transmission_per_million: 0,
        },
    ];
    disease.cases = (0..200)
        .rev()
        .map(|i| DiseaseCase {
            person: 10 + i * 3,
            strain: if i % 2 == 0 { 9 } else { 2 },
            infected_at: 0,
            source: None,
        })
        .collect();
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..8 {
        let before = kernel.export();
        kernel.step().unwrap();
        let after = kernel.export();
        let people: Vec<_> = before
            .regions
            .iter()
            .flat_map(|region| region.people.iter())
            .map(|person| DiseasePerson {
                id: person.id,
                health: person.health,
            })
            .collect();
        let expected = before
            .disease
            .as_ref()
            .unwrap()
            .prepare(
                &people,
                &after.social.as_ref().unwrap().events,
                before.seed,
                before.tick,
            )
            .unwrap();
        assert_eq!(after.disease.as_ref().unwrap(), &expected.state);
    }
}

#[test]
fn infection_columns_follow_split_travel_and_preserve_initial_case_order() {
    use game_parallel::{DiseaseCase, Move};
    let mut state = world();
    state.disease.as_mut().unwrap().cases = vec![
        DiseaseCase {
            person: 2,
            strain: 1,
            infected_at: 0,
            source: None,
        },
        DiseaseCase {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        },
    ];
    let original = state.disease.clone();
    let mut kernel = Kernel::compile(state).unwrap();
    assert_eq!(kernel.export().disease, original);
    kernel
        .move_people(&[Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    assert_eq!(kernel.export().disease, original);
    kernel.step().unwrap();
    let after = kernel.export();
    assert_eq!(
        after
            .disease
            .as_ref()
            .unwrap()
            .cases
            .iter()
            .map(|c| c.person)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let save = serde_json::to_string(&after).unwrap();
    let mut restored = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
        restored.step().unwrap();
    }
    assert_eq!(kernel.export(), restored.export());
}

#[test]
fn newborns_have_disease_columns_and_can_advance_after_birth() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/enrollment.toml")).unwrap();
    let disease = world().disease.unwrap();
    state.disease = Some(game_parallel::Disease {
        strains: disease.strains,
        treatments: disease.treatments,
        cases: Vec::new(),
        events: Vec::new(),
    });
    let initial = state.regions.iter().map(|r| r.people.len()).sum::<usize>();
    let mut kernel = Kernel::compile(state).unwrap();
    for _ in 0..12 {
        kernel.step().unwrap();
    }
    let after = kernel.export();
    assert!(after.regions.iter().map(|r| r.people.len()).sum::<usize>() > initial);
    after.validate().unwrap();
}

#[test]
fn completed_matching_medicine_cures_after_exposure_and_survives_reload() {
    use game_parallel::{CareStart, DiseaseTreatment};
    let mut state = world();
    let care = World::from_toml(include_str!("../../../../content/parallel/care.toml")).unwrap();
    state.production = care.production;
    state.care = care.care;
    state.care.as_mut().unwrap().duration = 1;
    let medicine = state.care.as_ref().unwrap().medicine;
    let illness = state.disease.as_mut().unwrap();
    illness.strains[0].incubation = 0;
    illness.treatments.push(DiseaseTreatment {
        strain: 1,
        medicine,
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 2,
            patient: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    let after = kernel.export();
    let disease = after.disease.as_ref().unwrap();
    assert!(!disease.cases.iter().any(|case| case.person == 1));
    assert!(
        disease
            .events
            .iter()
            .any(|event| event.person == 1 && event.kind == DiseaseEventKind::Treated)
    );
    let save = serde_json::to_string(&after).unwrap();
    let mut restored = Kernel::compile(World::from_json(&save).unwrap()).unwrap();
    kernel.step().unwrap();
    restored.step().unwrap();
    assert_eq!(kernel.export(), restored.export());
}

#[test]
fn invalid_treatment_definition_is_rejected() {
    use game_parallel::DiseaseTreatment;
    let mut state = world();
    state
        .disease
        .as_mut()
        .unwrap()
        .treatments
        .push(DiseaseTreatment {
            strain: 1,
            medicine: 999,
        });
    assert!(Kernel::compile(state).is_err());
}

#[test]
fn interrupted_treatment_does_not_clear_the_patients_infection() {
    use game_parallel::{CareStart, DiseaseCase};
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/illness_care.toml"
    ))
    .unwrap();
    state.rules.recovery = 0;
    state.regions[0].people[1].health = 1;
    state.care.as_mut().unwrap().duration = 1;
    let illness = state.disease.as_mut().unwrap();
    illness.strains[0].incubation = 0;
    illness.cases.push(DiseaseCase {
        person: 2,
        strain: 1,
        infected_at: 0,
        source: None,
    });
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .start_care(&[CareStart {
            id: 1,
            caregiver: 2,
            patient: 1,
        }])
        .unwrap();
    kernel.step().unwrap();
    let after = kernel.export();
    assert!(after.care.as_ref().unwrap().events[0].interrupted);
    assert!(
        after
            .disease
            .as_ref()
            .unwrap()
            .cases
            .iter()
            .any(|case| case.person == 1)
    );
    assert!(
        !after
            .disease
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.kind == DiseaseEventKind::Treated)
    );
}

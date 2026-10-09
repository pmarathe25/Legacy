use crate::*;
use game_core::*;

#[test]
fn water_partial_consumption_preserves_exact_load_across_slices() {
    let whole = consume_water(&WaterLot::new(10, 2), 5).unwrap();
    let first = consume_water(&WaterLot::new(10, 2), 1).unwrap();
    let second = consume_water(&first.remaining, 4).unwrap();
    assert_eq!(whole.remaining, second.remaining);
    assert_eq!(
        whole.consumed_load,
        first.consumed_load + second.consumed_load
    );
    let remaining = consume_water(&whole.remaining, 5).unwrap();
    assert_eq!(remaining.remaining.load(), 0);
    assert_eq!(remaining.consumed_load, 2 - whole.consumed_load);
}
#[test]
fn maximal_water_stocks_do_not_overflow_quality_arithmetic() {
    let lot = WaterLot::new(u64::MAX, u128::from(u64::MAX) * 1_000);
    let result = consume_water(&lot, u64::MAX).unwrap();
    assert_eq!(result.consumed_load, u128::from(u64::MAX) * 1_000);
    assert_eq!(result.remaining.quantity(), 0);
}
#[test]
fn transfer_and_purification_conserve_water_with_explicit_effects() {
    let source = collect_water(CollectionInput {
        quantity_after: 10,
        existing_load: 0,
        collected: 10,
        contamination: 200,
    })
    .unwrap();
    let destination = WaterLot::new(3, 30);
    let transfer = transfer_water(WaterTransferInput {
        source: &source,
        destination: &destination,
        quantity: 4,
    })
    .unwrap();
    assert_eq!(
        source.quantity() + destination.quantity(),
        transfer.source.quantity() + transfer.destination.quantity()
    );
    assert_eq!(
        source.load() + destination.load(),
        transfer.source.load() + transfer.destination.load()
    );
    assert_eq!(source.quantity(), 10); // Planning did not mutate either opening lot.
    let purified = purify_water(&transfer.destination, 7).unwrap();
    assert_eq!(purified.remaining.quantity(), 7);
    assert_eq!(purified.remaining.load(), 0);
    assert_eq!(purified.consumed_load, 830);
    assert!(consume_water(&source, 11).is_err());
    source.validate(10).unwrap();
}
fn input<'a>(
    rules: &'a Rules,
    state: &'a Clinical,
    now: u64,
    next: u64,
    contacts: [u64; 2],
) -> ClinicalInput<'a> {
    ClinicalInput {
        rules,
        state,
        now: SimTime(now),
        next: SimTime(next),
        alive: true,
        thirst_enabled: true,
        resting: true,
        exposure: ExposureInput {
            temperature_c: 0,
            insulation: 0,
            infectious_contacts: contacts,
        },
        physiology: clinical_physiology(rules, 1_000, 0).unwrap(),
    }
}
#[test]
fn unsafe_water_starts_incubation_at_actual_intake_without_body_mutation() {
    let rules = Rules::fixture();
    let before = Clinical {
        thirst: 250_000,
        ..Clinical::default()
    };
    let state = receive_water(
        &before,
        &rules,
        clinical_physiology(&rules, 1_000, 0).unwrap(),
        SimTime(100),
        InventoryId(9),
        Quantity(1_000),
        50_000,
    )
    .unwrap();
    assert_eq!(state.thirst, 0);
    assert_eq!(
        state.illnesses[&IllnessKind::Enteric].acquired,
        SimTime(100)
    );
    assert_eq!(
        state.illnesses[&IllnessKind::Enteric].onset,
        SimTime(30_100)
    );
    assert!(!state.illnesses[&IllnessKind::Enteric].symptomatic);
    assert_eq!(before.thirst, 250_000);
    let onset = reconcile_clinical(&state, &rules, SimTime(30_100), true).unwrap();
    assert_eq!(
        onset.events,
        vec![ClinicalEvent::Started(IllnessKind::Enteric)]
    );
    assert!(
        clinical_contribution(input(&rules, &onset.state, 30_100, 30_101, [0; 2]))
            .unwrap()
            .health_milli
            < 0
    );
}
#[test]
fn clinical_contributions_return_unclamped_signed_units() {
    let rules = Rules::fixture();
    let state = Clinical {
        thirst: rules.thirst_harm,
        cold: rules.cold_harm,
        ..Clinical::default()
    };
    let delta = clinical_contribution(input(&rules, &state, 0, 1, [0; 2])).unwrap();
    assert_eq!(delta.health_milli, -4_000);
    assert_eq!(delta.fatigue_milli, 500);
    let mut treated = state.clone();
    treated.treatment = Some(Course {
        until: SimTime(20),
        healing: 5_000,
    });
    assert_eq!(
        clinical_contribution(input(&rules, &treated, 0, 1, [0; 2]))
            .unwrap()
            .health_milli,
        1_000
    );
}
#[test]
fn contact_threshold_and_recovery_are_domain_transitions() {
    let rules = Rules::fixture();
    let state = Clinical::default();
    let mut context = input(&rules, &state, 0, 100_000, [0, 1]);
    context.exposure.temperature_c = 12;
    assert_eq!(
        clinical_boundary(context, SimTime(100_000)).unwrap(),
        SimTime(30_000)
    );
    context.next = SimTime(30_000);
    let exposed = advance_clinical(context).unwrap();
    let infected = reconcile_clinical(&exposed, &rules, context.next, true)
        .unwrap()
        .state;
    assert_eq!(
        infected.illnesses[&IllnessKind::Respiratory].onset,
        SimTime(60_000)
    );
    assert!(!infected.illnesses[&IllnessKind::Respiratory].symptomatic);
    let started = reconcile_clinical(&infected, &rules, SimTime(60_000), true).unwrap();
    assert_eq!(
        started.events,
        vec![ClinicalEvent::Started(IllnessKind::Respiratory)]
    );
    let recovered = reconcile_clinical(&started.state, &rules, SimTime(300_000), true).unwrap();
    assert_eq!(
        recovered.events,
        vec![ClinicalEvent::Recovered(IllnessKind::Respiratory)]
    );
    assert!(recovered.state.illnesses.is_empty());
    assert!(
        !recovered
            .state
            .doses
            .contains_key(&IllnessKind::Respiratory)
    );
}
fn advance_slices(chunks: &[u64]) -> (Clinical, HealthDelta, Vec<(u64, ClinicalEvent)>) {
    advance_slices_with_physiology(chunks, 1_000, 0)
}
fn advance_slices_with_physiology(
    chunks: &[u64],
    metabolism: u16,
    tolerance: i16,
) -> (Clinical, HealthDelta, Vec<(u64, ClinicalEvent)>) {
    let mut rules = Rules::fixture();
    rules.cold_accumulation = 37;
    rules.cold_harm = 10_000;
    rules.incubation = SimDuration(2_003);
    rules.illness_duration = SimDuration(60_013);
    let mut state = Clinical::default();
    let mut now = 0;
    let mut total = HealthDelta::default();
    let mut events = Vec::new();
    for chunk in chunks {
        let target = now + chunk;
        while now < target {
            let context = ClinicalInput {
                physiology: clinical_physiology(&rules, metabolism, tolerance).unwrap(),
                ..input(&rules, &state, now, target, [0, 1])
            };
            let next = clinical_boundary(context, SimTime(target)).unwrap();
            let actual = ClinicalInput { next, ..context };
            let delta = clinical_contribution(actual).unwrap();
            total.health_milli += delta.health_milli;
            total.fatigue_milli += delta.fatigue_milli;
            let advanced = advance_clinical(actual).unwrap();
            let transition = reconcile_clinical(&advanced, &rules, next, true).unwrap();
            state = transition.state;
            events.extend(transition.events.into_iter().map(|event| (next.0, event)));
            now = next.0;
        }
    }
    (state, total, events)
}
#[test]
fn clinical_boundaries_make_irregular_slices_equivalent() {
    assert_eq!(
        advance_slices(&[170_003]),
        advance_slices(&[1, 799, 13_003, 28_717, 3, 77_001, 50_479])
    );
}
#[test]
fn real_medical_work_has_explicit_progress_and_borrowed_level_duration() {
    let job = Job {
        actor: PersonId(1),
        area: AreaId(2),
        completes: SimTime(60_000),
        work: Work::Treat {
            patient: PersonId(3),
            skill_ms: 0,
            elapsed: 0,
        },
    };
    let paused = advance_job(&job, 10_000, false, 100).unwrap();
    assert_eq!(paused.completes, SimTime(70_000));
    let Work::Treat {
        skill_ms, elapsed, ..
    } = paused.work
    else {
        panic!()
    };
    assert_eq!((skill_ms, elapsed), (0, 0));
    let first = advance_job(&job, 30_000, true, 20).unwrap();
    let second = advance_job(&first, 30_000, true, 0).unwrap();
    let Work::Treat {
        skill_ms, elapsed, ..
    } = second.work
    else {
        panic!()
    };
    assert_eq!(skill_ms, 600_000);
    assert_eq!(elapsed, 60_000);
    assert_eq!(
        treatment_course(&Rules::fixture(), SimTime(60_000), skill_ms, elapsed)
            .unwrap()
            .healing,
        1_250
    );
}
#[test]
fn owned_state_roundtrips_current_semantic_data() {
    let mut world = World::default();
    world.water.insert(InventoryId(7), WaterLot::new(10, 2));
    world.clinical.insert(
        PersonId(1),
        Clinical {
            thirst: 120,
            cold: 9,
            cold_remainder: 31,
            ..Clinical::default()
        },
    );
    world.offers.insert(
        (PersonId(1), PersonId(2)),
        Offer {
            caregiver: PersonId(1),
            patient: PersonId(2),
            herbs: InventoryId(3),
            water: InventoryId(7),
            expires: SimTime(90),
            accepted: false,
        },
    );
    let json = serde_json::to_value(&world).unwrap();
    assert!(json["offers"].is_array());
    assert_eq!(serde_json::from_value::<World>(json).unwrap(), world);
    assert!(serde_json::from_value::<World>(serde_json::json!({"water":{}})).is_err());
}

#[test]
fn invalid_water_and_rule_inputs_fail_without_partial_changes() {
    let invalid = WaterLot {
        basis_quantity: 1,
        basis_load: 1,
        removed_quantity: 2,
        removed_load: 0,
    };
    assert!(consume_water(&invalid, 0).is_err());
    let mut valid = WaterLot::new(4, 20);
    let opening = valid.clone();
    assert!(valid.take(5).is_err());
    assert_eq!(valid, opening);
    let mut rules = Rules::fixture();
    rules.drink_quantity = Quantity(0);
    assert!(
        receive_water(
            &Clinical::default(),
            &rules,
            clinical_physiology(&rules, 1_000, 0).unwrap(),
            SimTime(0),
            InventoryId(1),
            Quantity(1_000),
            0
        )
        .is_err()
    );
}

#[test]
fn individual_metabolism_changes_deprivation_boundary_damage_and_real_intake_relief() {
    let rules = Rules::fixture();
    let fast = clinical_physiology(&rules, 1_500, 0).unwrap();
    let slow = clinical_physiology(&rules, 500, 0).unwrap();
    assert_eq!((fast.thirst_harm, fast.drink_relief), (120_000, 200_000));
    assert_eq!((slow.thirst_harm, slow.drink_relief), (360_000, 600_000));
    let opening = Clinical::default();
    let mut context = input(&rules, &opening, 0, 400_000, [0; 2]);
    context.exposure.temperature_c = 20;
    context.physiology = fast;
    assert_eq!(
        clinical_boundary(context, SimTime(400_000)).unwrap(),
        SimTime(120_000)
    );
    context.physiology = slow;
    assert_eq!(
        clinical_boundary(context, SimTime(400_000)).unwrap(),
        SimTime(360_000)
    );
    let deprived = Clinical {
        thirst: 200_000,
        ..Clinical::default()
    };
    let mut context = input(&rules, &deprived, 0, 10, [0; 2]);
    context.physiology = fast;
    assert_eq!(
        clinical_contribution(context).unwrap().health_milli,
        -20_000
    );
    context.physiology = slow;
    assert_eq!(clinical_contribution(context).unwrap().health_milli, 0);
    let thirsty = Clinical {
        thirst: 650_000,
        ..Clinical::default()
    };
    let drank = |physiology| {
        receive_water(
            &thirsty,
            &rules,
            physiology,
            SimTime(10),
            InventoryId(2),
            Quantity(500),
            100,
        )
        .unwrap()
    };
    assert_eq!(drank(fast).thirst, 550_000);
    assert_eq!(drank(slow).thirst, 350_000);
    // Neither heredity nor the relief calculation alters the actual dose.
    assert_eq!(drank(fast).doses, drank(slow).doses);
    assert!(clinical_physiology(&rules, 0, 0).is_err());
}

#[test]
fn cold_tolerance_keeps_centi_degree_effects_and_exact_fractional_progress() {
    let rules = Rules::fixture();
    assert_eq!(cold_rate(&rules, 0, 0, 0), 1_250);
    assert_eq!(cold_rate(&rules, 0, 0, 125), 937);
    assert_eq!(cold_rate(&rules, 0, 0, -125), 1_562);
    let opening = Clinical::default();
    let mut context = input(&rules, &opening, 0, 100_000, [0; 2]);
    context.physiology = clinical_physiology(&rules, 1_000, 125).unwrap();
    assert_eq!(
        clinical_boundary(context, SimTime(100_000)).unwrap(),
        SimTime(64_035)
    );
    context.next = SimTime(1);
    let cold = advance_clinical(context).unwrap();
    assert_eq!((cold.cold, cold.cold_remainder), (0, 937));
    context.state = &cold;
    context.now = SimTime(1);
    context.next = SimTime(2);
    let cold = advance_clinical(context).unwrap();
    assert_eq!((cold.cold, cold.cold_remainder), (1, 874));
}

#[test]
fn expressed_traits_preserve_exact_clinical_events_and_contributions_across_slices() {
    for (metabolism, tolerance) in [(500, -275), (1_500, 125), (1_137, 131)] {
        assert_eq!(
            advance_slices_with_physiology(&[170_003], metabolism, tolerance),
            advance_slices_with_physiology(
                &[1, 799, 13_003, 28_717, 3, 77_001, 50_479],
                metabolism,
                tolerance
            )
        );
    }
    let neutral = clinical_physiology(&Rules::fixture(), 1_000, 0).unwrap();
    assert_eq!(neutral.thirst_harm, Rules::fixture().thirst_harm);
    assert_eq!(neutral.drink_relief, Rules::fixture().drink_relief);
    assert_eq!(neutral.cold_tolerance_centi, 0);
}

use crate::*;
use game_core::*;
use std::collections::BTreeMap;
fn active<'a>(
    accepted: &'a BTreeMap<InvocationCost, Consent>,
    episode: Option<&'a Episode>,
) -> ActiveInput<'a> {
    ActiveInput {
        life_count: 1,
        accepted,
        episode,
        next_loss_burden: 300_000,
    }
}
fn input<'a>(
    rules: &'a Rules,
    before: &'a Burden,
    active: Option<ActiveInput<'a>>,
    now: u64,
    next: u64,
) -> AdvanceInput<'a> {
    AdvanceInput {
        rules,
        before,
        actor: PersonId(1),
        now: SimTime(now),
        next: SimTime(next),
        alive: true,
        active,
    }
}
#[test]
fn a_new_severity_requires_warning_before_bodily_cost() {
    let rules = Rules::fixture();
    let before = Burden {
        burden: 119_999,
        ..Burden::default()
    };
    let accepted = BTreeMap::from([(
        InvocationCost::Strain,
        Consent {
            temporary: 0,
            permanent: 0,
        },
    )]);
    let ctx = input(&rules, &before, Some(active(&accepted, None)), 0, 1);
    assert_eq!(body_contribution(ctx).unwrap(), BodyContribution::default());
    let advanced = advance_burden(ctx).unwrap();
    assert_eq!(advanced.burden.burden, 120_000);
    let warning = next_warning(
        &rules,
        1,
        &advanced.burden,
        300_000,
        &accepted,
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        warning,
        CostRequirement {
            cost: InvocationCost::Strain,
            severity: 1,
            permanent: false
        }
    );
    assert_eq!(
        body_rates(
            &rules,
            &advanced.burden,
            Some(active(&accepted, None)),
            PersonId(1),
            SimTime(1),
            true
        )
        .unwrap()
        .fatigue_per_mille,
        0
    );
    let accepted = accept_requirement(&accepted, warning);
    assert_eq!(
        body_rates(
            &rules,
            &advanced.burden,
            Some(active(&accepted, None)),
            PersonId(1),
            SimTime(1),
            true
        )
        .unwrap()
        .fatigue_per_mille,
        50
    );
}
#[test]
fn activation_pays_persistent_burden_without_creating_protracted_exposure() {
    let rules = Rules::fixture();
    let before = Burden::default();
    let accepted = BTreeMap::new();
    let projected = projected_activation(&rules, &before, 3).unwrap();
    assert_eq!(projected.burden, 54_000);
    assert_eq!(projected.exposure, 0);
    let warning = next_warning(
        &rules,
        3,
        &projected,
        rules.loss_threshold,
        &accepted,
        true,
        false,
    )
    .unwrap();
    assert_eq!(warning.severity, 0);
    assert_eq!(warning.cost, InvocationCost::Strain);
    assert_eq!(
        activation_charge(&rules, &before, 3).unwrap(),
        ActivationCharge {
            burden: 54_000,
            fatigue: 9_000
        }
    );
}
#[test]
fn explicit_loss_consent_authorizes_all_costs_for_exact_person_and_duration() {
    let rules = Rules::fixture();
    let before = Burden {
        burden: 900_000,
        exposure: 900_000,
        ..Burden::default()
    };
    assert!(
        begin_episode(
            &rules,
            &before,
            PersonId(1),
            SimTime(10),
            &Consent::default()
        )
        .is_err()
    );
    let started = begin_episode(
        &rules,
        &before,
        PersonId(1),
        SimTime(10),
        &Consent {
            temporary: 1,
            permanent: 0,
        },
    )
    .unwrap();
    assert_eq!(started.episode.ends, SimTime(420_010));
    let accepted = BTreeMap::new();
    let ctx = input(
        &rules,
        &before,
        Some(active(&accepted, Some(&started.episode))),
        10,
        20,
    );
    assert!(next_warning(&rules, 1, &before, 300_000, &accepted, false, true).is_none());
    assert_eq!(
        body_contribution(ctx).unwrap(),
        BodyContribution {
            health_milli: -5_000,
            fatigue_milli: 20_000
        }
    );
    assert!(controls_actor(
        Some(&started.episode),
        PersonId(1),
        SimTime(420_009),
        true
    ));
    assert!(!controls_actor(
        Some(&started.episode),
        PersonId(2),
        SimTime(20),
        true
    ));
    assert!(!controls_actor(
        Some(&started.episode),
        PersonId(1),
        SimTime(420_010),
        true
    ));
    assert!(
        advance_burden(input(
            &rules,
            &before,
            Some(active(&accepted, Some(&started.episode))),
            10,
            420_010
        ))
        .unwrap()
        .end_episode
    );
    assert!(next_warning(&rules, 1, &before, 300_000, &accepted, false, false).is_some());
}
#[test]
fn accepted_loss_has_no_protective_bodily_cap_or_health_dependency() {
    let rules = Rules::fixture();
    let before = Burden {
        burden: 900_000,
        exposure: 900_000,
        ..Burden::default()
    };
    let accepted = BTreeMap::new();
    let episode = Episode {
        actor: PersonId(1),
        starts: SimTime(0),
        ends: SimTime(420_000),
    };
    let ctx = input(
        &rules,
        &before,
        Some(ActiveInput {
            life_count: 8,
            ..active(&accepted, Some(&episode))
        }),
        0,
        100_000,
    );
    let contribution = body_contribution(ctx).unwrap();
    assert_eq!(contribution.health_milli, -3_200_000_000);
    assert_eq!(contribution.fatigue_milli, 12_800_000_000);
}
#[test]
fn recovery_keeps_exact_fractions_and_dismissal_does_not_reset_exposure() {
    let mut rules = Rules::fixture();
    rules.burden_recovery_per_mille = 333;
    rules.exposure_recovery_per_mille = 111;
    let before = Burden {
        burden: 480_000,
        exposure: 600_000,
        uninterrupted: 40_000,
        burden_remainder: 701,
        exposure_remainder: 777,
        temporary_identity: 10,
        emotional_disruption: 200,
        ..Burden::default()
    };
    let whole = advance_burden(input(&rules, &before, None, 0, 31_727))
        .unwrap()
        .burden;
    let mut split = before.clone();
    let mut now = 0;
    for delta in [1, 77, 15_001, 333, 16_315] {
        split = advance_burden(input(&rules, &split, None, now, now + delta))
            .unwrap()
            .burden;
        now += delta;
    }
    assert_eq!(whole, split);
    assert!(whole.exposure > 590_000);
    assert_eq!(whole.uninterrupted, 0);
    let accepted = BTreeMap::new();
    let resumed = advance_burden(input(
        &rules,
        &whole,
        Some(active(&accepted, None)),
        now,
        now + 1,
    ))
    .unwrap();
    assert_eq!(resumed.burden.exposure, whole.exposure + 1);
    assert_eq!(resumed.burden.exposure_remainder, whole.exposure_remainder);
}
#[test]
fn permanent_costs_require_both_severe_burden_and_very_long_exposure() {
    let rules = Rules::fixture();
    let short = Burden {
        burden: 9_000_000,
        exposure: rules.permanent_exposure.0 - 1,
        ..Burden::default()
    };
    let recovered = Burden {
        burden: rules.permanent_burden - 1,
        exposure: 9_000_000,
        ..Burden::default()
    };
    for before in [short, recovered] {
        assert!(
            required_bands(&rules, 8, &before, 300_000)
                .iter()
                .all(|band| !band.permanent)
        );
    }
    let before = Burden {
        burden: rules.permanent_burden,
        exposure: rules.permanent_exposure.0,
        ..Burden::default()
    };
    let permanent: Vec<_> = required_bands(&rules, 1, &before, 300_000)
        .into_iter()
        .filter(|band| band.permanent)
        .collect();
    assert_eq!(permanent.len(), 2);
    let identity = permanent
        .iter()
        .find(|b| b.cost == InvocationCost::Identity)
        .copied()
        .unwrap();
    let applied = apply_permanent(&rules, &before, identity).unwrap();
    assert!(applied.changed);
    assert_eq!(applied.burden.permanent_identity, 10);
    assert!(
        !apply_permanent(&rules, &applied.burden, identity)
            .unwrap()
            .changed
    );
    assert_eq!(Burden::default().permanent_identity, 0); // A different actual person's initial state.
}
#[test]
fn cost_and_controller_boundaries_are_pure_and_explicit() {
    let rules = Rules::fixture();
    let before = Burden {
        burden: 119_997,
        ..Burden::default()
    };
    assert_eq!(
        next_cost_after(&rules, &before, 2, 300_000),
        Some(SimDuration(1))
    );
    let accepted = BTreeMap::new();
    let episode = Episode {
        actor: PersonId(1),
        starts: SimTime(0),
        ends: SimTime(420_000),
    };
    let before = Burden::default();
    assert_eq!(
        active_boundary(
            &rules,
            &before,
            active(&accepted, Some(&episode)),
            SimTime(0),
            SimTime(6_000),
            SimTime(60_000)
        ),
        SimTime(6_000)
    );
    let recovering = Burden {
        burden: 240_000,
        ..Burden::default()
    };
    assert_eq!(
        recovery_boundary(&rules, &recovering, SimTime(0), SimTime(60_000)),
        SimTime(1)
    );
}
#[test]
fn current_state_roundtrips_without_body_fraction_fields() {
    let before = Burden {
        burden: 31,
        exposure: 80,
        burden_remainder: 37,
        exposure_remainder: 901,
        permanent_identity: 10,
        lifespan_loss: 60_000,
        ..Burden::default()
    };
    let json = serde_json::to_value(&before).unwrap();
    assert!(json.get("health_remainder").is_none());
    assert!(json.get("fatigue_remainder").is_none());
    assert_eq!(serde_json::from_value::<Burden>(json).unwrap(), before);
    validate_rules(&Rules::human()).unwrap();
    validate_rules(&Rules::fixture()).unwrap();
}
#[test]
fn overflow_is_detected_before_any_owned_state_changes() {
    let rules = Rules::fixture();
    let before = Burden {
        burden: u64::MAX,
        ..Burden::default()
    };
    let accepted = BTreeMap::new();
    assert!(advance_burden(input(&rules, &before, Some(active(&accepted, None)), 0, 1)).is_err());
    assert_eq!(before.burden, u64::MAX);
    assert!(growth_rate(usize::MAX).is_err());
}

#[test]
fn borrowed_work_and_identity_penalties_keep_the_frozen_body_and_fractional_units() {
    let body = game_core::SkillRate {
        numerator: 500,
        denominator: 1_000,
    };
    let own = game_core::SkillRate {
        numerator: 500_000,
        denominator: 1_000_000,
    };
    let rate = compose_work_rate(own, body, 2_000, 100).unwrap();
    assert_eq!(
        rate,
        game_core::SkillRate {
            numerator: 750_000,
            denominator: 1_000_000
        }
    );
    assert_eq!(
        compose_work_rate(own, body, 0, 100).unwrap().numerator,
        250_000
    );
    assert_eq!(
        compose_work_rate(own, body, 0, 1_000).unwrap().numerator,
        500
    );
    let neutral = game_core::SkillRate {
        numerator: 1_000,
        denominator: 1_000,
    };
    assert_eq!(
        compose_work_rate(neutral, neutral, 2_000, 100).unwrap(),
        game_core::SkillRate {
            numerator: 1_500,
            denominator: 1_000
        }
    );
}

use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
fn initial() -> World {
    World {
        incarnations: vec![Incarnation {
            person: PersonId(1),
            bound_at: SimTime(0),
            died_at: None,
        }],
        waiting: None,
        guardians: BTreeMap::from([(
            PersonId(2),
            Guardian {
                target: Some(PersonId(1)),
                sensed_location: Some(Location::AtArea(AreaId(1))),
                sensed_at: SimTime(0),
                goal: GuardianGoal::Mentoring,
                explanation: "Present".into(),
                assessments: BTreeMap::new(),
                assignment: Some(Assignment {
                    request: GuardianRequest::Work { site: SiteId(7) },
                    accepted_at: SimTime(0),
                }),
                provision: None,
                requested_delivery: None,
                knows_invocation: true,
            },
        )]),
        rules: MentorshipRules::default(),
        invocation_learning: BTreeMap::new(),
    }
}
fn birth(person: u64, born: u64) -> ActualBirth {
    ActualBirth {
        person: PersonId(person),
        born: SimTime(born),
        initial_age: SimDuration(0),
        parent_count: 2,
        alive: true,
        location: Location::AtArea(AreaId(3)),
    }
}
fn bold() -> GuardianTraits {
    GuardianTraits {
        discipline: 0,
        patience: 0,
        risk_tolerance: 1_000,
    }
}
fn cautious() -> GuardianTraits {
    GuardianTraits {
        discipline: 1_000,
        patience: 1_000,
        risk_tolerance: 0,
    }
}
#[test]
fn a_living_incarnation_and_possible_births_do_not_trigger_succession() {
    let before = initial();
    let unchanged = observe_death(&before, true, SimTime(10), &BTreeMap::new());
    assert_eq!(before, unchanged);
    assert!(bind_actual_birth(&before, &[birth(10, 20)], SimTime(20), &BTreeMap::new()).is_none());
    let waiting = observe_death(&before, false, SimTime(10), &BTreeMap::new());
    assert!(bind_actual_birth(&waiting, &[], SimTime(100_000), &BTreeMap::new()).is_none());
    assert!(
        bind_actual_birth(
            &waiting,
            &[birth(10, 100_001)],
            SimTime(100_000),
            &BTreeMap::new()
        )
        .is_none()
    );
}
#[test]
fn birth_must_be_actual_fresh_alive_and_strictly_after_death() {
    let waiting = observe_death(&initial(), false, SimTime(20), &BTreeMap::new());
    for candidate in [
        birth(10, 19),
        birth(10, 20),
        ActualBirth {
            initial_age: SimDuration(1),
            ..birth(10, 21)
        },
        ActualBirth {
            parent_count: 0,
            ..birth(10, 21)
        },
        ActualBirth {
            alive: false,
            ..birth(10, 21)
        },
    ] {
        assert!(
            bind_actual_birth(
                &waiting,
                std::slice::from_ref(&candidate),
                candidate.born,
                &BTreeMap::new()
            )
            .is_none()
        );
    }
    let bound =
        bind_actual_birth(&waiting, &[birth(10, 21)], SimTime(21), &BTreeMap::new()).unwrap();
    assert_eq!(bound.predecessor, PersonId(1));
    assert_eq!(bound.successor, PersonId(10));
    assert_eq!(bound.lineage.current(), Some(PersonId(10)));
    assert!(bound.lineage.waiting.is_none());
}
#[test]
fn simultaneous_actual_births_choose_persistent_order_independent_of_input_order() {
    let waiting = observe_death(&initial(), false, SimTime(20), &BTreeMap::new());
    let a = [birth(12, 21), birth(11, 21)];
    let b = [birth(11, 21), birth(12, 21)];
    let first = bind_actual_birth(&waiting, &a, SimTime(21), &BTreeMap::new()).unwrap();
    let second = bind_actual_birth(&waiting, &b, SimTime(21), &BTreeMap::new()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.successor, PersonId(11));
    assert!(
        bind_actual_birth(
            &first.lineage,
            &[birth(13, 21)],
            SimTime(21),
            &BTreeMap::new()
        )
        .is_none()
    );
}
#[test]
fn connection_changes_target_without_moving_or_releasing_a_guardian() {
    let constraints = BTreeMap::from([(PersonId(2), Some(GuardianGoal::Confined))]);
    let waiting = observe_death(&initial(), false, SimTime(20), &constraints);
    let guard = &waiting.guardians[&PersonId(2)];
    assert_eq!(guard.goal, GuardianGoal::Confined);
    assert!(guard.assignment.is_none());
    assert!(guard.sensed_location.is_none());
    let binding = bind_actual_birth(&waiting, &[birth(10, 21)], SimTime(21), &constraints).unwrap();
    let guard = &binding.lineage.guardians[&PersonId(2)];
    assert_eq!(guard.goal, GuardianGoal::Confined);
    assert_eq!(guard.target, Some(PersonId(10)));
    assert_eq!(guard.sensed_location, Some(Location::AtArea(AreaId(3))));
    // No body, stock, estate or office state exists in this domain result.
    assert_eq!(
        guardian_constraint(true, true, true),
        Some(GuardianGoal::Confined)
    );
}
#[test]
fn actual_witnessing_is_required_and_local_age_does_not_invent_competence() {
    let before = Assessment::default();
    let event = Observation {
        conduct: Conduct::Production,
        stage: LifeStage::Child,
        at: SimTime(50),
        witnessed: false,
    };
    assert_eq!(observe_conduct(&before, bold(), event), before);
    let stage = observe_stage(&before, bold(), LifeStage::Adult, SimTime(99), true);
    assert!(!stage.ready);
    assert_eq!(stage.competence, 0);
    let witnessed = observe_conduct(
        &before,
        bold(),
        Observation {
            witnessed: true,
            ..event
        },
    );
    assert_eq!(witnessed.competence, 100);
    assert_eq!(witnessed.observed_at, SimTime(50));
}
#[test]
fn guardians_disagree_about_the_same_conduct_and_visible_risk() {
    let mut evidence = Assessment::default();
    for conduct in [Conduct::Production, Conduct::Arrival] {
        evidence = observe_conduct(
            &evidence,
            bold(),
            Observation {
                conduct,
                stage: LifeStage::Child,
                at: SimTime(60),
                witnessed: true,
            },
        );
    }
    assert!(judge_readiness(&evidence, bold()).ready);
    assert!(!judge_readiness(&evidence, cautious()).ready);
    let risky = observe_conduct(
        &evidence,
        cautious(),
        Observation {
            conduct: Conduct::Departure {
                known_distress: true,
            },
            stage: LifeStage::Child,
            at: SimTime(61),
            witnessed: true,
        },
    );
    assert_eq!(risky.risks, 100);
    assert_eq!(risky.judgment, evidence.judgment);
    let infant = observe_stage(&evidence, bold(), LifeStage::Infant, SimTime(62), true);
    assert!(!infant.ready);
}
#[test]
fn readiness_never_compels_leaving_care_or_overriding_physical_duties() {
    let ready = RequestInput {
        constrained: false,
        in_combat: false,
        physical_duty_pending: false,
        ready: true,
        known_distress: false,
    };
    let movement = GuardianRequest::Move {
        destination: AreaId(8),
    };
    assert!(judge_request(&movement, ready).is_ok());
    assert_eq!(
        judge_request(
            &movement,
            RequestInput {
                known_distress: true,
                ..ready
            }
        ),
        Err(Refusal::ImmediateCare)
    );
    assert_eq!(
        judge_request(
            &movement,
            RequestInput {
                physical_duty_pending: true,
                ..ready
            }
        ),
        Err(Refusal::CurrentDuty)
    );
    assert_eq!(
        judge_request(
            &movement,
            RequestInput {
                ready: false,
                ..ready
            }
        ),
        Err(Refusal::Unready)
    );
    assert!(
        judge_request(
            &GuardianRequest::Rest,
            RequestInput {
                ready: false,
                ..ready
            }
        )
        .is_ok()
    );
}
#[test]
fn current_lineage_state_roundtrips_without_coordinator_or_compatibility_defaults() {
    let world = observe_death(&initial(), false, SimTime(20), &BTreeMap::new());
    let json = serde_json::to_vec(&world).unwrap();
    assert_eq!(serde_json::from_slice::<World>(&json).unwrap(), world);
    assert!(serde_json::from_str::<World>("{}").is_err());
}

#[test]
fn sensed_location_is_only_an_intent_and_care_uses_personal_fresh_knowledge() {
    let own = Location::AtArea(AreaId(1));
    let distant = Location::AtArea(AreaId(3));
    let base = GuardianInput {
        target: PersonId(9),
        own_location: &own,
        target_location: &distant,
        now: SimTime(40),
        can_reconsider: true,
        should_rest: true,
        ready: false,
        need: None,
    };
    assert_eq!(
        guardian_intention(base.clone()).action,
        Some(GuardianAction::Move {
            destination: AreaId(3)
        })
    );
    assert_eq!(own, Location::AtArea(AreaId(1)));
    let local = GuardianInput {
        target_location: &own,
        need: Some(NeedObservation {
            location: Location::AtArea(AreaId(1)),
            observed_at: SimTime(40),
            urgency: 50,
            hydration_urgency: 0,
            preference: None,
        }),
        ..base
    };
    assert_eq!(guardian_intention(local.clone()).action, None);
    let stale = GuardianInput {
        need: Some(NeedObservation {
            observed_at: SimTime(39),
            ..local.need.clone().unwrap()
        }),
        ..local.clone()
    };
    assert_eq!(guardian_intention(stale).action, Some(GuardianAction::Rest));
    let company = GuardianInput {
        need: Some(NeedObservation {
            urgency: 0,
            preference: Some(ChildhoodInterest::Company),
            ..local.need.unwrap()
        }),
        ..local
    };
    assert_eq!(
        guardian_intention(company).action,
        Some(GuardianAction::Comfort { child: PersonId(9) })
    );
}

#[test]
fn observed_care_requirements_persist_for_food_and_water_and_only_receipts_credit_delivery() {
    for (good, food, water) in [(Good::Food, 3_000, 0), (Good::Water, 0, 3_000)] {
        let target = PersonId(9);
        let dose = if good == Good::Water {
            Quantity(250)
        } else {
            Quantity(1_000)
        };
        let need = NeedObservation {
            location: Location::AtArea(AreaId(1)),
            observed_at: SimTime(40),
            urgency: food,
            hydration_urgency: water,
            preference: None,
        };
        assert!(
            review_care_provision(
                None,
                target,
                SimTime(40),
                false,
                Some(&need),
                CareDoses {
                    food: Quantity(1_000),
                    water: Quantity(250)
                }
            )
            .is_none()
        );
        assert!(
            review_care_provision(
                None,
                target,
                SimTime(41),
                true,
                Some(&need),
                CareDoses {
                    food: Quantity(1_000),
                    water: Quantity(250)
                }
            )
            .is_none()
        );
        let provision = review_care_provision(
            None,
            target,
            SimTime(40),
            true,
            Some(&need),
            CareDoses {
                food: Quantity(1_000),
                water: Quantity(250),
            },
        )
        .unwrap();
        assert_eq!(provision.delivery.requirement.good, good);
        assert_eq!(provision.delivery.requirement.quantity, dose);
        let continued = review_care_provision(
            Some(&provision),
            target,
            SimTime(70),
            false,
            None,
            CareDoses {
                food: Quantity(1_000),
                water: Quantity(250),
            },
        )
        .unwrap();
        assert_eq!(continued, provision);
        let mut world = initial();
        world.guardians.get_mut(&PersonId(2)).unwrap().provision = Some(provision);
        assert!(
            !world
                .record_care_receipt(PersonId(2), target, Good::Tool, Quantity(1_000), 1)
                .unwrap()
        );
        assert!(
            world
                .record_care_receipt(PersonId(2), target, good, Quantity(dose.0 / 2), 2)
                .unwrap()
        );
        assert_eq!(
            world.guardians[&PersonId(2)]
                .provision
                .as_ref()
                .unwrap()
                .delivery
                .remaining,
            Quantity(dose.0 / 2)
        );
        assert!(
            world
                .record_care_receipt(PersonId(2), target, good, Quantity(dose.0 / 2), 3)
                .unwrap()
        );
        assert!(
            review_care_provision(
                world.guardians[&PersonId(2)].provision.as_ref(),
                target,
                SimTime(80),
                false,
                None,
                CareDoses {
                    food: Quantity(1_000),
                    water: Quantity(250)
                }
            )
            .is_none()
        );
        assert!(
            review_care_provision(
                Some(&continued),
                PersonId(10),
                SimTime(80),
                false,
                None,
                CareDoses {
                    food: Quantity(1_000),
                    water: Quantity(250)
                }
            )
            .is_none()
        );
    }
}
#[test]
fn care_goal_composes_with_the_same_generic_delivery_for_food_and_water() {
    use game_logistics::*;
    for (good, food, water) in [(Good::Food, 3_000, 0), (Good::Water, 0, 3_000)] {
        let target = PersonId(9);
        let dose = if good == Good::Water {
            Quantity(250)
        } else {
            Quantity(1_000)
        };
        let location = Location::AtArea(AreaId(1));
        let need = NeedObservation {
            location: location.clone(),
            observed_at: SimTime(40),
            urgency: food,
            hydration_urgency: water,
            preference: None,
        };
        let provision = review_care_provision(
            None,
            target,
            SimTime(40),
            true,
            Some(&need),
            CareDoses {
                food: Quantity(1_000),
                water: Quantity(250),
            },
        )
        .unwrap();
        let stocks = [KnownStock {
            inventory: InventoryId(20),
            good,
            quantity: dose,
            accessible: true,
            location: location.clone(),
            observed_at: SimTime(40),
            room: None,
        }];
        let destination = KnownDestination {
            destination: Destination::Contact(target),
            location: location.clone(),
            observed_at: SimTime(40),
            accessible: true,
            room: None,
            direct_transfer: true,
        };
        let contacts = BTreeSet::from([target]);
        let routes = BTreeMap::new();
        let prerequisite_room = BTreeMap::new();
        let input = Input::<u8> {
            now: SimTime(40),
            place: &location,
            free: true,
            can_travel: true,
            room: Quantity(0),
            prerequisite_room: &prerequisite_room,
            minimum_handoff: dose,
            cargo: &[],
            stocks: &stocks,
            destination: Some(&destination),
            contacts: &contacts,
            routes: &routes,
            offers: &[],
        };
        assert_eq!(
            provision.delivery.plan(&input),
            Step::Arrived {
                source: InventoryId(20),
                quantity: dose
            }
        );
        assert_eq!(provision.delivery.remaining, dose);
    }
}

#[test]
fn locally_observed_care_inapplicability_clears_autonomous_care_and_preserves_requested_supply() {
    let mut world = initial();
    let target = PersonId(1);
    let need = NeedObservation {
        location: Location::AtArea(AreaId(1)),
        observed_at: SimTime(0),
        urgency: 3_000,
        hydration_urgency: 0,
        preference: None,
    };
    let goal = review_care_provision(
        None,
        target,
        SimTime(0),
        true,
        Some(&need),
        CareDoses {
            food: Quantity(1_000),
            water: Quantity(250),
        },
    )
    .unwrap();
    let requested = RequestedDelivery {
        delivery: game_logistics::Delivery::new(
            game_logistics::Requirement {
                good: Good::Timber,
                quantity: Quantity(1_000),
                source: None,
            },
            game_logistics::Destination::Contact(target),
        )
        .unwrap(),
        accepted_at: SimTime(0),
        cargo: BTreeMap::new(),
        last_event: 0,
    };
    let guardian = world.guardians.get_mut(&PersonId(2)).unwrap();
    guardian.provision = Some(goal);
    guardian.requested_delivery = Some(requested.clone());
    world.observe_care_unavailable(PersonId(2));
    assert!(world.guardians[&PersonId(2)].provision.is_none());
    assert_eq!(
        world.guardians[&PersonId(2)].requested_delivery,
        Some(requested)
    );
}

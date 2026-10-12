use super::*;
use game_core::*;
use std::collections::BTreeMap;
fn world() -> World {
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
                sensed_location: None,
                sensed_at: SimTime(0),
                goal: GuardianGoal::Mentoring,
                explanation: String::new(),
                assessments: BTreeMap::from([(
                    PersonId(1),
                    Assessment {
                        ready: true,
                        ..Assessment::default()
                    },
                )]),
                assignment: None,
                requested_delivery: None,
                provision: None,
                knows_invocation: true,
            },
        )]),
        rules: MentorshipRules::default(),
        invocation_learning: BTreeMap::new(),
    }
}
fn facts() -> GuardianServiceFacts {
    GuardianServiceFacts {
        actor: PersonId(2),
        sponsor: PersonId(1),
        guardian: true,
        care_urgency: 0,
        hydration_urgency: 0,
    }
}
#[test]
fn individual_readiness_and_signals_are_required_before_guardian_outside_service() {
    let mut world = world();
    assert!(!world.guardian_service_due(facts()));
    assert!(world.guardian_service_due(GuardianServiceFacts {
        care_urgency: 1,
        ..facts()
    }));
    assert!(world.guardian_service_due(GuardianServiceFacts {
        hydration_urgency: 1,
        ..facts()
    }));
    assert!(!world.guardian_service_due(GuardianServiceFacts {
        care_urgency: -1,
        hydration_urgency: -1,
        ..facts()
    }));
    world
        .guardians
        .get_mut(&PersonId(2))
        .unwrap()
        .assessments
        .clear();
    assert!(world.guardian_service_due(facts()));
    assert!(!world.guardian_service_due(GuardianServiceFacts {
        guardian: false,
        ..facts()
    }));
    world.incarnations[0].died_at = Some(SimTime(1));
    assert!(world.guardian_service_due(facts()));
}

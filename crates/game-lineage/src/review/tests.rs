use super::*;
use game_core::{AreaId, Good, InventoryId, Quantity};
use std::collections::BTreeMap;

fn facts(location: &Location) -> ReviewFacts<'_> {
    ReviewFacts {
        now: SimTime(10),
        location,
        activity: GuardianActivity::Idle,
        confined: false,
        incapacitated: false,
        combat_restricted: false,
        in_combat: false,
        serving_objective: false,
    }
}

#[test]
fn confinement_and_recovery_override_competing_guardian_duties() {
    let location = Location::AtArea(AreaId(1));
    let competing = ReviewFacts {
        confined: true,
        incapacitated: true,
        in_combat: true,
        serving_objective: true,
        ..facts(&location)
    };
    assert_eq!(
        review_priority(Some(PersonId(2)), true, false, competing),
        ReviewStep::Escape
    );
    assert_eq!(
        review_priority(
            Some(PersonId(2)),
            true,
            false,
            ReviewFacts {
                confined: false,
                ..competing
            }
        ),
        ReviewStep::Recover
    );
    assert_eq!(
        review_priority(None, false, true, facts(&location)),
        ReviewStep::Escape
    );
}

#[test]
fn escape_precedes_delivery_and_occupied_delivery_precedes_assistance() {
    assert_eq!(
        review_disposition(false, DeliveryAttempt::NotAttempted),
        ReviewDisposition::AttemptDelivery
    );
    assert_eq!(
        review_disposition(false, DeliveryAttempt::Occupied),
        ReviewDisposition::Complete
    );
    assert_eq!(
        review_disposition(false, DeliveryAttempt::Unavailable),
        ReviewDisposition::ContinueReview
    );
    for delivery in [
        DeliveryAttempt::NotAttempted,
        DeliveryAttempt::Occupied,
        DeliveryAttempt::Unavailable,
    ] {
        assert_eq!(
            review_disposition(true, delivery),
            ReviewDisposition::ContinueReview
        );
    }
}

#[test]
fn encounter_and_accepted_objective_precede_assignment_and_assistance() {
    let location = Location::AtArea(AreaId(1));
    let competing = ReviewFacts {
        in_combat: true,
        serving_objective: true,
        ..facts(&location)
    };
    assert_eq!(
        review_priority(Some(PersonId(2)), true, false, competing),
        ReviewStep::ResolveEncounter
    );
    assert_eq!(
        review_priority(
            Some(PersonId(2)),
            true,
            false,
            ReviewFacts {
                in_combat: false,
                ..competing
            }
        ),
        ReviewStep::ServeObjective
    );
    assert_eq!(
        review_priority(Some(PersonId(2)), true, false, facts(&location)),
        ReviewStep::FinishAssignment
    );
    assert_eq!(
        review_priority(Some(PersonId(2)), false, false, facts(&location)),
        ReviewStep::Assist(PersonId(2))
    );
    assert_eq!(
        review_priority(None, false, false, facts(&location)),
        ReviewStep::Wait { rest: true }
    );
}

#[test]
fn assignment_completion_uses_physical_position_work_and_acceptance_time() {
    let location = Location::AtArea(AreaId(1));
    let assignment = |request| Assignment {
        request,
        accepted_at: SimTime(10),
    };
    assert!(assignment_pending(
        Some(&assignment(GuardianRequest::Move {
            destination: AreaId(2)
        })),
        facts(&location)
    ));
    assert!(!assignment_pending(
        Some(&assignment(GuardianRequest::Move {
            destination: AreaId(1)
        })),
        facts(&location)
    ));
    assert!(assignment_pending(
        Some(&assignment(GuardianRequest::Work { site: SiteId(3) })),
        ReviewFacts {
            activity: GuardianActivity::Work(SiteId(3)),
            ..facts(&location)
        }
    ));
    assert!(!assignment_pending(
        Some(&assignment(GuardianRequest::Work { site: SiteId(3) })),
        facts(&location)
    ));
    let rest = assignment(GuardianRequest::Rest);
    assert!(assignment_pending(Some(&rest), facts(&location)));
    assert!(!assignment_pending(
        Some(&rest),
        ReviewFacts {
            now: SimTime(11),
            ..facts(&location)
        }
    ));
    assert!(!assignment_pending(
        Some(&assignment(GuardianRequest::Supply {
            good: Good::Food,
            source: InventoryId(4),
            quantity: Quantity(1)
        })),
        facts(&location)
    ));
}

#[test]
fn observation_retains_pending_assignment_and_failed_attempt_retains_the_target() {
    let mut guardian = crate::Guardian {
        target: None,
        sensed_location: None,
        sensed_at: SimTime(0),
        goal: GuardianGoal::Waiting,
        explanation: String::new(),
        assessments: BTreeMap::new(),
        assignment: Some(Assignment {
            request: GuardianRequest::Rest,
            accepted_at: SimTime(10),
        }),
        requested_delivery: None,
        provision: None,
        knows_invocation: false,
    };
    guardian.observe_review(ReviewObservation {
        target: Some(PersonId(2)),
        sensed_location: Some(Location::AtArea(AreaId(1))),
        now: SimTime(10),
        pending_assignment: true,
        goal: GuardianGoal::Mentoring,
        explanation: ReviewStep::FinishAssignment.explanation(false).into(),
    });
    assert!(guardian.assignment.is_some());
    guardian.observe_failed_attempt();
    assert_eq!(guardian.target, Some(PersonId(2)));
    assert_eq!(guardian.goal, GuardianGoal::Mentoring);
    assert!(guardian.assignment.is_some());
    guardian.observe_review(ReviewObservation {
        target: None,
        sensed_location: None,
        now: SimTime(11),
        pending_assignment: false,
        goal: GuardianGoal::Waiting,
        explanation: ReviewStep::Wait { rest: true }.explanation(false).into(),
    });
    assert!(guardian.assignment.is_none());
    assert_eq!(guardian.target, None);
    assert_eq!(guardian.sensed_location, None);
    assert_eq!(guardian.sensed_at, SimTime(11));
}

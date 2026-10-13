use game_core::{AreaId, Error, Location, PersonId, SimDuration, SimTime, Vitals};
use game_physiology::{
    Body, PersonalActionFacts, PersonalActionKind as Kind, Rules, prepare_personal_action_area,
};

fn body() -> Body {
    Body {
        vitals: Vitals {
            alive: true,
            health: 100,
            hunger: 0,
            fatigue: 0,
        },
        fatigue_remainder: 0,
        health_remainder: 0,
    }
}
const KINDS: [Kind; 12] = [
    Kind::Signal,
    Kind::Play,
    Kind::Rest,
    Kind::Cancel,
    Kind::Release,
    Kind::Comfort,
    Kind::Care,
    Kind::Tell,
    Kind::Eat,
    Kind::Load,
    Kind::Work,
    Kind::Other,
];
#[test]
fn real_conflict_and_physiology_admission_preserve_every_location_and_agency_class() {
    let body = body();
    let stopped = Location::StoppedOnRoad {
        from: AreaId(1),
        to: AreaId(2),
        progress: SimDuration(1),
        duration: SimDuration(10),
    };
    let travelling = Location::InTransit {
        from: AreaId(1),
        to: AreaId(2),
        departed: SimTime(0),
        arrives: SimTime(10),
        road_start: None,
    };
    for kind in KINDS {
        let confined_allowed = matches!(kind, Kind::Rest | Kind::Eat | Kind::Tell | Kind::Release);
        assert_eq!(
            game_conflict::physical_action_allowed(true, false, kind),
            confined_allowed
        );
        for in_combat in [false, true] {
            let conflict_allowed = game_conflict::physical_action_allowed(false, in_combat, kind);
            let admission = prepare_personal_action_area(
                kind,
                PersonalActionFacts {
                    conflict_allowed,
                    independent: true,
                    receiving_care: false,
                    body: Some(&body),
                },
                || Location::AtArea(AreaId(1)),
            );
            assert_eq!(
                admission,
                if in_combat {
                    Err(Error::Unavailable)
                } else {
                    Ok(AreaId(1))
                }
            );
        }
        for (location, allowed) in [
            (
                stopped.clone(),
                matches!(
                    kind,
                    Kind::Cancel
                        | Kind::Rest
                        | Kind::Eat
                        | Kind::Load
                        | Kind::Care
                        | Kind::Comfort
                        | Kind::Release
                        | Kind::Signal
                ),
            ),
            (
                travelling.clone(),
                matches!(kind, Kind::Signal | Kind::Cancel),
            ),
            (
                Location::Carried {
                    carrier: PersonId(2),
                },
                false,
            ),
        ] {
            let admission = prepare_personal_action_area(
                kind,
                PersonalActionFacts {
                    conflict_allowed: true,
                    independent: true,
                    receiving_care: false,
                    body: Some(&body),
                },
                || location,
            );
            assert_eq!(
                admission,
                if allowed {
                    Ok(AreaId(1))
                } else {
                    Err(Error::Unavailable)
                }
            );
        }
        for (independent, receiving_care) in [(false, false), (true, true)] {
            let admission = prepare_personal_action_area(
                kind,
                PersonalActionFacts {
                    conflict_allowed: true,
                    independent,
                    receiving_care,
                    body: Some(&body),
                },
                || Location::AtArea(AreaId(1)),
            );
            assert_eq!(
                admission,
                if matches!(kind, Kind::Signal | Kind::Cancel) {
                    Ok(AreaId(1))
                } else {
                    Err(Error::Unavailable)
                }
            );
        }
    }
}
#[test]
fn rejected_capability_never_resolves_location_and_release_deadline_is_checked() {
    let mut body = body();
    body.vitals.health = 0;
    for body in [None, Some(&body)] {
        assert_eq!(
            prepare_personal_action_area(
                Kind::Rest,
                PersonalActionFacts {
                    conflict_allowed: true,
                    independent: true,
                    receiving_care: false,
                    body
                },
                || panic!("rejected body location binding")
            ),
            Err(Error::Unavailable)
        );
    }
    let rules = Rules {
        hunger_harm: 100,
        fatigue_limit: 100,
        max_health: 100,
        meal_relief: 10,
        meal_size: game_core::Quantity(1),
        eating_time: SimDuration(2),
        loading_time: SimDuration(3),
        carrying_capacity: game_core::Quantity(5),
    };
    game_conflict::require_release_admission(true).unwrap();
    assert_eq!(rules.unbind_deadline(SimTime(7)), Ok(SimTime(10)));
    assert_eq!(
        rules.unbind_deadline(SimTime(u64::MAX)),
        Err(Error::Overflow)
    );
    assert_eq!(
        game_conflict::require_release_admission(false),
        Err(Error::Unavailable)
    );
}

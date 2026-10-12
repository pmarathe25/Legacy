use super::*;
use crate::fixture::Fixture;
use crate::transition_tests::*;
use game_conflict::ObstacleView;
fn world(objective: MissionObjective) -> Fixture {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.objective = objective;
    start(&mut w, p, &f);
    accept(&mut w, A, OFFER, &f);
    w
}
fn map() -> Map {
    Map {
        id: CombatMapId(1),
        version: 1,
        area: HOME,
        location: Location::AtArea(HOME),
        width: 20_000,
        height: 20_000,
        obstacles: Vec::new(),
        exits: vec![(AWAY, Point { x: 0, y: 1000 })],
    }
}
fn fighter(actor: PersonId, position: Point, team: u32) -> Fighter {
    Fighter {
        actor,
        joins_round: 0,
        team,
        position,
        surrendered: false,
        withdrawn: false,
        defending: false,
        retreat: None,
        known: BTreeMap::new(),
        search: SearchState::default(),
    }
}
struct Facts(TacticalCapabilities);
impl TacticalFacts for Facts {
    fn capabilities(&self, _: PersonId) -> TacticalCapabilities {
        self.0
    }
}
fn capabilities() -> Facts {
    Facts(TacticalCapabilities {
        release_available: false,
        target_can_fight: true,
        target_captive: false,
    })
}
fn input<'a>(
    map: &'a Map,
    roster: &'a BTreeMap<PersonId, Fighter>,
    routes: &'a KnownRoutes,
) -> TacticalInput<'a> {
    TacticalInput {
        actor: A,
        player: false,
        area: HOME,
        map,
        fighter: &roster[&A],
        roster,
        routes,
        health: 100,
        max_health: 100,
        risk_tolerance: 500,
        movement: 5000,
        reach: 2000,
        binding_available: true,
    }
}
fn roster() -> BTreeMap<PersonId, Fighter> {
    let mut a = fighter(A, Point { x: 1000, y: 1000 }, 0);
    a.known.insert(ENEMY, Point { x: 2500, y: 1000 });
    BTreeMap::from([
        (A, a),
        (ENEMY, fighter(ENEMY, Point { x: 2500, y: 1000 }, 1)),
    ])
}
#[test]
fn retreat_uses_known_reachable_exits_or_surrenders_without_them() {
    let w = world(MissionObjective::Defeat { person: ENEMY });
    let map = map();
    let roster = roster();
    let routes = KnownRoutes::from([(HOME, BTreeMap::from([(AWAY, SimDuration(10))]))]);
    let result = w
        .tactical_decision(
            &w.store,
            ID,
            TacticalInput {
                health: 49,
                ..input(&map, &roster, &routes)
            },
            &capabilities(),
        )
        .unwrap();
    assert_eq!(result.action, TacticAction::Retreat(AWAY));
    let missing = KnownRoutes::new();
    let result = w
        .tactical_decision(
            &w.store,
            ID,
            TacticalInput {
                health: 49,
                ..input(&map, &roster, &missing)
            },
            &capabilities(),
        )
        .unwrap();
    assert_eq!(result.action, TacticAction::Surrender);
    assert!(
        w.tactical_decision(
            &w.store,
            ID,
            TacticalInput {
                player: true,
                ..input(&map, &roster, &routes)
            },
            &capabilities()
        )
        .is_none()
    );
}
#[test]
fn surrender_capture_and_release_remain_distinct_owned_choices() {
    let map = map();
    let mut roster = roster();
    let routes = KnownRoutes::new();
    roster.get_mut(&ENEMY).unwrap().surrendered = true;
    let w = world(MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(10),
    });
    assert_eq!(
        w.tactical_decision(&w.store, ID, input(&map, &roster, &routes), &capabilities())
            .unwrap()
            .action,
        TacticAction::Capture(ENEMY)
    );
    assert_eq!(
        w.tactical_decision(
            &w.store,
            ID,
            TacticalInput {
                binding_available: false,
                ..input(&map, &roster, &routes)
            },
            &capabilities()
        )
        .unwrap()
        .action,
        TacticAction::Defend
    );
    let w = world(MissionObjective::Rescue {
        person: ENEMY,
        return_to: AWAY,
    });
    let ready = Facts(TacticalCapabilities {
        release_available: true,
        ..capabilities().0
    });
    assert_eq!(
        w.tactical_decision(&w.store, ID, input(&map, &roster, &routes), &ready)
            .unwrap()
            .action,
        TacticAction::Release(ENEMY)
    );
}
#[test]
fn stale_remembered_point_does_not_authorize_attack_at_new_actual_position() {
    let w = world(MissionObjective::Defeat { person: ENEMY });
    let map = map();
    let mut roster = roster();
    let routes = KnownRoutes::new();
    assert_eq!(
        w.tactical_decision(&w.store, ID, input(&map, &roster, &routes), &capabilities())
            .unwrap()
            .action,
        TacticAction::Attack(ENEMY)
    );
    roster.get_mut(&ENEMY).unwrap().position = Point { x: 9000, y: 1000 };
    assert_eq!(
        w.tactical_decision(&w.store, ID, input(&map, &roster, &routes), &capabilities())
            .unwrap()
            .action,
        TacticAction::Defend
    );
}
#[test]
fn exploration_ignores_unseen_people_and_preserves_owner_search_progress() {
    let w = world(MissionObjective::Defeat { person: ENEMY });
    let mut map = map();
    map.obstacles = vec![ObstacleView {
        index: 0,
        minimum: Point { x: 5000, y: 4000 },
        maximum: Point { x: 6000, y: 8000 },
        integrity: 10,
    }];
    let mut roster = roster();
    roster.get_mut(&A).unwrap().known.clear();
    let routes = KnownRoutes::new();
    let first = w
        .tactical_decision(&w.store, ID, input(&map, &roster, &routes), &capabilities())
        .unwrap();
    assert!(first.search.is_some());
    assert_eq!(first.action, TacticAction::Defend);
    roster.get_mut(&ENEMY).unwrap().position = Point {
        x: 19_000,
        y: 19_000,
    };
    assert_eq!(
        w.tactical_decision(&w.store, ID, input(&map, &roster, &routes), &capabilities())
            .unwrap(),
        first
    );
    let mut search = roster[&A].search.clone();
    let step = map.search_step(roster[&A].position, 5000, &mut search);
    assert_eq!(first.movement, step);
    assert_eq!(first.search, Some(search));
}
#[test]
fn commitment_busy_thresholds_are_owned_and_preserve_signed_urgency() {
    let w = world(MissionObjective::Visit { destination: AWAY });
    let ready = ServiceAvailability {
        guardian_due: false,
        alive: true,
        health: 100,
        max_health: 100,
        hunger: 0,
        meal_relief: 100,
        fatigue: 0,
        fatigue_limit: 100,
        restricted: false,
        care_urgency: -1,
    };
    assert!(w.member_busy(&w.store, A, ready));
    assert!(!w.member_busy(
        &w.store,
        A,
        ServiceAvailability {
            guardian_due: true,
            ..ready
        }
    ));
    assert!(!w.member_busy(
        &w.store,
        A,
        ServiceAvailability {
            care_urgency: 20_000,
            ..ready
        }
    ));
    assert!(!w.member_busy(
        &w.store,
        A,
        ServiceAvailability {
            health: 50,
            ..ready
        }
    ));
}

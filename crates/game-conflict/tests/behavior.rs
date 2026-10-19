mod support;
use game_conflict::*;
use game_core::*;
use support::*;

#[test]
fn a_shared_round_advances_once_and_damage_persists() {
    let mut f = setup();
    let id = f
        .host
        .dispatch(Start(Battle {
            area: f.home,
            map: Map {
                width: 1000,
                obstacles: vec![],
            },
            fighters: std::collections::BTreeMap::from([
                (
                    f.ruler,
                    Fighter {
                        team: 1,
                        point: Point { x: 10, y: 10 },
                        health: 100,
                    },
                ),
                (
                    f.other,
                    Fighter {
                        team: 2,
                        point: Point { x: 20, y: 10 },
                        health: 100,
                    },
                ),
            ]),
            controlled: std::collections::BTreeSet::from([f.ruler]),
            orders: std::collections::BTreeMap::new(),
            round: 0,
            interval: SimDuration(6000),
            movement: 100,
            reach: 50,
            damage: 20,
            scheduled: None,
            finished: false,
        }))
        .unwrap();
    let paused = f.host.advance(SimTime(10_000), 100).unwrap();
    assert_eq!(paused.reached, SimTime(0));
    assert!(matches!(paused.stop, game_sim::Stop::Choice(_)));
    f.host
        .dispatch(Order {
            person: f.ruler,
            battle: id,
            action: OrderKind::Attack(f.other),
        })
        .unwrap();
    let p = f.host.advance(SimTime(10_000), 100).unwrap();
    assert_eq!(p.reached, SimTime(6000));
    assert_eq!(f.host.get::<Battles>().unwrap().0[&id].round, 1);
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.ruler].health,
        80
    );
    assert_eq!(
        f.host.get::<game_physiology::Bodies>().unwrap().people[&f.other].health,
        80
    );
    assert!(
        f.host
            .dispatch(game_transport::Move {
                person: f.ruler,
                destination: f.away
            })
            .is_err()
    );
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}

#[test]
fn newborn_victims_remain_vulnerable_but_never_auto_attack() {
    let mut f = setup();
    f.host
        .dispatch(game_vitality::Register {
            person: f.other,
            born: f.host.now(),
            span: Some(SimDuration(80 * 365 * DAY.0)),
            base_health: 100,
        })
        .unwrap();
    let battle = Battle {
        area: f.home,
        map: Map {
            width: 1000,
            obstacles: vec![],
        },
        fighters: [
            (
                f.ruler,
                Fighter {
                    team: 1,
                    point: Point { x: 10, y: 10 },
                    health: 100,
                },
            ),
            (
                f.other,
                Fighter {
                    team: 2,
                    point: Point { x: 20, y: 10 },
                    health: 100,
                },
            ),
        ]
        .into(),
        controlled: [f.other].into(),
        orders: Default::default(),
        round: 0,
        interval: MINUTE,
        movement: 100,
        reach: 50,
        damage: 10,
        scheduled: None,
        finished: false,
    };
    let id = f.host.dispatch(Start(battle)).unwrap();
    assert!(
        f.host
            .dispatch(Order {
                person: f.other,
                battle: id,
                action: OrderKind::Attack(f.ruler)
            })
            .is_err()
    );
    assert!(
        f.host
            .dispatch(Order {
                person: f.other,
                battle: id,
                action: OrderKind::Move(Point { x: 15, y: 10 })
            })
            .is_err()
    );
    f.host
        .dispatch(Order {
            person: f.other,
            battle: id,
            action: OrderKind::Wait,
        })
        .unwrap();
    f.host.advance(SimTime(MINUTE.0), 128).unwrap();
    let bodies = f.host.get::<game_physiology::Bodies>().unwrap();
    assert_eq!(bodies.people[&f.ruler].health, 100);
    assert!(bodies.people[&f.other].health < 100);
}

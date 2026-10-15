mod support;
use game_core::*;
use game_invocation::*;
use support::*;

fn prepare(f: &mut Fixture) -> LifeId {
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(game_physiology::Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    run(&mut f.host, SimTime(1));
    f.ruler = f
        .host
        .dispatch(game_lifecycle::Birth {
            mother: f.other,
            father: f.guardian,
            caregiver: f.other,
            area: f.home,
        })
        .unwrap();
    let life = *f
        .host
        .get::<game_lineage::Lineage>()
        .unwrap()
        .history
        .keys()
        .next()
        .unwrap();
    f.host
        .dispatch(Configure {
            costs: vec![
                CostAt {
                    after: SimDuration(10),
                    cost: Cost::ControlLoss,
                    amount: 1,
                    duration: SimDuration(100),
                },
                CostAt {
                    after: SimDuration(20),
                    cost: Cost::Strain,
                    amount: 20,
                    duration: SimDuration(0),
                },
                CostAt {
                    after: SimDuration(30),
                    cost: Cost::Identity,
                    amount: 5,
                    duration: SimDuration(0),
                },
            ],
            repeat: SimDuration(1000),
        })
        .unwrap();
    f.host
        .dispatch(Invoke {
            person: f.ruler,
            lives: std::collections::BTreeSet::from([life]),
        })
        .unwrap();
    life
}
#[test]
fn control_loss_is_binding_and_subsequent_costs_do_not_pause() {
    let mut f = setup();
    let life = prepare(&mut f);
    let p = f.host.advance(SimTime(200), 1000).unwrap();
    assert_eq!(p.reached, SimTime(11));
    assert!(matches!(p.stop, game_sim::Stop::Choice(_)));
    assert!(!memories(&f.host).unwrap().is_empty());
    f.host
        .submit(
            1,
            Consent {
                step: 0,
                accept: true,
            },
        )
        .unwrap();
    let save = f.host.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    assert!(loaded.submit(2, Dismiss).is_err());
    assert!(
        loaded
            .submit(3, game_physiology::Rest { person: f.ruler })
            .is_err()
    );
    run(&mut loaded, SimTime(31));
    assert_eq!(
        loaded.get::<game_physiology::Bodies>().unwrap().people[&f.ruler].fatigue,
        20
    );
    assert_eq!(loaded.get::<Invocation>().unwrap().identity, 5);
    assert!(loaded.get::<Invocation>().unwrap().warning.is_none());
    run(&mut loaded, SimTime(111));
    loaded.submit(4, Dismiss).unwrap();
    assert!(memories(&loaded).unwrap().is_empty());
    assert!(
        loaded
            .get::<game_lineage::Lineage>()
            .unwrap()
            .history
            .contains_key(&life)
    );
}
#[test]
fn dismissal_before_consent_prevents_a_new_cost() {
    let mut f = setup();
    prepare(&mut f);
    f.host.advance(SimTime(20), 100).unwrap();
    f.host
        .submit(
            1,
            Consent {
                step: 0,
                accept: false,
            },
        )
        .unwrap();
    run(&mut f.host, SimTime(200));
    assert_eq!(f.host.get::<Invocation>().unwrap().control_until, None);
    assert_eq!(f.host.get::<Invocation>().unwrap().identity, 0);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
#[test]
fn combat_continues_during_control_loss_and_stops_when_control_returns() {
    let mut f = setup();
    prepare(&mut f);
    let battle = f
        .host
        .dispatch(game_conflict::Start(game_conflict::Battle {
            area: f.home,
            map: game_conflict::Map {
                width: 1000,
                obstacles: vec![],
            },
            fighters: std::collections::BTreeMap::from([
                (
                    f.ruler,
                    game_conflict::Fighter {
                        team: 1,
                        point: game_conflict::Point { x: 10, y: 10 },
                        health: 100,
                    },
                ),
                (
                    f.other,
                    game_conflict::Fighter {
                        team: 2,
                        point: game_conflict::Point { x: 20, y: 10 },
                        health: 100,
                    },
                ),
            ]),
            controlled: std::collections::BTreeSet::from([f.ruler]),
            orders: std::collections::BTreeMap::new(),
            round: 0,
            interval: SimDuration(5),
            movement: 100,
            reach: 50,
            damage: 1,
            scheduled: None,
            finished: false,
        }))
        .unwrap();
    for _ in 0..3 {
        f.host
            .submit(
                f.host.next_sequence(),
                game_conflict::Order {
                    person: f.ruler,
                    battle,
                    action: game_conflict::OrderKind::Wait,
                },
            )
            .unwrap();
        f.host.advance(SimTime(200), 100).unwrap();
    }
    assert_eq!(f.host.now(), SimTime(11));
    assert!(f.host.get::<Invocation>().unwrap().warning.is_some());
    f.host
        .submit(
            10,
            Consent {
                step: 0,
                accept: true,
            },
        )
        .unwrap();
    let save = f.host.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    assert!(
        loaded
            .submit(
                11,
                game_conflict::Order {
                    person: f.ruler,
                    battle,
                    action: game_conflict::OrderKind::Wait
                }
            )
            .is_err()
    );
    let progress = loaded.advance(SimTime(200), 100).unwrap();
    assert_eq!(progress.reached, SimTime(111));
    assert_eq!(
        progress.stop,
        game_sim::Stop::Choice(format!("combat:{battle}"))
    );
    assert!(loaded.get::<game_conflict::Battles>().unwrap().0[&battle].round > 10);
    assert!(loaded.get::<game_physiology::Bodies>().unwrap().people[&f.ruler].health < 98);
    assert_eq!(loaded.get::<Invocation>().unwrap().identity, 5);
    assert!(loaded.get::<Invocation>().unwrap().warning.is_none());
    assert!(controlled(loaded.get().unwrap(), loaded.now()));
}

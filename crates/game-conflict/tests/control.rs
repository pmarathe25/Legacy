mod support;
use game_conflict::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Control(bool);
impl Resource for Control {
    const KEY: &'static str = "test.control";
}
#[derive(Clone, Serialize, Deserialize)]
struct SetControl(bool);
impl Request for SetControl {
    const KEY: &'static str = "test.set-control";
    type Response = ();
}
#[test]
fn waiting_index_does_not_cache_player_control_and_rebuilds_for_another_host() {
    fn setup() -> support::Fixture {
        let mut b = support::builder();
        b.plugin("test-control", 1).unwrap();
        b.resource::<Control>().unwrap();
        b.request::<HasPlayerControl>(|tx, _| Ok(tx.get::<Control>()?.0))
            .unwrap();
        b.request::<SetControl>(|tx, r| {
            tx.edit::<Control>()?.0 = r.0;
            Ok(())
        })
        .unwrap();
        support::setup_with(b)
    }
    let mut f = setup();
    f.host.dispatch(SetControl(true)).unwrap();
    f.host
        .dispatch(Start(Battle {
            area: f.home,
            map: Map {
                width: 100,
                obstacles: vec![],
            },
            fighters: BTreeMap::from([
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
            controlled: BTreeSet::from([f.ruler]),
            orders: BTreeMap::new(),
            round: 0,
            interval: SimDuration(6000),
            movement: 10,
            reach: 50,
            damage: 20,
            scheduled: None,
            finished: false,
        }))
        .unwrap();
    assert!(matches!(
        f.host.advance(SimTime(0), 1).unwrap().stop,
        Stop::Choice(_)
    ));
    let unchanged = f.host.get::<Battles>().unwrap().clone();
    f.host.dispatch(SetControl(false)).unwrap();
    assert_eq!(f.host.advance(SimTime(0), 1).unwrap().stop, Stop::Target);
    assert_eq!(*f.host.get::<Battles>().unwrap(), unchanged);
    // Another host with no waiting battle must not inherit the cached candidates.
    let mut other = setup();
    assert_eq!(
        other.host.advance(SimTime(0), 1).unwrap().stop,
        Stop::Target
    );
    f.host.dispatch(SetControl(true)).unwrap();
    assert!(matches!(
        f.host.advance(SimTime(0), 1).unwrap().stop,
        Stop::Choice(_)
    ));
}

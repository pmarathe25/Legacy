use game_core::*;
use game_physiology::*;
#[test]
fn hunger_forecasts_match_tick_evolution_including_saturation_and_clock_limits() {
    let body = Body {
        kind: Kind::Mortal,
        born: SimTime(0),
        alive: true,
        hunger: 10,
        fatigue: 0,
        health: 100,
        maximum_health: None,
        fatigue_fraction: 0,
        resting: false,
        last: SimTime(7),
        life_limit: None,
    };
    for rate in [0, 1, 7, u64::MAX] {
        for threshold in [0, 10, 11, 100, u64::MAX] {
            let rules = Rules {
                hunger_per_tick: rate,
                ..Rules::default()
            };
            if let Some(at) = hunger_threshold(&body, &rules, threshold) {
                assert!(advance_body(&body, &rules, at).hunger >= threshold);
                if at > body.last {
                    assert!(advance_body(&body, &rules, SimTime(at.0 - 1)).hunger < threshold);
                }
            }
        }
    }
    for kind in [Kind::Mortal, Kind::Guardian] {
        let unavailable = Body {
            kind,
            alive: kind == Kind::Guardian,
            ..body
        };
        assert_eq!(hunger_threshold(&unavailable, &Rules::default(), 20), None);
    }
}

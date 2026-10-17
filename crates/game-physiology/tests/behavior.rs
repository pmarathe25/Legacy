mod support;
use game_core::*;
use game_physiology::*;
use support::*;

#[test]
fn actual_food_relief_and_physically_distant_abundance() {
    let mut f = setup();
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    run(&mut f.host, SimTime(2 * MINUTE.0));
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 35));
    let before = f.host.get::<Bodies>().unwrap().people[&f.other].hunger;
    assert!(
        f.host
            .dispatch(Eat {
                person: f.other,
                inventory: f.store
            })
            .is_err()
    );
    assert_eq!(
        f.host.get::<Bodies>().unwrap().people[&f.other].hunger,
        before
    );
    f.host
        .dispatch(Eat {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 990);
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.ruler].hunger, 0);
    run(&mut f.host, SimTime(MINUTE.0 * 220));
    assert!(!f.host.get::<Bodies>().unwrap().people[&f.other].alive);
    assert!(f.host.get::<Bodies>().unwrap().people[&f.guardian].alive);
}
#[test]
fn rest_takes_time_and_a_dead_person_cannot_travel() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    f.host
        .dispatch(Strain {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    f.host.dispatch(Rest { person: f.ruler }).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 2));
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.ruler].fatigue, 92);
    f.host
        .dispatch(Damage {
            person: f.ruler,
            amount: 100,
        })
        .unwrap();
    assert!(f.host.get::<game_sim::Actions>().unwrap().0.is_empty());
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
fn twenty_thousand_deaths_commit_in_stable_order_without_truncating_the_phase() {
    let mut host = builder().build(9, 4).unwrap();
    host.dispatch(Configure(Rules {
        deprivation: 0,
        harm_per_tick: 100,
        ..Default::default()
    }))
    .unwrap();
    for _ in 0..20_000 {
        host.dispatch(Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    }
    host.dispatch(Start).unwrap();
    run(&mut host, SimTime(MINUTE.0));
    assert!(
        host.get::<Bodies>()
            .unwrap()
            .people
            .values()
            .all(|p| !p.alive && p.health == 0)
    );
    let notices: Vec<_> = host.notices().collect();
    assert_eq!(notices.len(), 20_000);
    assert_eq!(notices.first().unwrap().sequence, 1);
    assert_eq!(notices.last().unwrap().sequence, 20_000);
    assert!(
        notices
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    let saved = host.save().unwrap();
    let loaded = builder().load(&saved, 1).unwrap();
    assert_eq!(loaded.save().unwrap(), saved);
}

#[test]
fn batched_physiology_matches_individual_ticks_across_boundaries_and_saturation() {
    for seed in 0..200 {
        let draw = |n| random(seed, 500, n);
        let rules = Rules {
            tick: SimDuration(7),
            hunger_per_tick: draw(1) % 5,
            fatigue_per_tick: draw(2) % 5,
            rest_per_tick: draw(3) % 5,
            deprivation: draw(4) % 120,
            harm_per_tick: draw(5) % 8,
            ..Default::default()
        };
        let initial = Body {
            kind: if draw(6) % 2 == 0 {
                Kind::Mortal
            } else {
                Kind::Guardian
            },
            born: SimTime(0),
            alive: draw(7) % 4 != 0,
            hunger: draw(8) % 100,
            fatigue: draw(9) % 100,
            health: draw(10) % 101,
            maximum_health: None,
            fatigue_fraction: 0,
            hunger_fraction: 0,
            recovery_fraction: 0,
            metabolic_rates: Default::default(),
            resting: draw(11) % 2 == 0,
            last: SimTime(3),
            life_limit: Some(SimTime(draw(12) % 10000)),
        };
        let ticks = draw(13) % 1000;
        let mut slow = initial;
        for _ in 0..ticks {
            if slow.alive {
                if slow.kind == Kind::Mortal {
                    slow.hunger = slow.hunger.saturating_add(rules.hunger_per_tick);
                }
                slow.fatigue = if slow.resting {
                    slow.fatigue.saturating_sub(rules.rest_per_tick)
                } else {
                    slow.fatigue.saturating_add(rules.fatigue_per_tick)
                };
                if slow.kind == Kind::Guardian {
                    slow.health = slow
                        .health
                        .saturating_add(rules.guardian_recovery)
                        .min(rules.max_health);
                } else if slow.hunger >= rules.deprivation {
                    slow.health = slow.health.saturating_sub(rules.harm_per_tick);
                }
            }
            slow.last.0 += rules.tick.0;
            if slow.kind == Kind::Mortal && slow.life_limit.is_some_and(|t| t <= slow.last) {
                slow.health = 0;
            }
        }
        assert_eq!(
            advance_body(
                &initial,
                &rules,
                SimTime(initial.last.0 + ticks * rules.tick.0 + 3)
            ),
            slow,
            "seed {seed}"
        );
    }
}

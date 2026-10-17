//! Exact projection for an uninterrupted interval of the existing linear rules.
use game_core::*;
use game_physiology::{Body, Kind, Rules, advance_body};
use std::{hint::black_box, time::Instant};
fn project(body: &Body, r: &Rules, at: SimTime) -> Body {
    let mut p = *body;
    let ticks = at.0.saturating_sub(p.last.0) / r.tick.0;
    if ticks == 0 {
        return p;
    }
    if p.alive {
        if p.kind == Kind::Mortal {
            let first = if p.hunger >= r.deprivation {
                Some(1)
            } else if r.hunger_per_tick == 0 {
                None
            } else {
                Some((r.deprivation - p.hunger).div_ceil(r.hunger_per_tick))
            };
            if let Some(first) = first {
                let harmful = ticks.saturating_sub(first - 1);
                p.health = p
                    .health
                    .saturating_sub(harmful.saturating_mul(r.harm_per_tick));
            }
            p.hunger = p
                .hunger
                .saturating_add(ticks.saturating_mul(r.hunger_per_tick));
        } else {
            p.health = p
                .health
                .saturating_add(ticks.saturating_mul(r.guardian_recovery))
                .min(r.max_health);
        }
        p.fatigue = if p.resting {
            p.fatigue
                .saturating_sub(ticks.saturating_mul(r.rest_per_tick))
        } else {
            p.fatigue
                .saturating_add(ticks.saturating_mul(r.fatigue_per_tick))
        };
    }
    p.last.0 += ticks * r.tick.0;
    if p.kind == Kind::Mortal && p.life_limit.is_some_and(|limit| limit <= p.last) {
        p.health = 0;
    }
    p
}
fn death_at(b: &Body, r: &Rules) -> Option<SimTime> {
    if !b.alive || b.kind != Kind::Mortal {
        return None;
    }
    if b.health == 0 {
        return Some(b.last);
    }
    let age = b
        .life_limit
        .map(|t| t.0.saturating_sub(b.last.0).div_ceil(r.tick.0).max(1));
    let hunger = if b.hunger >= r.deprivation {
        Some(1)
    } else if r.hunger_per_tick > 0 {
        Some((r.deprivation - b.hunger).div_ceil(r.hunger_per_tick))
    } else {
        None
    };
    let deprivation = hunger.and_then(|first| {
        (r.harm_per_tick > 0)
            .then(|| b.health.div_ceil(r.harm_per_tick))
            .and_then(|harm| first.checked_add(harm - 1))
    });
    let ticks = match (age, deprivation) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }?;
    ticks
        .checked_mul(r.tick.0)?
        .checked_add(b.last.0)
        .map(SimTime)
}
pub fn verify() {
    for seed in 0..5000 {
        let x = random(17, seed, 0);
        let edge = |shift: u32| match (x >> shift) % 8u64 {
            0 => 0,
            1 => u64::MAX,
            _ => (x >> shift) % 101u64,
        };
        let r = Rules {
            tick: SimDuration(1 + (x % 13)),
            hunger_per_tick: edge(3),
            fatigue_per_tick: edge(7),
            rest_per_tick: edge(11),
            deprivation: edge(15),
            harm_per_tick: edge(19),
            guardian_recovery: edge(23),
            max_health: 100,
            ..Rules::default()
        };
        let last = if seed % 23 == 0 {
            u64::MAX - 1000
        } else {
            x % 100
        };
        let b = Body {
            kind: if x & 1 == 0 {
                Kind::Mortal
            } else {
                Kind::Guardian
            },
            born: SimTime(0),
            alive: x & 2 != 0,
            hunger: edge(27),
            fatigue: edge(31),
            health: (x >> 35) % 101,
            maximum_health: None,
            fatigue_fraction: 0,
            resting: x & 4 != 0,
            last: SimTime(last),
            life_limit: (x & 8 != 0).then_some(SimTime(last.saturating_add(x % 500))),
        };
        let at = SimTime(last.saturating_add((x % 1000) * r.tick.0));
        assert_eq!(
            project(&b, &r, at),
            advance_body(&b, &r, at),
            "projection seed={seed}"
        );
        if b.health > 0
            && let Some(death) = death_at(&b, &r)
            && death.0.saturating_sub(last) / r.tick.0 <= 1000
        {
            assert_eq!(advance_body(&b, &r, death).health, 0);
            assert!(advance_body(&b, &r, SimTime(death.0 - 1)).health > 0);
        }
    }
}
pub fn measure(n: usize) {
    let rules = Rules {
        harm_per_tick: 0,
        ..Rules::default()
    };
    let bodies: Vec<_> = (0..n)
        .map(|i| Body {
            kind: if i % 19 == 0 {
                Kind::Guardian
            } else {
                Kind::Mortal
            },
            born: SimTime(0),
            alive: true,
            hunger: i as u64 % 25,
            fatigue: i as u64 % 10,
            health: 100,
            resting: i % 2 == 0,
            last: SimTime(0),
            life_limit: None,
            maximum_health: None,
            fatigue_fraction: 0,
        })
        .collect();
    let at = SimTime(DAY.0);
    // This compares the pure fold, not a live interval containing deaths or actions.
    let t = Instant::now();
    let expected: Vec<_> = black_box(&bodies)
        .iter()
        .map(|b| advance_body(b, &rules, at))
        .collect();
    let eager = t.elapsed().as_secs_f64() * 1000.;
    let t = Instant::now();
    let projected: Vec<_> = black_box(&bodies)
        .iter()
        .map(|b| project(b, &rules, at))
        .collect();
    let projected_ms = t.elapsed().as_secs_f64() * 1000.;
    assert_eq!(expected, projected);
    println!(
        "architecture=interval_projection population={n} interval_minutes=1440 passive_only=true harm_per_tick=0 eager_fold_ms={eager:.3} projected_ms={projected_ms:.3} exact=true"
    );
}

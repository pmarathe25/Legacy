use super::*;
pub fn advance_body(body: &Body, rules: &Rules, at: SimTime) -> Body {
    let mut p = *body;
    let ticks = (at.0.saturating_sub(p.last.0)) / rules.tick.0;
    for _ in 0..ticks {
        if p.alive {
            if p.kind == Kind::Mortal {
                p.hunger = p.hunger.saturating_add(rules.hunger_per_tick);
            }
            if p.resting {
                p.fatigue = p.fatigue.saturating_sub(rules.rest_per_tick);
            } else {
                p.fatigue = p.fatigue.saturating_add(rules.fatigue_per_tick);
            }
            if p.kind == Kind::Guardian {
                p.health = p
                    .health
                    .saturating_add(rules.guardian_recovery)
                    .min(rules.max_health);
            } else if p.hunger >= rules.deprivation {
                p.health = p.health.saturating_sub(rules.harm_per_tick);
            }
        }
        p.last.0 += rules.tick.0;
        if p.kind == Kind::Mortal && p.life_limit.is_some_and(|limit| limit <= p.last) {
            p.health = 0;
        }
    }
    p
}

/// First possible hunger crossing if physiology continues unchanged. External
/// changes invalidate this forecast; action completion wakes its consumers.
pub fn hunger_threshold(body: &Body, rules: &Rules, threshold: u64) -> Option<SimTime> {
    if body.hunger >= threshold {
        return Some(body.last);
    }
    if !body.alive || body.kind != Kind::Mortal || rules.hunger_per_tick == 0 {
        return None;
    }
    let ticks = (threshold - body.hunger).div_ceil(rules.hunger_per_tick);
    ticks
        .checked_mul(rules.tick.0)
        .and_then(|dt| body.last.0.checked_add(dt))
        .map(SimTime)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hunger_forecasts_match_tick_evolution_including_saturation_and_clock_limits() {
        let body = Body {
            kind: Kind::Mortal,
            born: SimTime(0),
            alive: true,
            hunger: 10,
            fatigue: 0,
            health: 100,
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
}

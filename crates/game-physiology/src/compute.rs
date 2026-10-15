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

use super::*;
pub fn advance_body(body: &Body, rules: &Rules, at: SimTime) -> Body {
    advance_scaled(body, rules, at, 1000)
}
pub fn advance_scaled(body: &Body, rules: &Rules, at: SimTime, fatigue_per_mille: u64) -> Body {
    let mut p = *body;
    let ticks = (at.0.saturating_sub(p.last.0)) / rules.tick.0;
    if p.alive {
        if p.resting {
            p.fatigue = p
                .fatigue
                .saturating_sub(ticks.saturating_mul(rules.rest_per_tick));
        } else {
            let gain = u128::from(ticks)
                .saturating_mul(u128::from(rules.fatigue_per_tick))
                .saturating_mul(u128::from(fatigue_per_mille))
                .saturating_add(u128::from(p.fatigue_fraction));
            p.fatigue = p
                .fatigue
                .saturating_add((gain / 1000).min(u128::from(u64::MAX)) as u64);
            p.fatigue_fraction = (gain % 1000) as u16;
        }
        if p.kind == Kind::Guardian {
            p.health = p
                .health
                .saturating_add(ticks.saturating_mul(rules.guardian_recovery))
                .min(p.maximum(rules));
        } else {
            let first = if p.hunger >= rules.deprivation {
                1
            } else if rules.hunger_per_tick > 0 {
                (rules.deprivation - p.hunger).div_ceil(rules.hunger_per_tick)
            } else {
                u64::MAX
            };
            let harmful = if ticks >= first { ticks - first + 1 } else { 0 };
            p.health = p
                .health
                .saturating_sub(harmful.saturating_mul(rules.harm_per_tick));
            p.hunger = p
                .hunger
                .saturating_add(ticks.saturating_mul(rules.hunger_per_tick));
        }
    }
    p.last.0 += ticks * rules.tick.0;
    if ticks > 0 && p.kind == Kind::Mortal && p.life_limit.is_some_and(|limit| limit <= p.last) {
        p.health = 0;
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

use super::*;
pub fn advance_body(body: &Body, rules: &Rules, at: SimTime) -> Body {
    advance_scaled(body, rules, at, 1000)
}
pub fn advance_scaled(body: &Body, rules: &Rules, at: SimTime, fatigue_per_mille: u64) -> Body {
    advance_rates(
        body,
        rules,
        at,
        Rates {
            fatigue: fatigue_per_mille,
            ..Default::default()
        },
    )
}
pub fn advance_rates(body: &Body, rules: &Rules, at: SimTime, rates: Rates) -> Body {
    let mut p = *body;
    let ticks = (at.0.saturating_sub(p.last.0)) / rules.tick.0;
    if p.alive {
        p.metabolic_rates = rates;
        if p.resting {
            let rest = u128::from(ticks)
                .saturating_mul(u128::from(rules.rest_per_tick))
                .saturating_mul(u128::from(rates.recovery))
                .saturating_add(u128::from(p.recovery_fraction));
            p.fatigue = p
                .fatigue
                .saturating_sub((rest / 1000).min(u128::from(u64::MAX)) as u64);
            p.recovery_fraction = if p.fatigue == 0 {
                0
            } else {
                (rest % 1000) as u16
            };
        } else {
            let gain = u128::from(ticks)
                .saturating_mul(u128::from(rules.fatigue_per_tick))
                .saturating_mul(u128::from(rates.fatigue))
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
            let first = crossing(&p, rules, rules.deprivation, rates.energy);
            let harmful = first
                .filter(|first| *first <= u128::from(ticks))
                .map_or(0, |first| u128::from(ticks) - first + 1) as u64;
            p.health = p
                .health
                .saturating_sub(harmful.saturating_mul(rules.harm_per_tick));
            let gain = u128::from(ticks)
                .saturating_mul(u128::from(rules.hunger_per_tick))
                .saturating_mul(u128::from(rates.energy))
                .saturating_add(u128::from(p.hunger_fraction));
            p.hunger = p
                .hunger
                .saturating_add((gain / 1000).min(u128::from(u64::MAX)) as u64);
            p.hunger_fraction = (gain % 1000) as u16;
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
    hunger_threshold_scaled(body, rules, threshold, body.metabolic_rates.energy)
}
fn crossing(body: &Body, rules: &Rules, threshold: u64, energy: u64) -> Option<u128> {
    if body.hunger >= threshold {
        return Some(1);
    }
    let rate = u128::from(rules.hunger_per_tick) * u128::from(energy);
    (rate > 0).then(|| {
        (u128::from(threshold - body.hunger) * 1000 - u128::from(body.hunger_fraction))
            .div_ceil(rate)
    })
}
pub fn hunger_threshold_scaled(
    body: &Body,
    rules: &Rules,
    threshold: u64,
    energy: u64,
) -> Option<SimTime> {
    if body.hunger >= threshold {
        return Some(body.last);
    }
    if !body.alive || body.kind != Kind::Mortal || rules.hunger_per_tick == 0 {
        return None;
    }
    let ticks = u64::try_from(crossing(body, rules, threshold, energy)?).ok()?;
    ticks
        .checked_mul(rules.tick.0)
        .and_then(|dt| body.last.0.checked_add(dt))
        .map(SimTime)
}

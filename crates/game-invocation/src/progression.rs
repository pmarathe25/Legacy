use crate::*;
use game_core::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct ActiveInput<'a> {
    pub life_count: usize,
    pub accepted: &'a BTreeMap<InvocationCost, Consent>,
    pub episode: Option<&'a Episode>,
    pub next_loss_burden: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct AdvanceInput<'a> {
    pub rules: &'a Rules,
    pub before: &'a Burden,
    pub actor: PersonId,
    pub now: SimTime,
    pub next: SimTime,
    pub alive: bool,
    pub active: Option<ActiveInput<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BurdenAdvance {
    pub burden: Burden,
    pub end_episode: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CostRequirement {
    pub cost: InvocationCost,
    pub severity: u8,
    pub permanent: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyRates {
    pub fatigue_per_mille: u64,
    pub health_harm_per_mille: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodyContribution {
    pub health_milli: i128,
    pub fatigue_milli: i128,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivationCharge {
    pub burden: u64,
    pub fatigue: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CostEffect {
    pub fatigue_per_mille: u64,
    pub health_harm_per_mille: u64,
    pub identity_skill_penalty: u16,
    pub emotion_disruption: u16,
    pub lifespan_loss: SimDuration,
    pub control_loss_duration: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeStart {
    pub episode: Episode,
    pub next_loss_burden: u64,
    pub next_controller: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermanentChange {
    pub burden: Burden,
    pub changed: bool,
}

pub fn severity(burden: u64, thresholds: [u64; 3]) -> u8 {
    thresholds.iter().filter(|value| burden >= **value).count() as u8
}
pub fn growth_rate(life_count: usize) -> Result<u64, Error> {
    let count = u64::try_from(life_count).map_err(|_| Error::Overflow)?;
    count.checked_mul(count).ok_or(Error::Overflow)
}
pub fn controls_actor(
    episode: Option<&Episode>,
    actor: PersonId,
    now: SimTime,
    alive: bool,
) -> bool {
    alive && episode.is_some_and(|e| e.actor == actor && now < e.ends)
}
pub fn effective_band(
    accepted: &BTreeMap<InvocationCost, Consent>,
    cost: InvocationCost,
    required: u8,
    controlled: bool,
) -> u8 {
    if controlled {
        required
    } else {
        accepted
            .get(&cost)
            .map_or(0, |consent| consent.temporary.min(required))
    }
}
pub fn required_bands(
    rules: &Rules,
    life_count: usize,
    before: &Burden,
    next_loss: u64,
) -> Vec<CostRequirement> {
    let permanent =
        before.burden >= rules.permanent_burden && before.exposure >= rules.permanent_exposure.0;
    let strain = if permanent {
        4
    } else {
        severity(before.burden, rules.strain_thresholds)
    };
    let mut bands = Vec::new();
    if strain > 0 || rules.strain_per_mille[0] > 0 {
        bands.push(CostRequirement {
            cost: InvocationCost::Strain,
            severity: strain,
            permanent: false,
        });
    }
    let emotional = severity(before.burden, rules.emotional_thresholds);
    if emotional > 0 {
        bands.push(CostRequirement {
            cost: InvocationCost::Emotional,
            severity: emotional,
            permanent: false,
        });
    }
    let identity = severity(before.burden, rules.identity_thresholds);
    if identity > 0 {
        bands.push(CostRequirement {
            cost: InvocationCost::Identity,
            severity: identity,
            permanent: false,
        });
    }
    if permanent {
        let count = (1
            + (before.exposure - rules.permanent_exposure.0) / rules.permanent_interval.0)
            .min(252) as u8;
        bands.push(CostRequirement {
            cost: InvocationCost::Identity,
            severity: count,
            permanent: true,
        });
        bands.push(CostRequirement {
            cost: InvocationCost::Lifespan,
            severity: count,
            permanent: true,
        });
    }
    if before.burden >= next_loss && life_count > 0 {
        bands.push(CostRequirement {
            cost: InvocationCost::LossOfControl,
            severity: 1,
            permanent: false,
        });
    }
    bands
}
pub fn next_warning(
    rules: &Rules,
    life_count: usize,
    before: &Burden,
    next_loss: u64,
    accepted: &BTreeMap<InvocationCost, Consent>,
    activation: bool,
    controlled: bool,
) -> Option<CostRequirement> {
    if controlled {
        return None;
    }
    if activation && !accepted.contains_key(&InvocationCost::Strain) {
        return Some(CostRequirement {
            cost: InvocationCost::Strain,
            severity: 0,
            permanent: false,
        });
    }
    required_bands(rules, life_count, before, next_loss)
        .into_iter()
        .find(|band| {
            accepted.get(&band.cost).is_none_or(|consent| {
                if band.permanent {
                    consent.permanent < band.severity
                } else {
                    consent.temporary < band.severity
                }
            }) || !activation && band.cost == InvocationCost::LossOfControl
        })
}
pub fn accept_requirement(
    accepted: &BTreeMap<InvocationCost, Consent>,
    requirement: CostRequirement,
) -> BTreeMap<InvocationCost, Consent> {
    let mut accepted = accepted.clone();
    let consent = accepted.entry(requirement.cost).or_default();
    if requirement.permanent {
        consent.permanent = requirement.severity;
    } else {
        consent.temporary = requirement.severity;
    }
    accepted
}
pub fn activation_charge(
    rules: &Rules,
    before: &Burden,
    life_count: usize,
) -> Result<ActivationCharge, Error> {
    let rate = growth_rate(life_count)?;
    let burden = rules
        .activation_burden
        .checked_mul(rate)
        .ok_or(Error::Overflow)?;
    let fatigue = rules
        .activation_fatigue
        .checked_mul(rate)
        .ok_or(Error::Overflow)?;
    before.burden.checked_add(burden).ok_or(Error::Overflow)?;
    Ok(ActivationCharge { burden, fatigue })
}
pub fn projected_activation(
    rules: &Rules,
    before: &Burden,
    life_count: usize,
) -> Result<Burden, Error> {
    let charge = activation_charge(rules, before, life_count)?;
    let mut after = before.clone();
    after.burden = after
        .burden
        .checked_add(charge.burden)
        .ok_or(Error::Overflow)?;
    Ok(after)
}
pub fn body_rates(
    rules: &Rules,
    before: &Burden,
    active: Option<ActiveInput<'_>>,
    actor: PersonId,
    now: SimTime,
    alive: bool,
) -> Result<BodyRates, Error> {
    let Some(active) = active else {
        return Ok(BodyRates::default());
    };
    let controlled = controls_actor(active.episode, actor, now, alive);
    if !controlled && !active.accepted.contains_key(&InvocationCost::Strain) {
        return Ok(BodyRates::default());
    }
    let required = if before.burden >= rules.permanent_burden
        && before.exposure >= rules.permanent_exposure.0
    {
        4
    } else {
        severity(before.burden, rules.strain_thresholds)
    };
    let band = effective_band(
        active.accepted,
        InvocationCost::Strain,
        required,
        controlled,
    );
    let count = growth_rate(active.life_count)?;
    Ok(BodyRates {
        fatigue_per_mille: rules.strain_per_mille[usize::from(band.min(3))]
            .checked_mul(count)
            .ok_or(Error::Overflow)?,
        health_harm_per_mille: if band > 3 {
            rules.health_harm_per_mille[3]
                .checked_mul(count)
                .ok_or(Error::Overflow)?
        } else {
            0
        },
    })
}
pub fn body_contribution(input: AdvanceInput<'_>) -> Result<BodyContribution, Error> {
    let elapsed = input
        .next
        .0
        .checked_sub(input.now.0)
        .ok_or(Error::Overflow)?;
    let rates = body_rates(
        input.rules,
        input.before,
        input.active,
        input.actor,
        input.now,
        input.alive,
    )?;
    Ok(BodyContribution {
        health_milli: -i128::from(elapsed) * i128::from(rates.health_harm_per_mille),
        fatigue_milli: i128::from(elapsed) * i128::from(rates.fatigue_per_mille),
    })
}
fn signed_accumulator(value: u64, remainder: u16, elapsed: u64, rate: i128) -> (u64, u16) {
    let total = (i128::from(value) * 1_000 + i128::from(remainder) + i128::from(elapsed) * rate)
        .clamp(0, i128::from(u64::MAX) * 1_000);
    ((total / 1_000) as u64, (total % 1_000) as u16)
}
pub fn advance_burden(input: AdvanceInput<'_>) -> Result<BurdenAdvance, Error> {
    let elapsed = input
        .next
        .0
        .checked_sub(input.now.0)
        .ok_or(Error::Overflow)?;
    let mut burden = input.before.clone();
    let end_episode = input
        .active
        .and_then(|a| a.episode)
        .is_some_and(|episode| input.next >= episode.ends);
    if !input.alive {
        return Ok(BurdenAdvance {
            burden,
            end_episode,
        });
    }
    if let Some(active) = input.active {
        let rate = growth_rate(active.life_count)?;
        burden.burden = burden
            .burden
            .checked_add(elapsed.checked_mul(rate).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        burden.exposure = burden
            .exposure
            .checked_add(elapsed)
            .ok_or(Error::Overflow)?;
        burden.uninterrupted = burden
            .uninterrupted
            .checked_add(elapsed)
            .ok_or(Error::Overflow)?;
        let controlled = controls_actor(active.episode, input.actor, input.now, input.alive);
        burden.temporary_identity = u16::from(effective_band(
            active.accepted,
            InvocationCost::Identity,
            severity(input.before.burden, input.rules.identity_thresholds),
            controlled,
        )) * 5;
        burden.emotional_disruption = u16::from(effective_band(
            active.accepted,
            InvocationCost::Emotional,
            severity(input.before.burden, input.rules.emotional_thresholds),
            controlled,
        )) * 100;
    } else {
        let (value, remainder) = signed_accumulator(
            burden.burden,
            burden.burden_remainder,
            elapsed,
            -i128::from(input.rules.burden_recovery_per_mille),
        );
        burden.burden = value;
        burden.burden_remainder = remainder;
        let (value, remainder) = signed_accumulator(
            burden.exposure,
            burden.exposure_remainder,
            elapsed,
            -i128::from(input.rules.exposure_recovery_per_mille),
        );
        burden.exposure = value;
        burden.exposure_remainder = remainder;
        burden.uninterrupted = 0;
        burden.temporary_identity = burden
            .temporary_identity
            .min(u16::from(severity(burden.burden, input.rules.identity_thresholds)) * 5);
        burden.emotional_disruption = burden
            .emotional_disruption
            .min(u16::from(severity(burden.burden, input.rules.emotional_thresholds)) * 100);
    }
    Ok(BurdenAdvance {
        burden,
        end_episode,
    })
}
pub fn refresh_temporary(
    rules: &Rules,
    before: &Burden,
    accepted: &BTreeMap<InvocationCost, Consent>,
    controlled: bool,
) -> Burden {
    let mut burden = before.clone();
    let identity = u16::from(effective_band(
        accepted,
        InvocationCost::Identity,
        severity(before.burden, rules.identity_thresholds),
        controlled,
    )) * 5;
    let emotional = u16::from(effective_band(
        accepted,
        InvocationCost::Emotional,
        severity(before.burden, rules.emotional_thresholds),
        controlled,
    )) * 100;
    burden.temporary_identity = burden.temporary_identity.max(identity);
    burden.emotional_disruption = burden.emotional_disruption.max(emotional);
    burden
}
pub fn apply_permanent(
    rules: &Rules,
    before: &Burden,
    requirement: CostRequirement,
) -> Result<PermanentChange, Error> {
    let mut burden = before.clone();
    let changed = match requirement.cost {
        InvocationCost::Identity => {
            let target = u16::from(requirement.severity)
                .checked_mul(rules.permanent_identity_per_band)
                .ok_or(Error::Overflow)?;
            let changed = target > burden.permanent_identity;
            burden.permanent_identity = burden.permanent_identity.max(target);
            changed
        }
        InvocationCost::Lifespan => {
            let target = rules
                .lifespan_loss_per_band
                .0
                .checked_mul(u64::from(requirement.severity))
                .ok_or(Error::Overflow)?;
            let changed = target > burden.lifespan_loss;
            burden.lifespan_loss = burden.lifespan_loss.max(target);
            changed
        }
        _ => false,
    };
    Ok(PermanentChange { burden, changed })
}
pub fn begin_episode(
    rules: &Rules,
    before: &Burden,
    actor: PersonId,
    now: SimTime,
    consent: &Consent,
) -> Result<EpisodeStart, Error> {
    if consent.temporary == 0 {
        return Err(Error::Unavailable);
    }
    let ends = SimTime(
        now.0
            .checked_add(rules.loss_duration.0)
            .ok_or(Error::Overflow)?,
    );
    let next_loss_burden = before
        .burden
        .checked_add(rules.loss_gap)
        .ok_or(Error::Overflow)?;
    Ok(EpisodeStart {
        episode: Episode {
            actor,
            starts: now,
            ends,
        },
        next_loss_burden,
        next_controller: now,
    })
}
pub fn cost_effect(
    rules: &Rules,
    life_count: usize,
    requirement: CostRequirement,
) -> Result<CostEffect, Error> {
    let mut effect = CostEffect::default();
    let band = requirement.severity;
    match requirement.cost {
        InvocationCost::Strain => {
            let count = growth_rate(life_count)?;
            effect.fatigue_per_mille = rules.strain_per_mille[usize::from(band.min(3))]
                .checked_mul(count)
                .ok_or(Error::Overflow)?;
            if band > 3 {
                effect.health_harm_per_mille = rules.health_harm_per_mille[3]
                    .checked_mul(count)
                    .ok_or(Error::Overflow)?;
            }
        }
        InvocationCost::Identity => {
            effect.identity_skill_penalty = u16::from(band)
                .checked_mul(if requirement.permanent {
                    rules.permanent_identity_per_band
                } else {
                    5
                })
                .ok_or(Error::Overflow)?;
        }
        InvocationCost::Emotional => effect.emotion_disruption = u16::from(band) * 100,
        InvocationCost::Lifespan => {
            effect.lifespan_loss = SimDuration(
                rules
                    .lifespan_loss_per_band
                    .0
                    .checked_mul(u64::from(band))
                    .ok_or(Error::Overflow)?,
            )
        }
        InvocationCost::LossOfControl => effect.control_loss_duration = rules.loss_duration,
    }
    Ok(effect)
}
pub fn next_cost_after(
    rules: &Rules,
    before: &Burden,
    life_count: usize,
    next_loss: u64,
) -> Option<SimDuration> {
    let rate = growth_rate(life_count).ok()?;
    if rate == 0 {
        return None;
    }
    let mut candidates = Vec::new();
    for threshold in rules
        .strain_thresholds
        .into_iter()
        .chain(rules.emotional_thresholds)
        .chain(rules.identity_thresholds)
        .chain([rules.permanent_burden])
    {
        if threshold > before.burden {
            candidates.push((threshold - before.burden).div_ceil(rate));
        }
    }
    if next_loss > before.burden {
        candidates.push((next_loss - before.burden).div_ceil(rate));
    }
    if before.exposure < rules.permanent_exposure.0 {
        candidates.push(rules.permanent_exposure.0 - before.exposure);
    } else {
        let next = rules.permanent_exposure.0.checked_add(
            ((before.exposure - rules.permanent_exposure.0) / rules.permanent_interval.0 + 1)
                .checked_mul(rules.permanent_interval.0)?,
        )?;
        candidates.push(next - before.exposure);
    }
    candidates
        .into_iter()
        .filter(|value| *value > 0)
        .min()
        .map(SimDuration)
}
pub fn recovery_boundary(rules: &Rules, before: &Burden, now: SimTime, limit: SimTime) -> SimTime {
    let mut boundary = limit;
    if rules.burden_recovery_per_mille == 0 {
        return boundary;
    }
    for threshold in rules
        .identity_thresholds
        .into_iter()
        .chain(rules.emotional_thresholds)
    {
        if before.burden >= threshold {
            let remaining = u128::from(before.burden) * 1_000 + u128::from(before.burden_remainder)
                - u128::from(threshold) * 1_000
                + 1;
            let duration = remaining.div_ceil(u128::from(rules.burden_recovery_per_mille));
            if let Ok(duration) = u64::try_from(duration)
                && let Some(time) = now.0.checked_add(duration.max(1))
            {
                boundary = boundary.min(SimTime(time));
            }
        }
    }
    boundary
}
pub fn active_boundary(
    rules: &Rules,
    before: &Burden,
    active: ActiveInput<'_>,
    now: SimTime,
    next_controller: SimTime,
    limit: SimTime,
) -> SimTime {
    let mut boundary = limit;
    if let Some(duration) =
        next_cost_after(rules, before, active.life_count, active.next_loss_burden)
        && let Some(time) = now.0.checked_add(duration.0)
    {
        boundary = boundary.min(SimTime(time));
    }
    if let Some(episode) = active.episode {
        if episode.ends > now {
            boundary = boundary.min(episode.ends);
        }
        if next_controller > now {
            boundary = boundary.min(next_controller);
        }
    }
    boundary
}

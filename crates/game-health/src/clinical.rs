use crate::*;
use game_core::*;

pub fn validate_rules(r: &Rules) -> Result<(), Error> {
    if r.drink_quantity.0 == 0
        || r.drink_relief == 0
        || r.thirst_harm == 0
        || r.cold_harm == 0
        || r.infection_threshold == 0
        || r.boiling_fuel.0 == 0
        || r.treatment_herbs.0 == 0
        || r.treatment_water.0 == 0
        || r.shelter_timber_per_place.0 == 0
        || [
            r.drinking_time,
            r.water_search_retry,
            r.incubation,
            r.illness_duration,
            r.boiling_time,
            r.treatment_time,
            r.treatment_duration,
            r.offer_duration,
            r.shelter_time_per_place,
        ]
        .iter()
        .any(|d| d.0 == 0)
        || !(-100..=100).contains(&r.cold_threshold_c)
        || [
            r.thirst_damage,
            r.cold_accumulation,
            r.warmth_recovery,
            r.cold_damage,
            r.sheltered_sleep_bonus,
            r.contact_dose,
            r.illness_damage,
            r.illness_fatigue,
            r.treatment_healing,
        ]
        .iter()
        .any(|v| *v > 10_000)
    {
        return Err(Error::InvalidConfiguration(
            "invalid human health rates".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExposureInput {
    pub temperature_c: i32,
    pub insulation: u16,
    pub infectious_contacts: [u64; 2],
}
/// Individual biological thresholds, derived from static scenario rules and
/// expressed traits. They do not alter the shared rules or stored exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClinicalPhysiology {
    pub thirst_harm: u64,
    pub drink_relief: u64,
    pub cold_tolerance_centi: i16,
}
pub fn clinical_physiology(
    rules: &Rules,
    metabolism_milli: u16,
    cold_tolerance_centi: i16,
) -> Result<ClinicalPhysiology, Error> {
    if metabolism_milli == 0 {
        return Err(Error::InvalidConfiguration(
            "zero clinical metabolism".into(),
        ));
    }
    let scale = |duration: u64| {
        (u128::from(duration) * 1_000 / u128::from(metabolism_milli))
            .min(u128::from(u64::MAX))
            .max(1) as u64
    };
    Ok(ClinicalPhysiology {
        thirst_harm: scale(rules.thirst_harm),
        drink_relief: scale(rules.drink_relief),
        cold_tolerance_centi,
    })
}
/// Opening facts for one actor. Contact counts and environment are supplied by
/// the coordinator; the domain cannot query another actor or the world.
#[derive(Clone, Copy, Debug)]
pub struct ClinicalInput<'a> {
    pub rules: &'a Rules,
    pub state: &'a Clinical,
    pub now: SimTime,
    pub next: SimTime,
    pub alive: bool,
    pub thirst_enabled: bool,
    pub resting: bool,
    pub exposure: ExposureInput,
    pub physiology: ClinicalPhysiology,
}
impl ClinicalInput<'_> {
    fn elapsed(&self) -> Result<u64, Error> {
        self.next.0.checked_sub(self.now.0).ok_or(Error::Overflow)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HealthDelta {
    pub health_milli: i128,
    pub fatigue_milli: i128,
}
pub fn cold_rate(rules: &Rules, temperature_c: i32, insulation: u16, tolerance_centi: i16) -> i128 {
    let effective = i128::from(temperature_c) * 1_000
        + i128::from(insulation) * 20
        + i128::from(tolerance_centi) * 10;
    let deficit = i128::from(rules.cold_threshold_c) * 1_000 - effective;
    if deficit > 0 {
        deficit * i128::from(rules.cold_accumulation) / 1_000
    } else {
        -i128::from(rules.warmth_recovery)
    }
}
pub fn contact_rates(rules: &Rules, state: &Clinical, contacts: [u64; 2]) -> [u128; 2] {
    let susceptibility = if state.cold >= rules.cold_harm { 2 } else { 1 };
    contacts.map(|count| u128::from(count) * u128::from(rules.contact_dose) * susceptibility)
}
/// Signed thousandths of bodily resource change. The physiology owner sums
/// every domain contribution before its single clamp. Clip the interval with
/// `clinical_boundary` before calling this opening-band calculation.
pub fn clinical_contribution(input: ClinicalInput<'_>) -> Result<HealthDelta, Error> {
    if !input.alive {
        return Ok(HealthDelta::default());
    }
    let elapsed = input.elapsed()?;
    let state = input.state;
    let rules = input.rules;
    let mut damage = 0i128;
    let mut fatigue = 0i128;
    if input.thirst_enabled && state.thirst >= input.physiology.thirst_harm {
        damage += i128::from(rules.thirst_damage);
    }
    if state.cold >= rules.cold_harm {
        damage += i128::from(rules.cold_damage);
        fatigue += i128::from(rules.illness_fatigue);
    }
    let treated = state
        .treatment
        .as_ref()
        .filter(|course| course.until > input.now);
    for _ in state
        .illnesses
        .values()
        .filter(|i| i.symptomatic && i.recovers > input.now)
    {
        damage += i128::from(rules.illness_damage) / (if treated.is_some() { 2 } else { 1 });
        fatigue += i128::from(rules.illness_fatigue);
    }
    let healing = treated.map_or(0, |c| i128::from(c.healing));
    if input.resting {
        fatigue -=
            i128::from(rules.sheltered_sleep_bonus) * i128::from(input.exposure.insulation) / 1_000;
    }
    Ok(HealthDelta {
        health_milli: (healing - damage)
            .checked_mul(i128::from(elapsed))
            .ok_or(Error::Overflow)?,
        fatigue_milli: fatigue
            .checked_mul(i128::from(elapsed))
            .ok_or(Error::Overflow)?,
    })
}
/// Advance clinical counters over an interval whose input facts are constant.
/// Reconcile closing transitions separately, so runtime effects remain explicit.
pub fn advance_clinical(input: ClinicalInput<'_>) -> Result<Clinical, Error> {
    let mut state = input.state.clone();
    if !input.alive {
        return Ok(state);
    }
    let elapsed = input.elapsed()?;
    if input.thirst_enabled {
        state.thirst = state.thirst.checked_add(elapsed).ok_or(Error::Overflow)?;
    }
    let cold = i128::from(state.cold) * 1_000
        + i128::from(state.cold_remainder)
        + cold_rate(
            input.rules,
            input.exposure.temperature_c,
            input.exposure.insulation,
            input.physiology.cold_tolerance_centi,
        )
        .checked_mul(i128::from(elapsed))
        .ok_or(Error::Overflow)?;
    let cold = cold.max(0);
    state.cold = u64::try_from(cold / 1_000).map_err(|_| Error::Overflow)?;
    state.cold_remainder = (cold % 1_000) as u16;
    let rates = contact_rates(input.rules, input.state, input.exposure.infectious_contacts);
    for (index, kind) in [IllnessKind::Enteric, IllnessKind::Respiratory]
        .into_iter()
        .enumerate()
    {
        if state.illnesses.contains_key(&kind) {
            continue;
        }
        let dose = state.doses.entry(kind).or_default();
        let total = u128::from(dose.amount) * 1_000
            + u128::from(dose.remainder)
            + rates[index]
                .checked_mul(u128::from(elapsed))
                .ok_or(Error::Overflow)?;
        dose.amount = (total / 1_000).min(u128::from(input.rules.infection_threshold)) as u64;
        dose.remainder = (total % 1_000) as u16;
        if dose.amount >= input.rules.infection_threshold {
            input
                .next
                .0
                .checked_add(input.rules.incubation.0)
                .and_then(|t| t.checked_add(input.rules.illness_duration.0))
                .ok_or(Error::Overflow)?;
        }
    }
    Ok(state)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClinicalEvent {
    Started(IllnessKind),
    Recovered(IllnessKind),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClinicalReconciliation {
    pub state: Clinical,
    pub events: Vec<ClinicalEvent>,
}
pub fn reconcile_clinical(
    before: &Clinical,
    rules: &Rules,
    now: SimTime,
    alive: bool,
) -> Result<ClinicalReconciliation, Error> {
    let mut state = before.clone();
    let mut events = Vec::new();
    if !alive {
        return Ok(ClinicalReconciliation { state, events });
    }
    for kind in [IllnessKind::Enteric, IllnessKind::Respiratory] {
        if !state.illnesses.contains_key(&kind)
            && state
                .doses
                .get(&kind)
                .is_some_and(|d| d.amount >= rules.infection_threshold)
        {
            let onset = SimTime(
                now.0
                    .checked_add(rules.incubation.0)
                    .ok_or(Error::Overflow)?,
            );
            let recovers = SimTime(
                onset
                    .0
                    .checked_add(rules.illness_duration.0)
                    .ok_or(Error::Overflow)?,
            );
            state.illnesses.insert(
                kind,
                Infection {
                    kind,
                    acquired: now,
                    onset,
                    recovers,
                    symptomatic: false,
                },
            );
        }
        if let Some(infection) = state.illnesses.get_mut(&kind) {
            if infection.recovers <= now {
                state.illnesses.remove(&kind);
                state.doses.remove(&kind);
                events.push(ClinicalEvent::Recovered(kind));
            } else if !infection.symptomatic && infection.onset <= now {
                infection.symptomatic = true;
                events.push(ClinicalEvent::Started(kind));
            }
        }
    }
    if state.treatment.as_ref().is_some_and(|c| c.until <= now) {
        state.treatment = None;
    }
    Ok(ClinicalReconciliation { state, events })
}
pub fn receive_water(
    before: &Clinical,
    rules: &Rules,
    physiology: ClinicalPhysiology,
    now: SimTime,
    source: InventoryId,
    quantity: Quantity,
    load: u128,
) -> Result<Clinical, Error> {
    validate_rules(rules)?;
    let mut state = before.clone();
    let relief = (u128::from(physiology.drink_relief) * u128::from(quantity.0)
        / u128::from(rules.drink_quantity.0))
    .min(u128::from(u64::MAX)) as u64;
    state.thirst = state.thirst.saturating_sub(relief);
    state.last_drink = Some((source, now));
    let dose = state.doses.entry(IllnessKind::Enteric).or_default();
    dose.amount =
        (u128::from(dose.amount) + load).min(u128::from(rules.infection_threshold)) as u64;
    if !state.illnesses.contains_key(&IllnessKind::Enteric)
        && dose.amount >= rules.infection_threshold
    {
        let onset = SimTime(
            now.0
                .checked_add(rules.incubation.0)
                .ok_or(Error::Overflow)?,
        );
        let recovers = SimTime(
            onset
                .0
                .checked_add(rules.illness_duration.0)
                .ok_or(Error::Overflow)?,
        );
        state.illnesses.insert(
            IllnessKind::Enteric,
            Infection {
                kind: IllnessKind::Enteric,
                acquired: now,
                onset,
                recovers,
                symptomatic: false,
            },
        );
    }
    Ok(state)
}
/// Clinical transitions only; mortality and fatigue limits belong to physiology.
pub fn clinical_boundary(input: ClinicalInput<'_>, limit: SimTime) -> Result<SimTime, Error> {
    if limit < input.now {
        return Err(Error::Overflow);
    }
    let mut boundary = limit;
    let state = input.state;
    let rules = input.rules;
    for infection in state.illnesses.values() {
        for at in [infection.onset, infection.recovers] {
            if at > input.now {
                boundary = boundary.min(at);
            }
        }
    }
    if let Some(course) = &state.treatment
        && course.until > input.now
    {
        boundary = boundary.min(course.until);
    }
    if !input.alive {
        return Ok(boundary);
    }
    let at = |elapsed: u128| {
        u64::try_from(elapsed)
            .ok()
            .and_then(|elapsed| input.now.0.checked_add(elapsed))
            .map(SimTime)
    };
    if input.thirst_enabled
        && state.thirst < input.physiology.thirst_harm
        && let Some(next) = at(u128::from(input.physiology.thirst_harm - state.thirst))
    {
        boundary = boundary.min(next);
    }
    let rate = cold_rate(
        rules,
        input.exposure.temperature_c,
        input.exposure.insulation,
        input.physiology.cold_tolerance_centi,
    );
    let current = u128::from(state.cold) * 1_000 + u128::from(state.cold_remainder);
    let threshold = u128::from(rules.cold_harm) * 1_000;
    if rate > 0
        && current < threshold
        && let Some(next) = at((threshold - current).div_ceil(rate as u128))
    {
        boundary = boundary.min(next);
    } else if rate < 0
        && current >= threshold
        && let Some(next) = at((current - threshold + 1).div_ceil((-rate) as u128))
    {
        boundary = boundary.min(next);
    }
    let rates = contact_rates(rules, state, input.exposure.infectious_contacts);
    for (index, kind) in [IllnessKind::Enteric, IllnessKind::Respiratory]
        .into_iter()
        .enumerate()
    {
        if state.illnesses.contains_key(&kind) {
            continue;
        }
        let dose = state.doses.get(&kind).cloned().unwrap_or_default();
        let current = u128::from(dose.amount) * 1_000 + u128::from(dose.remainder);
        let threshold = u128::from(rules.infection_threshold) * 1_000;
        if rates[index] > 0
            && current < threshold
            && let Some(next) = at((threshold - current).div_ceil(rates[index]))
        {
            boundary = boundary.min(next);
        }
    }
    Ok(boundary)
}
pub fn advance_job(
    job: &Job,
    elapsed: u64,
    can_progress: bool,
    medicine: u16,
) -> Result<Job, Error> {
    let mut job = job.clone();
    if !can_progress {
        job.completes = SimTime(
            job.completes
                .0
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?,
        );
        return Ok(job);
    }
    if let Work::Treat {
        skill_ms,
        elapsed: spent,
        ..
    } = &mut job.work
    {
        *skill_ms = skill_ms
            .checked_add(u128::from(medicine) * u128::from(elapsed))
            .ok_or(Error::Overflow)?;
        *spent = spent.checked_add(elapsed).ok_or(Error::Overflow)?;
    }
    Ok(job)
}
pub fn treatment_course(
    rules: &Rules,
    now: SimTime,
    skill_ms: u128,
    elapsed: u64,
) -> Result<Course, Error> {
    let average = if elapsed == 0 {
        0
    } else {
        skill_ms / u128::from(elapsed)
    };
    let bonus = average.saturating_mul(25).min(1_000) as u16;
    let healing = rules.treatment_healing.saturating_add(bonus);
    let until = SimTime(
        now.0
            .checked_add(rules.treatment_duration.0)
            .ok_or(Error::Overflow)?,
    );
    Ok(Course { until, healing })
}
pub fn shelter_cost(rules: &Rules, capacity: u32, insulation: u16) -> Result<Quantity, Error> {
    if !(1..=64).contains(&capacity) || !(1..=1_000).contains(&insulation) {
        return Err(Error::Unavailable);
    }
    let cost = u128::from(rules.shelter_timber_per_place.0)
        * u128::from(capacity)
        * u128::from(500 + insulation);
    Ok(Quantity(
        u64::try_from(cost.div_ceil(1_000)).map_err(|_| Error::Overflow)?,
    ))
}

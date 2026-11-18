//! Deadlines for individual cases in simulated time.
use super::{Case, Damage, Disease, Draft, Event, EventKind, Strain};
use rayon::prelude::*;

impl Case {
    pub(crate) fn elapsed_age(&self, now: u128, unit: u64) -> Option<u128> {
        now.checked_sub(self.infected_at_millis.unwrap_or(0))
            .map(|age| age / u128::from(unit))
    }
}

impl Strain {
    fn elapsed_stages(&self, unit: u64) -> (u128, u128, u128) {
        let unit = u128::from(unit);
        let incubation = u128::from(self.incubation) * unit;
        let recovery = (u128::from(self.incubation) + u128::from(self.infectious_ticks)) * unit;
        let expiry = u128::from(self.end().expect("validated strain")) * unit;
        (incubation, recovery, expiry)
    }
}

impl Disease {
    pub fn stamp_treatments(&mut self, tick: u64, at: u128) {
        if self.time_unit_millis.is_some() {
            self.events
                .par_iter_mut()
                .filter(|event| {
                    event.tick == tick
                        && event.kind == EventKind::Treated
                        && event.at_millis.is_none()
                })
                .for_each(|event| event.at_millis = Some(at));
        }
    }

    pub fn validate_elapsed(&self, now: u128) -> Result<(), String> {
        if self.time_unit_millis == Some(0)
            || self
                .cases
                .par_iter()
                .any(|case| match case.infected_at_millis {
                    Some(at) => self.time_unit_millis.is_none() || at > now,
                    None => self.time_unit_millis.is_some() && case.infected_at != 0,
                })
            || self.events.par_iter().any(|event| {
                event
                    .at_millis
                    .is_some_and(|at| self.time_unit_millis.is_none() || at > now)
            })
        {
            return Err("invalid elapsed disease clock".into());
        }
        Ok(())
    }

    pub fn next_elapsed_deadline(&self, now: u128) -> Option<u128> {
        let unit = self.time_unit_millis?;
        self.cases
            .par_iter()
            .filter_map(|case| {
                let age = now.checked_sub(case.infected_at_millis.unwrap_or(0))?;
                let strain = self
                    .strains
                    .iter()
                    .find(|strain| strain.id == case.strain)?;
                let (incubation, recovery, expiry) = strain.elapsed_stages(unit);
                if age >= expiry {
                    return None;
                }
                if age >= recovery {
                    return Some(expiry - age);
                }
                if strain.damage_per_tick == 0 {
                    return Some(recovery - age);
                }
                let period = u128::from(unit);
                let next_damage = (age.max(incubation) / period + 1) * period;
                Some(next_damage - age)
            })
            .min()
    }

    /// Stage the consequences at the earliest elapsed case deadline.
    pub fn stage_elapsed(
        &self,
        health: impl Fn(u64) -> Option<u32> + Sync,
        now: u128,
        tick: u64,
    ) -> Option<Draft> {
        let unit = self.time_unit_millis?;
        let mut cases: Vec<_> = self.cases.par_iter().collect();
        if cases
            .par_windows(2)
            .any(|pair| (pair[0].person, pair[0].strain) > (pair[1].person, pair[1].strain))
        {
            cases.par_sort_unstable_by_key(|case| (case.person, case.strain));
        }
        let rows: Vec<_> = cases
            .par_chunk_by(|first, second| first.person == second.person)
            .filter_map(|cases| {
                let id = cases[0].person;
                let health = health(id)?;
                if health == 0 {
                    return None;
                }
                let mut damage = 0u32;
                let mut retained = Vec::new();
                let mut events = Vec::new();
                for case in cases {
                    let strain = self
                        .strains
                        .iter()
                        .find(|strain| strain.id == case.strain)
                        .expect("validated strain");
                    let (incubation, recovery, expiry) = strain.elapsed_stages(unit);
                    let age = now - case.infected_at_millis.unwrap_or(0);
                    if age > incubation && age <= recovery && age.is_multiple_of(u128::from(unit)) {
                        damage = damage.saturating_add(strain.damage_per_tick);
                    }
                    if age == recovery {
                        events.push(Event {
                            tick,
                            at_millis: Some(now),
                            person: id,
                            strain: case.strain,
                            source: case.source,
                            kind: EventKind::Recovered,
                        });
                    }
                    if age < expiry {
                        retained.push((*case).clone());
                    }
                }
                if damage >= health {
                    retained.clear();
                    events.clear();
                }
                Some((
                    retained,
                    events,
                    (damage > 0).then_some(Damage {
                        person: id,
                        amount: damage,
                    }),
                ))
            })
            .collect();
        let (cases, effects): (Vec<_>, Vec<_>) = rows
            .into_par_iter()
            .map(|(cases, events, injury)| (cases, (events, injury)))
            .unzip();
        let (events, damage): (Vec<_>, Vec<_>) = effects.into_par_iter().unzip();
        let state = Self {
            time_unit_millis: self.time_unit_millis,
            strains: self.strains.clone(),
            treatments: self.treatments.clone(),
            cases: cases.into_par_iter().flatten().collect(),
            events: events.into_par_iter().flatten().collect(),
        };
        let damage = damage.into_par_iter().flatten().collect();
        Some(Draft { state, damage })
    }
}

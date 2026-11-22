//! Personal medical assistance, physical medicine consumption, and actual contact.
use super::*;

impl CoreHost {
    pub(super) fn next_disease_deadline(&self) -> Option<u64> {
        let health = self.health.as_ref()?;
        u64::try_from(
            health
                .owner
                .next_elapsed_deadline(self.core.simulated_millis())?,
        )
        .ok()
    }

    pub(super) fn stage_elapsed_disease(
        &self,
        people: &mut [game_economy_person::Person],
        now: u128,
    ) -> Option<(CoreHealth, Vec<u64>)> {
        let health = self.health.as_ref()?;
        let draft = health.owner.stage_elapsed(
            |person| {
                people
                    .binary_search_by_key(&person, |row| row.id)
                    .ok()
                    .map(|index| people[index].health)
            },
            now,
            self.core.physical.tick(),
        )?;
        let casualties = people.par_iter_mut().filter_map(|person| {
            let index = draft
                .damage
                .binary_search_by_key(&person.id, |row| row.person)
                .ok()?;
            let before = person.health;
            person.health = before.saturating_sub(draft.damage[index].amount);
            (before > 0 && person.health == 0).then_some(person.id)
        }).collect();
        let health = CoreHealth {
            owner: draft.state,
            treatment: health.treatment.clone(),
            last_tick: health.last_tick,
        };
        Some((health, casualties))
    }

    pub(super) fn plan_medical(
        &self,
        external: &[u64],
        occupied: &[u64],
        dependents: &[u64],
        base: u64,
    ) -> Result<Option<crate::medical::Plan>, String> {
        let Some(owner) = &self.medical else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let communication = game_economy_confinement::Communication {
            before: self.confinement(),
            after: None,
        };
        let resting = self
            .fatigue
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.fatigue.resting());
        crate::medical::Context {
            owner: Some(owner),
            disease: self.health.as_ref().map(|health| &health.owner),
            injury_medicine: None,
            injuries: None,
            social: self.social.as_ref().map(|social| &social.owner),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            goods: self.core.physical.goods(),
            view: &view,
            busy: &busy,
            blocked: external,
            available_patients: dependents,
            communication: &communication,
            resting: &resting,
            seed: self.core.seed,
            tick,
            max_health: self.core.rules.max_health,
            manual: None,
            locations: &locations,
            walking: self.walking(),
        }
        .prepare(base)
        .map(Some)
    }

    pub(super) fn stage_contact_health(
        &self,
        staged: Option<CoreHealth>,
        contacts: &[(u64, u64)],
        tick: u64,
    ) -> Result<Option<CoreHealth>, String> {
        let Some(health) = self.health.as_ref().filter(|health| {
            !contacts.is_empty()
                && !health.owner.cases.is_empty()
                && health
                    .owner
                    .strains
                    .iter()
                    .any(|strain| strain.transmission_per_million > 0)
        }) else {
            return Ok(staged);
        };
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let mut encounters: Vec<_> = contacts
            .par_iter()
            .map(|&(actor, patient)| -> Result<_, String> {
                let first = self
                    .core
                    .people
                    .binary_search_by_key(&actor, |row| row.id)
                    .map_err(|_| "unknown contact actor")?;
                let second = self
                    .core
                    .people
                    .binary_search_by_key(&patient, |row| row.id)
                    .map_err(|_| "unknown contact recipient")?;
                let a = &self.core.people[first];
                let b = &self.core.people[second];
                let region = locations
                    .region((a.id, a.region), (b.id, b.region))
                    .ok_or("contact lost location")?;
                Ok(game_social::Encounter {
                    tick,
                    region,
                    a: actor.min(patient),
                    b: actor.max(patient),
                    pressured: None,
                    also_pressured: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        encounters.par_sort_unstable_by_key(|row| (row.a, row.b));
        encounters.dedup_by_key(|row| (row.a, row.b));
        self.stage_encounter_health(health, staged, &encounters, tick)
            .map(|(staged, _)| Some(staged))
    }

    pub(super) fn stage_encounter_health(
        &self,
        health: &CoreHealth,
        staged: Option<CoreHealth>,
        encounters: &[game_social::Encounter],
        tick: u64,
    ) -> Result<(CoreHealth, Vec<game_disease::Event>), String> {
        let people = disease_people(&self.core.people);
        let mut next = staged.unwrap_or_else(|| health.clone());
        let events = if health.owner.time_unit_millis.is_some() {
            next.owner.admit_elapsed_contacts(
                &health.owner,
                &people,
                encounters,
                self.core.seed,
                tick,
                self.core.simulated_millis(),
            )
        } else {
            next.owner
                .admit_contacts_validated(
                    &health.owner,
                    &people,
                    encounters,
                    self.core.seed,
                    tick - 1,
                )
                .map(|()| Vec::new())
        }
        .map_err(str::to_owned)?;
        Ok((next, events))
    }

    pub(super) fn stage_medical_health(
        &self,
        staged: Option<CoreHealth>,
        resolved: Option<&crate::medical::Resolved>,
        tick: u64,
    ) -> Option<CoreHealth> {
        let Some((health, resolved)) = self
            .health
            .as_ref()
            .zip(resolved)
            .filter(|(_, resolved)| !resolved.is_empty())
        else {
            return staged;
        };
        let mut next = staged.unwrap_or_else(|| health.clone());
        resolved.apply(Some(&mut next.owner), tick);
        next.owner
            .stamp_treatments(tick, self.core.simulated_millis());
        Some(next)
    }
}

fn disease_people(people: &[game_economy_person::Person]) -> Vec<game_disease::Person> {
    people
        .par_iter()
        .map(|person| game_disease::Person {
            id: person.id,
            health: person.health,
        })
        .collect()
}

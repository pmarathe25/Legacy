//! Personal medical assistance, physical medicine consumption, and actual contact.
use super::*;

impl CoreHost {
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
            before: None,
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
            walking: None,
        }
        .prepare(base)
        .map(Some)
    }

    pub(super) fn stage_medical_health(
        &self,
        staged: Option<CoreHealth>,
        resolved: Option<&crate::medical::Resolved>,
        events: &game_lot_runtime::Events,
        tick: u64,
    ) -> Result<Option<CoreHealth>, String> {
        let Some((health, resolved)) = self.health.as_ref().zip(resolved) else {
            return Ok(staged);
        };
        let contacts = resolved.contacts();
        if resolved.is_empty() {
            return Ok(staged);
        }
        let mut next = staged.unwrap_or_else(|| health.clone());
        if !contacts.is_empty()
            && health
                .owner
                .strains
                .iter()
                .any(|strain| strain.transmission_per_million > 0)
        {
            let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
            let mut encounters: Vec<_> = contacts
                .par_iter()
                .map(|&(actor, patient)| -> Result<_, String> {
                    let first = self
                        .core
                        .people
                        .binary_search_by_key(&actor, |row| row.id)
                        .map_err(|_| "unknown medical actor")?;
                    let second = self
                        .core
                        .people
                        .binary_search_by_key(&patient, |row| row.id)
                        .map_err(|_| "unknown medical patient")?;
                    let a = &self.core.people[first];
                    let b = &self.core.people[second];
                    let region = locations
                        .region((a.id, a.region), (b.id, b.region))
                        .ok_or("medical contact lost location")?;
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
            let people: Vec<_> = self
                .core
                .people
                .par_iter()
                .map(|person| game_disease::Person {
                    id: person.id,
                    health: person.health,
                })
                .collect();
            next.owner
                .admit_contacts_validated(
                    &health.owner,
                    &people,
                    &encounters,
                    self.core.seed,
                    tick - 1,
                )
                .map_err(str::to_owned)?;
        }
        resolved.apply(Some(&mut next.owner), events.tick);
        Ok(Some(next))
    }
}

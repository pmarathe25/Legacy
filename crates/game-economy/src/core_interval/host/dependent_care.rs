//! Staged care, including real food transfers to dependents.
use super::*;

impl CoreHost {
    pub(super) fn stage_care_health(
        &self,
        staged: Option<CoreHealth>,
        plan: Option<&game_economy_work::care::Plan>,
        tick: u64,
    ) -> Result<Option<CoreHealth>, String> {
        let Some((health, plan)) = self.health.as_ref().zip(plan).filter(|(health, _)| {
            !health.owner.cases.is_empty()
                && health
                    .owner
                    .strains
                    .iter()
                    .any(|strain| strain.transmission_per_million > 0)
        }) else {
            return Ok(staged);
        };
        let mut contacts: Vec<_> = plan
            .draft
            .events
            .par_iter()
            .filter_map(|event| {
                event.caregiver.map(|caregiver| -> Result<_, String> {
                    let (a, b) = if caregiver < event.child {
                        (caregiver, event.child)
                    } else {
                        (event.child, caregiver)
                    };
                    let person = self
                        .core
                        .people
                        .binary_search_by_key(&caregiver, |person| person.id)
                        .map_err(|_| "unknown caregiver")?;
                    let region = self.core.people[person]
                        .region
                        .or_else(|| {
                            let travel = self.core.travel.as_ref()?;
                            let journey = travel
                                .journeys
                                .binary_search_by_key(&caregiver, |journey| journey.person)
                                .ok()?;
                            Some(travel.journeys[journey].source)
                        })
                        .ok_or("caregiver has no contact location")?;
                    Ok(game_social::Encounter {
                        tick,
                        region,
                        a,
                        b,
                        pressured: None,
                        also_pressured: None,
                    })
                })
            })
            .collect::<Result<_, _>>()?;
        if contacts.is_empty() {
            return Ok(staged);
        }
        contacts.par_sort_unstable_by_key(|event| (event.a, event.b));
        contacts.dedup_by_key(|event| (event.a, event.b));
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| game_disease::Person {
                id: person.id,
                health: person.health,
            })
            .collect();
        let mut next = staged.unwrap_or_else(|| health.clone());
        next.owner
            .admit_contacts_validated(&health.owner, &people, &contacts, self.core.seed, tick - 1)
            .map_err(str::to_owned)?;
        Ok(Some(next))
    }

    pub(super) fn plan_dependent_care(
        &self,
        occupied: &[u64],
    ) -> Result<Option<game_economy_work::care::Plan>, String> {
        let Some((care, family)) = self.dependent_care.as_ref().zip(self.family.as_ref()) else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| (person.id, person.region, person.health, person.hunger))
            .collect();
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        game_economy_work::care::Context {
            care,
            family,
            people: &people,
            rules: &self.core.rules,
            goods: self.core.physical.goods(),
            view: &view,
            travel: self.core.travel.as_ref(),
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            access: game_economy_work::care::Access {
                contact: &|_, _| true,
                local_stock: &|_| true,
            },
            eating: &[],
            seed: self.core.seed,
            tick,
        }
        .prepare(1)
        .map(Some)
    }
}

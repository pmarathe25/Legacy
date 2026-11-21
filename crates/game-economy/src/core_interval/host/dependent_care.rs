//! Staged care, including real food transfers to dependents.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCareVisit {
    pub at_millis: u128,
    pub events: Vec<game_economy_work::care::Event>,
    pub transfers: Vec<game_lots::TransactionOutcome>,
    pub incapacitated: Vec<u64>,
}

impl CoreHost {
    pub(super) fn next_elapsed_care(&self) -> Option<u64> {
        let instant = self.dependent_care.as_ref()?.elapsed?.next_millis()?;
        u64::try_from(instant.saturating_sub(self.core.simulated_millis())).ok()
    }

    pub(super) fn publish_elapsed_care(
        &mut self,
        occupied: &[u64],
    ) -> Result<CoreCareVisit, String> {
        let owner = self
            .dependent_care
            .as_ref()
            .ok_or("missing elapsed care owner")?;
        let clock = owner.elapsed.ok_or("missing elapsed care clock")?;
        let at = self.core.simulated_millis();
        if clock.next_millis() != Some(at) {
            return Err("care visit is outside its elapsed deadline".into());
        }
        let sequence = u64::try_from(at / u128::from(clock.period_millis))
            .map_err(|_| "care sequence overflow")?;
        let tick = self.core.physical.tick();
        let plan = self
            .plan_care_visits(occupied, tick, Some(sequence))?
            .ok_or("elapsed care requires family")?;
        let damage = owner.rules.neglect_damage;
        let mut harm: Vec<_> = plan
            .draft
            .events
            .par_iter()
            .filter(|row| row.caregiver.is_none() && damage != 0)
            .filter_map(|row| {
                let index = self
                    .core
                    .people
                    .binary_search_by_key(&row.child, |person| person.id)
                    .ok()?;
                let person = &self.core.people[index];
                (person.health != 0).then_some((index, person.health.saturating_sub(damage)))
            })
            .collect();
        harm.par_sort_unstable_by_key(|row| row.0);
        let generation = (!harm.is_empty())
            .then(|| {
                self.core
                    .generation
                    .checked_add(1)
                    .ok_or("core revision overflow")
            })
            .transpose()?;
        let incapacitated: Vec<_> = harm
            .par_iter()
            .filter_map(|&(index, health)| (health == 0).then_some(self.core.people[index].id))
            .collect();
        let mut pending = self.pending_vital_deaths.clone();
        pending.par_extend(incapacitated.par_iter().copied());
        pending.par_sort_unstable();
        pending.dedup();
        let legacy_contacts = self
            .health
            .as_ref()
            .is_some_and(|health| health.owner.time_unit_millis.is_none());
        let contact_tick = tick
            .checked_add(u64::from(legacy_contacts))
            .ok_or("care contact tick overflow")?;
        let health = self.stage_care_health(None, Some(&plan), contact_tick)?;
        let mut transfers = Vec::new();
        let mut claims = None;
        if !plan.actions.is_empty() {
            let draft = self
                .core
                .physical
                .prepare_interval_transfer(&plan.actions, self.core.seed)?;
            claims = self
                .claims
                .as_ref()
                .filter(|_| !plan.uses.is_empty())
                .map(|owner| {
                    let mut claims = owner.clone();
                    let mut opinions = self.opinions.clone();
                    game_economy_claims::publish_transactions(
                        &mut claims,
                        &plan.uses,
                        draft.transactions(),
                        &self.core.people,
                        &mut opinions,
                        tick,
                        Some(&|person, region| self.regional_stock_reachable(person, region)),
                    );
                    (claims, opinions)
                });
            transfers = self
                .core
                .physical
                .commit_interval_consumption(draft)?
                .transactions;
        }
        if let Some((claims, opinions)) = claims {
            self.claims = Some(claims);
            self.opinions = opinions;
        }
        if let Some(health) = health {
            self.health = Some(health);
        }
        if let Some(generation) = generation {
            self.core
                .people
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, person)| {
                    if let Ok(slot) = harm.binary_search_by_key(&index, |row| row.0) {
                        person.health = harm[slot].1;
                    }
                });
            self.core.generation = generation;
            self.pending_vital_deaths = pending;
            if !incapacitated.is_empty() {
                self.remove_deceased_holds();
            }
        }
        let events = plan.draft.events.clone();
        let owner = self.dependent_care.as_mut().expect("checked care");
        plan.publish(owner);
        owner
            .elapsed
            .as_mut()
            .expect("checked elapsed clock")
            .last_millis = Some(at);
        self.work_checked_revision = None;
        Ok(CoreCareVisit {
            at_millis: at,
            events,
            transfers,
            incapacitated,
        })
    }

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
        self.stage_encounter_health(health, staged, &contacts, tick)
            .map(|(staged, _)| Some(staged))
    }

    pub(super) fn plan_dependent_care(
        &self,
        occupied: &[u64],
    ) -> Result<Option<game_economy_work::care::Plan>, String> {
        if self
            .dependent_care
            .as_ref()
            .is_none_or(|owner| owner.elapsed.is_some())
        {
            return Ok(None);
        }
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        self.plan_care_visits(occupied, tick, None)
    }

    fn plan_care_visits(
        &self,
        occupied: &[u64],
        tick: u64,
        sequence: Option<u64>,
    ) -> Result<Option<game_economy_work::care::Plan>, String> {
        let Some((care, family)) = self.dependent_care.as_ref().zip(self.family.as_ref()) else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
        busy.par_extend(
            self.core
                .people
                .par_iter()
                .filter_map(|person| person.controlled.then_some(person.id)),
        );
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
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let context = game_economy_work::care::Context {
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
                contact: &|a, b| self.same_local_endpoint(a, b),
                local_stock: &|person| {
                    self.core
                        .people
                        .binary_search_by_key(&person, |row| row.id)
                        .ok()
                        .and_then(|slot| self.core.people[slot].region)
                        .is_some_and(|region| self.regional_stock_reachable(person, region))
                },
            },
            eating: &[],
            seed: self.core.seed,
            tick,
        };
        let now = self.core.simulated_millis();
        if let Some(sequence) = sequence {
            context.prepare_elapsed_at(1, now, sequence).map(Some)
        } else {
            context.prepare_at(1, Some(now)).map(Some)
        }
    }
}

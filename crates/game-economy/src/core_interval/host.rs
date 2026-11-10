//! A saved host for the limited fractional-time core and its personal owners.
use super::*;
use game_autowork::Autowork;
use game_economy_work::tool_wear::ToolWear;
mod agriculture;
mod animal_care;
mod claims;
mod fatigue;
mod fauna;
mod food_search;
mod gathering;
mod hunting;
mod offers;
mod requests;
mod scheduler;
mod sites;
mod social;
mod tools;
mod treatment;
pub use food_search::{CoreFoodProvision, CoreFoodSearch};
pub use offers::CoreSocialResults;
pub use requests::{CoreDeliveryResults, CoreRequestResults};
pub use scheduler::{CoreRun, CoreRunEvent, CoreSchedule, DecisionPhase};
pub use sites::{CoreRepairResults, CoreSites};
pub use social::{CoreDynamicsResults, CoreHealth, CoreSocial};
pub use tools::CoreToolRepairResults;
pub use treatment::CoreTreatmentResults;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreHostSnapshot {
    pub core: CoreSnapshot,
    #[serde(default)]
    pub autowork: Option<Autowork>,
    #[serde(default)]
    pub tool_wear: Option<ToolWear>,
    #[serde(default)]
    pub tool_requirements: Vec<game_economy_work::tools::Requirement>,
    #[serde(default, alias = "tool_paused_rates")]
    pub paused_work_rates: Vec<(u64, u64)>,
    #[serde(default)]
    pub fatigue: Option<game_economy_fatigue::TimedFatigue>,
    #[serde(default)]
    pub activity: Vec<game_economy_fatigue::TimedActivity>,
    #[serde(default)]
    pub food_search: Option<CoreFoodSearch>,
    #[serde(default)]
    pub claims: Option<game_economy_claims::Claims>,
    #[serde(default)]
    pub social: Option<CoreSocial>,
    #[serde(default)]
    pub knowledge: Option<game_economy_knowledge::Knowledge>,
    #[serde(default)]
    pub health: Option<CoreHealth>,
    #[serde(default)]
    pub agriculture: Option<game_economy_agriculture::Agriculture>,
    #[serde(default)]
    pub irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    #[serde(default)]
    pub environment: Option<game_environment::Environment>,
    #[serde(default)]
    pub gathering: Option<crate::Gathering>,
    #[serde(default)]
    pub fauna: Option<game_fauna::Fauna>,
    #[serde(default)]
    pub hunting: Option<crate::Hunting>,
    #[serde(default)]
    pub animal_feelings: Option<crate::AnimalFeelings>,
    #[serde(default)]
    pub animal_care: Option<crate::AnimalCare>,
    #[serde(default)]
    pub opinions: Option<game_opinions::Opinions>,
    #[serde(default)]
    pub goals: Option<game_goals::Goals>,
    #[serde(default)]
    pub agreements: Option<game_agreements::Agreements>,
    #[serde(default)]
    pub sites: Option<CoreSites>,
    #[serde(default)]
    pub schedule: Option<CoreSchedule>,
}

/// This host composes only the owners listed in `CoreHostSnapshot`.
pub struct CoreHost {
    core: CoreInterval,
    autowork: Option<Autowork>,
    tool_wear: Option<ToolWear>,
    tool_requirements: Vec<game_economy_work::tools::Requirement>,
    paused_work_rates: Vec<(u64, u64)>,
    fatigue: Option<game_economy_fatigue::TimedFatigue>,
    activity: Vec<game_economy_fatigue::TimedActivity>,
    food_search: Option<CoreFoodSearch>,
    claims: Option<game_economy_claims::Claims>,
    social: Option<CoreSocial>,
    knowledge: Option<game_economy_knowledge::Knowledge>,
    health: Option<CoreHealth>,
    agriculture: Option<game_economy_agriculture::Agriculture>,
    irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    environment: Option<game_environment::Environment>,
    gathering: Option<crate::Gathering>,
    fauna: Option<game_fauna::Fauna>,
    hunting: Option<crate::Hunting>,
    animal_feelings: Option<crate::AnimalFeelings>,
    animal_care: Option<crate::AnimalCare>,
    work_checked_revision: Option<u64>,
    work_checked_blocked: Vec<u64>,
    opinions: Option<game_opinions::Opinions>,
    goals: Option<game_goals::Goals>,
    agreements: Option<game_agreements::Agreements>,
    sites: Option<CoreSites>,
    schedule: Option<CoreSchedule>,
}
struct CoreRuntimeOptions {
    sites: Option<CoreSites>,
    schedule: Option<CoreSchedule>,
    tool_requirements: Vec<game_economy_work::tools::Requirement>,
    paused_work_rates: Vec<(u64, u64)>,
    fatigue: Option<game_economy_fatigue::TimedFatigue>,
    activity: Vec<game_economy_fatigue::TimedActivity>,
    food_search: Option<CoreFoodSearch>,
    claims: Option<game_economy_claims::Claims>,
    social: Option<CoreSocial>,
    knowledge: Option<game_economy_knowledge::Knowledge>,
    health: Option<CoreHealth>,
    agriculture: Option<game_economy_agriculture::Agriculture>,
    irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    environment: Option<game_environment::Environment>,
    gathering: Option<crate::Gathering>,
    fauna: Option<game_fauna::Fauna>,
    hunting: Option<crate::Hunting>,
    animal_feelings: Option<crate::AnimalFeelings>,
    animal_care: Option<crate::AnimalCare>,
}
impl CoreHost {
    pub fn begin(snapshot: CoreHostSnapshot, rates: &[(u64, u64)]) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            food_search,
            claims,
            social,
            knowledge,
            health,
            agriculture,
            irrigation,
            environment,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            opinions,
            goals,
            agreements,
            sites,
            schedule,
        } = snapshot;
        let core = CoreInterval::begin(core, rates)?;
        Self::compile(
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            CoreRuntimeOptions {
                sites,
                schedule,
                tool_requirements,
                paused_work_rates,
                fatigue,
                activity,
                food_search,
                claims,
                social,
                knowledge,
                health,
                agriculture,
                irrigation,
                environment,
                gathering,
                fauna,
                hunting,
                animal_feelings,
                animal_care,
            },
        )
    }
    pub fn resume(snapshot: CoreHostSnapshot) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            food_search,
            claims,
            social,
            knowledge,
            health,
            agriculture,
            irrigation,
            environment,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            opinions,
            goals,
            agreements,
            sites,
            schedule,
        } = snapshot;
        let core = CoreInterval::resume(core)?;
        Self::compile(
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            CoreRuntimeOptions {
                sites,
                schedule,
                tool_requirements,
                paused_work_rates,
                fatigue,
                activity,
                food_search,
                claims,
                social,
                knowledge,
                health,
                agriculture,
                irrigation,
                environment,
                gathering,
                fauna,
                hunting,
                animal_feelings,
                animal_care,
            },
        )
    }
    fn compile(
        core: CoreInterval,
        autowork: Option<Autowork>,
        tool_wear: Option<ToolWear>,
        opinions: Option<game_opinions::Opinions>,
        goals: Option<game_goals::Goals>,
        agreements: Option<game_agreements::Agreements>,
        options: CoreRuntimeOptions,
    ) -> Result<Self, String> {
        let CoreRuntimeOptions {
            mut sites,
            schedule,
            tool_requirements,
            paused_work_rates,
            fatigue,
            mut activity,
            mut food_search,
            mut claims,
            social,
            mut knowledge,
            health,
            agriculture,
            irrigation,
            environment,
            gathering,
            fauna,
            hunting,
            mut animal_feelings,
            animal_care,
        } = options;
        if let Some(legacy) = food_search.as_mut().and_then(|owner| owner.claims.take()) {
            if claims.is_some() {
                return Err("duplicate personal claim owners".into());
            }
            claims = Some(legacy);
        }
        if let Some(legacy) = food_search
            .as_mut()
            .and_then(|owner| owner.knowledge.take())
        {
            if knowledge.is_some() {
                return Err("duplicate personal knowledge owners".into());
            }
            knowledge = Some(legacy);
        }
        if let Some(owner) = &autowork {
            let people: Vec<_> = core
                .people
                .par_iter()
                .map(|p| (p.id, p.region, p.health, p.hunger))
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            let production =
                game_lot_projection::production(&core.physical, &view, core.rules.max_work_hunger);
            owner.validate(&people, &production)?;
            if owner.next_id == 0 {
                return Err("invalid autonomous work identity".into());
            }
        }
        if let Some(owner) = &tool_wear {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&core.physical, &goods, &people, core.physical.tick())?;
        }
        if !tool_requirements.is_empty() {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            game_economy_work::tools::validate(&tool_requirements, &recipes, &goods)?;
        }
        let rates = &core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates;
        if (tool_requirements.is_empty()
            && fatigue.is_none()
            && health.is_none()
            && !paused_work_rates.is_empty())
            || paused_work_rates
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || paused_work_rates.par_iter().any(|&(job, rate)| {
                rate == 0
                    || rates.binary_search_by_key(&job, |row| row.0).is_ok()
                    || core
                        .physical
                        .work()
                        .binary_search_by_key(&job, |row| row.job.id)
                        .ok()
                        .is_none_or(|index| {
                            core.physical.work()[index].phase != game_lot_runtime::Phase::Working
                        })
            })
        {
            return Err("invalid paused work".into());
        }
        if let Some(owner) = &fatigue {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&people, core.physical.tick_millis())?;
            let elapsed = core.elapsed_millis().ok_or("no physical interval")?;
            if activity.is_empty() && elapsed == 0 {
                activity = core
                    .people
                    .par_iter()
                    .map(|person| game_economy_fatigue::TimedActivity {
                        id: person.id,
                        ..Default::default()
                    })
                    .collect();
            }
            if activity.len() != core.people.len()
                || activity
                    .par_iter()
                    .zip(core.people.par_iter())
                    .any(|(row, person)| {
                        row.id != person.id
                            || row.alive_millis > elapsed
                            || u128::from(row.work_millis)
                                + u128::from(row.travel_millis)
                                + u128::from(row.rest_millis)
                                + u128::from(row.idle_millis)
                                > u128::from(row.alive_millis)
                    })
            {
                return Err("invalid saved fatigue activity".into());
            }
        } else if !activity.is_empty() {
            return Err("fatigue activity has no owner".into());
        }
        if let Some(owner) = &opinions {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner
                .validate(&people, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &knowledge {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            game_economy_knowledge::validate(Some(owner), &people, &view, core.physical.tick())?;
        }
        if let Some(owner) = &food_search {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner
                .search
                .validate(&people, knowledge.as_ref(), core.travel.as_ref())?;
        }
        if let Some(claims) = &claims {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            claims.validate(&people, &view, core.physical.tick())?;
        }
        if let Some(social) = &social {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            social
                .owner
                .validate(&people, core.physical.tick())
                .map_err(str::to_owned)?;
            if social
                .last_tick
                .is_some_and(|tick| tick == 0 || tick > core.physical.tick())
                || social
                    .owner
                    .events
                    .par_iter()
                    .any(|event| Some(event.tick) != social.last_tick)
            {
                return Err("invalid social decision time".into());
            }
        }
        if let Some(health) = &health {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            health.owner.validate(&people, core.physical.tick())?;
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            if health
                .owner
                .treatments
                .par_iter()
                .any(|treatment| !goods.contains(&treatment.medicine))
                || health
                    .last_tick
                    .is_some_and(|tick| tick == 0 || tick > core.physical.tick())
            {
                return Err("invalid disease decision state".into());
            }
            game_economy_treatment::validate(
                &health.treatment,
                Some(&health.owner),
                &people,
                &goods,
            )?;
        }
        if let Some(owner) = &agriculture {
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&core.people, core.physical.goods(), &view)?;
        }
        if let Some(owner) = &irrigation {
            let agriculture = agriculture
                .as_ref()
                .ok_or("irrigation requires agriculture")?;
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(agriculture, &people, &goods, core.physical.tick())?;
        }
        if environment.is_some() {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            crate::environment::validate(environment.as_ref(), &view, &goods)?;
        }
        if let Some(owner) = &gathering {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&people, &goods, &view)?;
        }
        if let Some(owner) = &fauna {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|row| match row.place {
                    game_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            owner
                .validate(&regions, &goods, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &mut animal_feelings {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.normalize_and_validate(&people, fauna.as_ref())?;
        }
        if let Some(owner) = &animal_care {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, fauna.as_ref())?;
        }
        if let Some(owner) = &hunting {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, fauna.as_ref())?;
        }
        if let Some(owner) = &goals {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            owner
                .validate(&people, &recipes, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &agreements {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner
                .validate(&people, &goods, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &mut sites {
            owner.validate(&core)?;
            if let Some(maintenance) = &mut owner.maintenance {
                maintenance.enroll(&owner.facilities);
            }
        }
        if let Some(schedule) = &schedule {
            schedule.validate(&core)?;
        }
        Ok(Self {
            core,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            food_search,
            claims,
            social,
            knowledge,
            health,
            agriculture,
            irrigation,
            environment,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            work_checked_revision: None,
            work_checked_blocked: Vec::new(),
            opinions,
            goals,
            agreements,
            sites,
            schedule,
        })
    }
    pub fn snapshot(&self) -> CoreHostSnapshot {
        CoreHostSnapshot {
            core: self.core.snapshot(),
            autowork: self.autowork.clone(),
            tool_wear: self.tool_wear.clone(),
            tool_requirements: self.tool_requirements.clone(),
            paused_work_rates: self.paused_work_rates.clone(),
            fatigue: self.fatigue.clone(),
            activity: self.activity.clone(),
            food_search: self.food_search.clone(),
            claims: self.claims.clone(),
            social: self.social.clone(),
            knowledge: self.knowledge.clone(),
            health: self.health.clone(),
            agriculture: self.agriculture.clone(),
            irrigation: self.irrigation.clone(),
            environment: self.environment.clone(),
            gathering: self.gathering.clone(),
            fauna: self.fauna.clone(),
            hunting: self.hunting.clone(),
            animal_feelings: self.animal_feelings.clone(),
            animal_care: self.animal_care.clone(),
            opinions: self.opinions.clone(),
            goals: self.goals.clone(),
            agreements: self.agreements.clone(),
            sites: self.sites.clone(),
            schedule: self.schedule.clone(),
        }
    }
    pub fn core(&self) -> &CoreInterval {
        &self.core
    }
    pub fn progress(&mut self, millis: u64) -> Result<u64, String> {
        if self.schedule.is_some() {
            return Err("use scheduled advancement while a schedule is active".into());
        }
        if self
            .core
            .elapsed_millis()
            .is_some_and(|offset| offset < self.core.physical.tick_millis())
        {
            self.sync_work_rates(&[])?;
        }
        self.progress_with_activity(millis, &[])
    }
    pub fn next_deadline(&mut self, hunger: Option<u32>) -> Result<CoreDeadline, String> {
        if self
            .core
            .elapsed_millis()
            .is_some_and(|offset| offset < self.core.physical.tick_millis())
        {
            self.sync_work_rates(&[])?;
        }
        self.core.next_deadline(hunger)
    }
    pub fn publish_arrivals(&mut self) -> Result<Option<CoreArrivals>, String> {
        self.core.publish_arrivals()
    }
    /// Start journeys selected by the enclosing decision owner.
    pub fn depart(
        &mut self,
        requests: &[travel::Departure],
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let outcomes = self.core.depart(tick, offset, requests, occupied)?;
        self.work_checked_revision = None;
        Ok(outcomes)
    }
    /// Let each eligible person pursue their own configured destination.
    pub fn depart_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        if occupied.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(travel) = &self.core.travel else {
            return Ok(Vec::new());
        };
        if travel.destinations.is_empty() {
            return Ok(Vec::new());
        }
        let occupied_rest = self.blocked_with_rest(occupied);
        let occupied = occupied_rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
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
        let mut routes: Vec<_> = travel
            .routes
            .par_iter()
            .map(|r| (r.source, r.destination))
            .collect();
        routes.par_sort_unstable();
        let mut requests: Vec<_> = travel
            .destinations
            .par_iter()
            .filter_map(|choice| {
                if !choice.enabled || busy.binary_search(&choice.person).is_ok() {
                    return None;
                }
                let person = self
                    .core
                    .people
                    .binary_search_by_key(&choice.person, |p| p.id)
                    .ok()
                    .map(|i| &self.core.people[i])?;
                let source = person.region?;
                (person.health > 0
                    && !person.controlled
                    && routes.binary_search(&(source, choice.destination)).is_ok())
                .then_some(travel::Departure {
                    person: choice.person,
                    destination: choice.destination,
                })
            })
            .collect();
        requests.par_sort_unstable_by_key(|choice| choice.person);
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        self.depart(&requests, occupied)
    }
    pub fn defer_work(&mut self, tick: u64, offset: u64, jobs: &[u64]) -> Result<(), String> {
        self.core.defer_work(tick, offset, jobs)
    }
    /// Admit actions selected by an enclosing owner, including work motivated
    /// by pressure rather than willingness. Physical eligibility still applies.
    pub fn start_chosen_work(
        &mut self,
        requests: &[(game_production_model::Start, u64)],
    ) -> Result<Vec<(u64, bool)>, String> {
        self.start_with_sites(requests)
    }
    /// A worker may independently stop work; inputs return at the boundary.
    pub fn abandon_work(&mut self, requests: &[(u64, u64)]) -> Result<(), String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let stopped: Vec<_> = requests.par_iter().map(|&(job, _)| (job, false)).collect();
        let next = self.stage_request_resolution(&stopped, tick, &self.core.people, false);
        let mut ended: Vec<_> = requests.par_iter().map(|&(job, _)| job).collect();
        ended.par_sort_unstable();
        let sites = self.stage_sites(&[], &ended, false);
        self.core.abandon_work(tick, offset, requests)?;
        self.paused_work_rates
            .retain(|row| ended.binary_search(&row.0).is_err());
        self.work_checked_revision = None;
        self.publish_request_resolution(next);
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        Ok(())
    }
    /// Physical transfers do not imply any social consent or ownership rule.
    pub fn transfer_goods(
        &mut self,
        actions: &[game_lots::Transaction],
    ) -> Result<game_lot_runtime::PhysicalOutcomes, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        self.core.transfer_goods(tick, offset, actions)
    }

    fn stage_tools(
        &self,
        items: &[game_lot_runtime::ProducedItem],
        next: Option<u64>,
        updates: &[(u64, Option<game_lots::Place>)],
    ) -> Result<Option<ToolWear>, String> {
        let Some(owner) = &self.tool_wear else {
            if !updates.is_empty() || !items.is_empty() {
                return Err("identified changes require an item owner".into());
            }
            return Ok(None);
        };
        if items.is_empty() && updates.is_empty() && next.is_none_or(|id| id == owner.next_item) {
            return Ok(None);
        }
        if updates.par_iter().any(|(id, place)| {
            place.is_none()
                && owner
                    .conditions
                    .binary_search_by_key(id, |row| row.item)
                    .is_err()
        }) {
            return Err("identified removal has no tool owner".into());
        }
        Ok(Some(owner.stage_identified_outputs(
            items,
            next.unwrap_or(owner.next_item),
            updates,
        )?))
    }
    /// Publish completed nonperishable work and its staged tool conditions.
    pub fn publish_early_work(
        &mut self,
    ) -> Result<Option<game_lot_runtime::EarlyWorkEvents>, String> {
        let goods: Vec<_> = self.tool_wear.as_ref().map_or_else(Vec::new, |owner| {
            owner.rules.iter().map(|rule| rule.good).collect()
        });
        let config = self
            .tool_wear
            .as_ref()
            .map(|owner| game_lot_runtime::IdentifiedProduction {
                goods: &goods,
                next_item: owner.next_item,
            });
        let Some(draft) = self.core.prepare_early_work(config)? else {
            return Ok(None);
        };
        let tools = self.stage_tools(
            draft.produced_items(),
            draft.next_produced_item(),
            draft.item_updates(),
        )?;
        let mut outcomes: Vec<_> = draft
            .events()
            .finished
            .par_iter()
            .map(|&job| (job, true))
            .collect();
        outcomes.par_extend(
            draft
                .events()
                .interrupted
                .par_iter()
                .map(|&job| (job, false)),
        );
        let requests =
            self.stage_request_resolution(&outcomes, draft.events().tick, &self.core.people, false);
        let ended: Vec<_> = outcomes.par_iter().map(|&(job, _)| job).collect();
        let sites = self.stage_sites(&draft.events().finished, &ended, false);
        let goals = self.stage_goals(
            &draft.events().finished,
            draft.events().tick,
            &self.core.people,
            false,
        )?;
        self.core.validate_early_work(&draft)?;
        let events = self.core.commit_early_work(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        self.publish_request_resolution(requests);
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        if let Some(goals) = goals {
            self.goals = Some(goals);
        }
        Ok(Some(events))
    }
    /// Stage and commit carried meals selected by individuals. Active workers
    /// and the host-provided occupied people are excluded.
    pub fn eat_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<Vec<game_lot_diet::Meal>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let mut busy = blocked.to_vec();
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
        let Some((meals, draft)) = self.core.prepare_autonomous_meals(&busy)? else {
            return Ok(Vec::new());
        };
        let tools = self.stage_tools(&[], None, draft.item_updates())?;
        self.core.validate_meals(&draft)?;
        self.core.commit_meals(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        Ok(meals)
    }
    /// Admit personally chosen work. Other owners' activity, claim, site and
    /// tool restrictions must be supplied in `blocked` by the enclosing host.
    pub fn start_autonomous_work(
        &mut self,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Vec<(u64, bool)>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        if let Some(result) = self.start_claim_aware_work(blocked, rate)? {
            return Ok(result);
        }
        if self.sites.is_some() || !self.tool_requirements.is_empty() {
            let Some(owner) = &self.autowork else {
                return Ok(Vec::new());
            };
            let tick = self.core.physical.tick();
            let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
            let effective = self.goals.as_ref().map(|goals| goals.policies(owner, tick));
            let Some(plan) = self.core.plan_autonomous_work(
                tick,
                offset,
                effective.as_ref().unwrap_or(owner),
                blocked,
                rate,
            )?
            else {
                return Ok(Vec::new());
            };
            let requests: Vec<_> = plan
                .requests
                .par_iter()
                .map(|start| (*start, rate))
                .collect();
            let outcomes = self.start_with_sites(&requests)?;
            self.autowork.as_mut().expect("checked work owner").next_id = plan.next_id;
            return Ok(outcomes);
        }
        let Some(owner) = &mut self.autowork else {
            return Ok(Vec::new());
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if let Some(goals) = &self.goals {
            let mut effective = goals.policies(owner, tick);
            let result =
                self.core
                    .start_autonomous_work(tick, offset, &mut effective, blocked, rate)?;
            owner.next_id = effective.next_id;
            Ok(result)
        } else {
            self.core
                .start_autonomous_work(tick, offset, owner, blocked, rate)
        }
    }
    /// Settle the completed quantum after staging tool conditions.
    pub fn settle(&mut self) -> Result<Events, String> {
        self.settle_with_occupied(&[])
    }
    /// Settle while excluding people occupied by owners outside this host.
    pub fn settle_with_occupied(&mut self, occupied: &[u64]) -> Result<Events, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let mut cultivation = self.plan_cultivation(occupied)?;
        let mut actions = Vec::new();
        let compact = game_economy_agriculture::append_planting(&mut cultivation, &mut actions);
        let irrigation_plan = self.plan_irrigation(occupied, &mut cultivation, &mut actions)?;
        let gathering =
            self.plan_gathering(occupied, cultivation.as_ref(), irrigation_plan.as_ref())?;
        let goods: Vec<_> = self.tool_wear.as_ref().map_or_else(Vec::new, |owner| {
            owner.rules.iter().map(|rule| rule.good).collect()
        });
        let config = self
            .tool_wear
            .as_ref()
            .map(|owner| game_lot_runtime::IdentifiedProduction {
                goods: &goods,
                next_item: owner.next_item,
            });
        let mut growth = game_economy_agriculture::physical_growth(cultivation.as_ref(), &compact);
        growth.gathered = gathering
            .as_ref()
            .map_or(&[], |(_, outputs)| outputs.as_slice());
        let (care, hunting) = if self.hunting.is_some() || self.animal_care.is_some() {
            let mut next_action = actions
                .par_iter()
                .map(|row| row.id)
                .chain(growth.harvest.par_iter().map(|row| row.id))
                .chain(growth.gathered.par_iter().map(|row| row.action))
                .max()
                .map_or(Ok(1), |id| {
                    id.checked_add(1).ok_or("animal action overflow")
                })?;
            let mut care = self.plan_animal_care(
                occupied,
                cultivation.as_ref(),
                irrigation_plan.as_ref(),
                gathering.as_ref().map(|(plan, _)| plan),
                next_action,
            )?;
            if let Some(plan) = &mut care {
                next_action = next_action
                    .checked_add(plan.actions.len() as u64)
                    .ok_or("animal action overflow")?;
                actions.par_extend(std::mem::take(&mut plan.actions).into_par_iter());
            }
            let hunting = self.plan_hunting(
                occupied,
                cultivation.as_ref(),
                irrigation_plan.as_ref(),
                gathering.as_ref().map(|(plan, _)| plan),
                care.as_ref(),
                next_action,
            )?;
            (care, hunting)
        } else {
            (None, None)
        };
        growth.animal_yield = hunting.as_ref().map_or(&[], |plan| plan.yields.as_slice());
        let mut draft = self
            .core
            .prepare_settlement_with_growth(&actions, growth, config)?;
        let fatigue = self.stage_fatigue(&mut draft.people.people)?;
        if self
            .environment
            .as_ref()
            .is_some_and(|owner| !owner.regions.is_empty())
        {
            let tick = draft.events().tick;
            let view = self.core.physical.view(tick)?;
            crate::environment::apply(
                self.environment.as_ref(),
                &view,
                self.core.travel.as_ref(),
                &mut draft.people.people,
                tick,
            );
        }
        let irrigation = self
            .irrigation
            .as_ref()
            .zip(irrigation_plan)
            .map(|(owner, plan)| owner.advance(plan, draft.events(), draft.events().tick));
        let (agriculture, claims) = self.stage_cultivation(cultivation, draft.events())?;
        let mut gathering = gathering
            .map(|(plan, _)| {
                plan.publish(
                    self.gathering.as_ref().expect("gathering owner"),
                    draft.events(),
                    self.environment
                        .as_ref()
                        .and_then(|owner| owner.season(draft.events().tick)),
                )
            })
            .transpose()?;
        let hunted = hunting
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.accepted(draft.events()));
        let feeding = care
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.accepted(draft.events()));
        let animal_feelings = self.stage_animal_feelings(hunting.as_ref(), &hunted, &feeding);
        let fauna = self.stage_fauna(&mut gathering, &hunted, &feeding, draft.events().tick)?;
        let mut tools = self.stage_tools(
            draft.produced_items(),
            draft.next_produced_item(),
            draft.item_updates(),
        )?;
        if tools.is_none() {
            tools = self
                .tool_wear
                .as_ref()
                .filter(|owner| !owner.events.is_empty())
                .cloned();
        }
        if let Some(tools) = &mut tools {
            tools.events.clear();
        }
        let mut outcomes: Vec<_> = draft
            .events()
            .finished
            .par_iter()
            .map(|&job| (job, true))
            .collect();
        outcomes.par_extend(
            draft
                .events()
                .interrupted
                .par_iter()
                .chain(draft.events().abandoned.par_iter())
                .chain(draft.events().suspended.par_iter())
                .map(|&job| (job, false)),
        );
        outcomes.par_sort_unstable_by_key(|&(job, success)| (job, !success));
        outcomes.dedup_by_key(|row| row.0);
        let requests = self.stage_request_resolution(
            &outcomes,
            draft.events().tick,
            &draft.people.people,
            true,
        );
        let goals = self.stage_goals(
            &draft.events().finished,
            draft.events().tick,
            &draft.people.people,
            true,
        )?;
        let mut ended = draft.events().finished.clone();
        ended.par_extend(draft.events().interrupted.par_iter().copied());
        ended.par_extend(draft.events().abandoned.par_iter().copied());
        let sites = self.stage_sites(&draft.events().finished, &ended, true);
        let agreements = self.agreements.as_ref().and_then(|owner| {
            (!owner.events.is_empty()
                || owner.contracts.par_iter().any(|contract| {
                    contract.due < draft.events().tick
                        && matches!(
                            contract.status,
                            game_agreements::Status::Active | game_agreements::Status::Proposed
                        )
                }))
            .then(|| owner.advance(draft.events().tick))
        });
        self.core.validate_settlement(&draft)?;
        let events = self.core.commit_settlement(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        self.publish_request_resolution(requests);
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        if let Some(goals) = goals {
            self.goals = Some(goals);
        }
        if let Some(agreements) = agreements {
            self.agreements = Some(agreements);
        }
        if let Some(owner) = agriculture {
            self.agriculture = Some(owner);
        }
        if let Some(owner) = irrigation {
            self.irrigation = Some(owner);
        }
        if let Some(owner) = gathering {
            self.gathering = Some(owner);
        }
        self.publish_fauna(fauna);
        if let Some(feelings) = animal_feelings {
            self.animal_feelings = Some(feelings);
        }
        if let Some((owner, opinions)) = claims {
            self.claims = Some(owner);
            self.opinions = opinions;
        }
        if let Some(fatigue) = fatigue {
            self.fatigue = Some(fatigue);
            self.activity = self
                .core
                .people
                .par_iter()
                .map(|person| game_economy_fatigue::TimedActivity {
                    id: person.id,
                    ..Default::default()
                })
                .collect();
        }
        self.paused_work_rates.retain(|(job, _)| {
            self.core
                .physical
                .work()
                .binary_search_by_key(job, |row| row.job.id)
                .ok()
                .is_some_and(|index| {
                    self.core.physical.work()[index].phase == game_lot_runtime::Phase::Working
                })
        });
        self.work_checked_revision = None;
        self.schedule = None;
        Ok(events)
    }
}

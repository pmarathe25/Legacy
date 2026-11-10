//! A saved host for the limited fractional-time core and its personal owners.
use super::*;
use game_autowork::Autowork;
use game_economy_work::tool_wear::ToolWear;
mod fatigue;
mod offers;
mod requests;
mod scheduler;
mod sites;
mod tools;
pub use offers::CoreSocialResults;
pub use requests::{CoreDeliveryResults, CoreRequestResults};
pub use scheduler::{CoreRun, CoreRunEvent, CoreSchedule, DecisionPhase};
pub use sites::{CoreRepairResults, CoreSites};
pub use tools::CoreToolRepairResults;

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
        } = options;
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
        if (tool_requirements.is_empty() && fatigue.is_none() && !paused_work_rates.is_empty())
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
        let mut draft = self.core.prepare_settlement(config)?;
        let fatigue = self.stage_fatigue(&mut draft.people.people)?;
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

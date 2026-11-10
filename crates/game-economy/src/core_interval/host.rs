//! A saved host for the limited fractional-time core and its personal owners.
use super::*;
use game_autowork::Autowork;
use game_economy_work::tool_wear::ToolWear;
mod requests;
mod scheduler;
pub use requests::CoreRequestResults;
pub use scheduler::{CoreRun, CoreRunEvent, CoreSchedule, DecisionPhase};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreHostSnapshot {
    pub core: CoreSnapshot,
    #[serde(default)]
    pub autowork: Option<Autowork>,
    #[serde(default)]
    pub tool_wear: Option<ToolWear>,
    #[serde(default)]
    pub opinions: Option<game_opinions::Opinions>,
    #[serde(default)]
    pub goals: Option<game_goals::Goals>,
    #[serde(default)]
    pub agreements: Option<game_agreements::Agreements>,
    #[serde(default)]
    pub schedule: Option<CoreSchedule>,
}

/// This host composes only the owners listed in `CoreHostSnapshot`.
pub struct CoreHost {
    core: CoreInterval,
    autowork: Option<Autowork>,
    tool_wear: Option<ToolWear>,
    opinions: Option<game_opinions::Opinions>,
    goals: Option<game_goals::Goals>,
    agreements: Option<game_agreements::Agreements>,
    schedule: Option<CoreSchedule>,
}
impl CoreHost {
    pub fn begin(snapshot: CoreHostSnapshot, rates: &[(u64, u64)]) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            schedule,
        } = snapshot;
        let core = CoreInterval::begin(core, rates)?;
        Self::compile(
            core, autowork, tool_wear, opinions, goals, agreements, schedule,
        )
    }
    pub fn resume(snapshot: CoreHostSnapshot) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            schedule,
        } = snapshot;
        let core = CoreInterval::resume(core)?;
        Self::compile(
            core, autowork, tool_wear, opinions, goals, agreements, schedule,
        )
    }
    fn compile(
        core: CoreInterval,
        autowork: Option<Autowork>,
        tool_wear: Option<ToolWear>,
        opinions: Option<game_opinions::Opinions>,
        goals: Option<game_goals::Goals>,
        agreements: Option<game_agreements::Agreements>,
        schedule: Option<CoreSchedule>,
    ) -> Result<Self, String> {
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
        if let Some(schedule) = &schedule {
            schedule.validate(&core)?;
        }
        Ok(Self {
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            schedule,
        })
    }
    pub fn snapshot(&self) -> CoreHostSnapshot {
        CoreHostSnapshot {
            core: self.core.snapshot(),
            autowork: self.autowork.clone(),
            tool_wear: self.tool_wear.clone(),
            opinions: self.opinions.clone(),
            goals: self.goals.clone(),
            agreements: self.agreements.clone(),
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
        self.core.progress(millis)
    }
    pub fn next_deadline(&self, hunger: Option<u32>) -> Result<CoreDeadline, String> {
        self.core.next_deadline(hunger)
    }
    pub fn publish_arrivals(&mut self) -> Result<Option<CoreArrivals>, String> {
        self.core.publish_arrivals()
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
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        self.core.start_work(tick, offset, requests)
    }
    /// A worker may independently stop work; inputs return at the boundary.
    pub fn abandon_work(&mut self, requests: &[(u64, u64)]) -> Result<(), String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let stopped: Vec<_> = requests.par_iter().map(|&(job, _)| (job, false)).collect();
        let next = self.stage_request_resolution(&stopped, tick, &self.core.people, false);
        self.core.abandon_work(tick, offset, requests)?;
        self.publish_request_resolution(next);
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
        let draft = self.core.prepare_settlement(config)?;
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
        if let Some(goals) = goals {
            self.goals = Some(goals);
        }
        if let Some(agreements) = agreements {
            self.agreements = Some(agreements);
        }
        self.schedule = None;
        Ok(events)
    }
}

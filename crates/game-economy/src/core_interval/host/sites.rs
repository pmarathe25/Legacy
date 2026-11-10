//! Physical site occupancy and maintenance for the limited core host.
use super::*;
use game_economy_building::maintenance::Maintenance;
use game_economy_work::sites::Facilities;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSites {
    pub facilities: Facilities,
    #[serde(default)]
    pub maintenance: Option<Maintenance>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreRepairResults {
    pub actors: Vec<u64>,
    pub events: Vec<game_economy_building::maintenance::Event>,
}
impl CoreSites {
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let recipes: HashSet<_> = core
            .physical
            .recipes()
            .par_iter()
            .map(|recipe| recipe.id)
            .collect();
        let goods: HashSet<_> = core
            .physical
            .goods()
            .par_iter()
            .map(|good| good.id)
            .collect();
        let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
        let view = core.physical.view(core.physical.tick())?;
        let regions: HashSet<_> = view
            .containers
            .par_iter()
            .filter_map(|row| match row.place {
                game_lots::Place::Stock { region, .. } => Some(region),
                _ => None,
            })
            .collect();
        self.facilities
            .validate(core.physical.work(), &recipes, &regions)?;
        if self.maintenance.as_ref().is_some_and(|owner| {
            !owner.valid(&people, &goods, &self.facilities, core.physical.tick())
        }) {
            return Err("invalid site maintenance".into());
        }
        Ok(())
    }
}
impl CoreHost {
    /// Let people choose maintenance; real carried materials must be consumed.
    pub fn repair_sites_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreRepairResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        let Some(owner) = self.sites.as_ref() else {
            return Ok(CoreRepairResults::default());
        };
        let Some(maintenance) = owner
            .maintenance
            .as_ref()
            .filter(|maintenance| !maintenance.policies.is_empty())
        else {
            return Ok(CoreRepairResults::default());
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before maintenance".into());
        }
        let mut busy = blocked.to_vec();
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
        let plan = game_economy_building::maintenance::Context {
            owner: maintenance,
            sites: &owner.facilities,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: None,
            busy: &busy,
            max_hunger: self.core.rules.max_work_hunger,
            manual: None,
            manual_destroy: None,
            seed: self.core.seed,
            tick,
        }
        .prepare(1)?;
        if plan.actions.is_empty() {
            return Ok(CoreRepairResults::default());
        }
        let actors = plan.actors.clone();
        let physical = self
            .core
            .physical
            .prepare_interval_consumption(&plan.actions, self.core.seed)?;
        let events = plan.publish_outcomes(tick, physical.transactions());
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        let writing = self.stage_written_page_removals(physical.item_updates());
        let mut next = owner.clone();
        next.maintenance
            .as_mut()
            .expect("checked maintenance")
            .record_repairs(events.clone());
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        self.sites = Some(next);
        Ok(CoreRepairResults { actors, events })
    }
    /// Reserve site slots provisionally, then retain only actual physical starts.
    pub(super) fn start_with_sites(
        &mut self,
        starts: &[(game_production_model::Start, u64)],
    ) -> Result<Vec<(u64, bool)>, String> {
        let skill_rates = self.skill_start_rates(starts)?;
        let starts = skill_rates.as_deref().unwrap_or(starts);
        if let Some(family) = &self.family {
            let underage = game_economy_work::underage(family);
            if starts
                .par_iter()
                .any(|(start, _)| underage.binary_search(&start.worker).is_ok())
            {
                return Err("an underage person cannot start work".into());
            }
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if self.sites.is_none() && self.tool_requirements.is_empty() {
            return self.core.start_work(tick, offset, starts);
        }
        let mut ids: Vec<_> = starts.par_iter().map(|(start, _)| start.id).collect();
        let mut workers: Vec<_> = starts.par_iter().map(|(start, _)| start.worker).collect();
        let recipes: std::collections::HashMap<_, _> = self
            .core
            .physical
            .recipes()
            .par_iter()
            .map(|recipe| (recipe.id, recipe))
            .collect();
        let active: HashSet<_> = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.phase == game_lot_runtime::Phase::Working)
            .map(|work| work.job.worker)
            .collect();
        ids.par_sort_unstable();
        workers.par_sort_unstable();
        if ids.par_windows(2).any(|rows| rows[0] == rows[1])
            || workers.par_windows(2).any(|rows| rows[0] == rows[1])
            || starts.par_iter().any(|(start, rate)| {
                *rate == 0
                    || start.id == 0
                    || start.worker == 0
                    || active.contains(&start.worker)
                    || start.region == 0
                    || start.units == 0
                    || recipes.get(&start.recipe).is_none_or(|recipe| {
                        [&recipe.inputs, &recipe.outputs].into_iter().any(|items| {
                            items
                                .iter()
                                .try_fold(0u64, |total, item| {
                                    item.quantity
                                        .checked_mul(u64::from(start.units))
                                        .and_then(|amount| total.checked_add(amount))
                                })
                                .is_none_or(|total| total == 0)
                        })
                    })
                    || self
                        .core
                        .physical
                        .work()
                        .binary_search_by_key(&start.id, |work| work.job.id)
                        .is_ok()
                    || self
                        .core
                        .people
                        .binary_search_by_key(&start.worker, |person| person.id)
                        .ok()
                        .is_none_or(|index| {
                            let person = &self.core.people[index];
                            person.health == 0
                                || person.hunger > self.core.rules.max_work_hunger
                                || person.region != Some(start.region)
                        })
            })
        {
            return Err("invalid site work starts".into());
        }
        let mut tools = if self.tool_requirements.is_empty() {
            Vec::new()
        } else {
            let view = self.core.physical.view(tick)?;
            starts
                .par_iter()
                .map(|(start, _)| {
                    let selected = if let Some(owner) = &self.tool_wear {
                        owner.select(
                            &self.tool_requirements,
                            &self.core.physical,
                            &view,
                            start.worker,
                            start.recipe,
                        )
                    } else {
                        game_economy_work::tools::available(
                            &self.tool_requirements,
                            &view,
                            start.worker,
                            start.recipe,
                        )
                        .then(Vec::new)
                    };
                    (start.id, start.worker, selected)
                })
                .collect::<Vec<_>>()
        };
        tools.par_sort_unstable_by_key(|row| row.0);
        let tool_permits = |id: u64| {
            self.tool_requirements.is_empty()
                || tools
                    .binary_search_by_key(&id, |row| row.0)
                    .ok()
                    .is_some_and(|index| tools[index].2.is_some())
        };
        let plain: Vec<_> = starts
            .par_iter()
            .filter(|(start, _)| tool_permits(start.id))
            .map(|row| row.0)
            .collect();
        let plan = self.sites.as_ref().map(|owner| {
            let broken = owner
                .maintenance
                .as_ref()
                .map_or_else(Vec::new, |maintenance| {
                    maintenance.broken(&owner.facilities)
                });
            let available = (!broken.is_empty()).then(|| {
                let mut sites = owner.facilities.clone();
                sites.sites = sites
                    .sites
                    .into_par_iter()
                    .filter(|site| broken.binary_search(&site.id).is_err())
                    .collect();
                sites
            });
            available
                .as_ref()
                .unwrap_or(&owner.facilities)
                .prepare(&plain, self.core.seed, tick)
        });
        let permitted = |start: &game_production_model::Start| {
            tool_permits(start.id)
                && self.sites.as_ref().is_none_or(|owner| {
                    owner
                        .facilities
                        .requirements
                        .binary_search_by_key(&start.recipe, |row| row.recipe)
                        .is_err()
                        || plan.as_ref().is_some_and(|plan| plan.permits(start.id))
                })
        };
        let mut denied: Vec<_> = starts
            .par_iter()
            .filter(|(start, _)| !permitted(start))
            .map(|(start, _)| (start.id, false))
            .collect();
        let allowed: Vec<_> = starts
            .par_iter()
            .copied()
            .filter(|(start, _)| permitted(start))
            .collect();
        let mut outcomes = self.core.start_work(tick, offset, &allowed)?;
        if let (Some(owner), Some(plan)) = (&mut self.sites, &plan) {
            owner.facilities.record_started(plan, &outcomes);
        }
        if let Some(owner) = &mut self.tool_wear {
            let selected: Vec<_> = tools
                .par_iter()
                .filter(|row| {
                    outcomes
                        .binary_search_by_key(&row.0, |outcome| outcome.0)
                        .ok()
                        .is_some_and(|index| outcomes[index].1)
                })
                .flat_map_iter(|row| row.2.iter().flatten().map(move |&item| (row.1, item)))
                .collect();
            owner.wear_items(&selected);
        }
        outcomes.append(&mut denied);
        outcomes.par_sort_unstable_by_key(|row| row.0);
        Ok(outcomes)
    }
    /// Prepare site wear and release against physical job outcomes.
    pub(super) fn stage_sites(
        &self,
        finished: &[u64],
        ended: &[u64],
        clear_events: bool,
    ) -> Option<CoreSites> {
        let owner = self.sites.as_ref()?;
        let mut ids = ended.to_vec();
        ids.par_sort_unstable();
        let resolving = !ids.is_empty()
            && owner
                .facilities
                .bindings
                .par_iter()
                .any(|row| ids.binary_search(&row.job).is_ok());
        let clearing = clear_events
            && owner
                .maintenance
                .as_ref()
                .is_some_and(|maintenance| !maintenance.events.is_empty());
        if !resolving && !clearing {
            return None;
        }
        let mut next = owner.clone();
        if let Some(maintenance) = &mut next.maintenance {
            maintenance.record_work(&owner.facilities, finished);
            if clear_events {
                maintenance.events.clear();
            }
        }
        next.facilities.release_jobs(&ids);
        Some(next)
    }
}

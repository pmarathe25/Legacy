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
        self.sites = Some(next);
        Ok(CoreRepairResults { actors, events })
    }
    /// Reserve site slots provisionally, then retain only actual physical starts.
    pub(super) fn start_with_sites(
        &mut self,
        starts: &[(game_production_model::Start, u64)],
    ) -> Result<Vec<(u64, bool)>, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let Some(owner) = &self.sites else {
            return self.core.start_work(tick, offset, starts);
        };
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
        let plain: Vec<_> = starts.par_iter().map(|row| row.0).collect();
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
        let plan =
            available
                .as_ref()
                .unwrap_or(&owner.facilities)
                .prepare(&plain, self.core.seed, tick);
        let mut denied: Vec<_> = starts
            .par_iter()
            .filter(|(start, _)| {
                owner
                    .facilities
                    .requirements
                    .binary_search_by_key(&start.recipe, |row| row.recipe)
                    .is_ok()
                    && !plan.permits(start.id)
            })
            .map(|(start, _)| (start.id, false))
            .collect();
        let allowed: Vec<_> = starts
            .par_iter()
            .copied()
            .filter(|(start, _)| {
                owner
                    .facilities
                    .requirements
                    .binary_search_by_key(&start.recipe, |row| row.recipe)
                    .is_err()
                    || plan.permits(start.id)
            })
            .collect();
        let mut outcomes = self.core.start_work(tick, offset, &allowed)?;
        self.sites
            .as_mut()
            .expect("checked site owner")
            .facilities
            .record_started(&plan, &outcomes);
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

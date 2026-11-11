//! Personal repair of physically identified tools.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreToolRepairResults {
    pub actors: Vec<u64>,
    pub events: Vec<game_economy_work::tool_wear::RepairEvent>,
}

impl CoreHost {
    /// Combine rest and tool blockers while retaining one desired work rate.
    pub(super) fn sync_work_rates(&mut self, blocked: &[u64]) -> Result<(), String> {
        if self.tool_requirements.is_empty()
            && self.fatigue.is_none()
            && self.health.is_none()
            && self.shared_work.as_ref().is_none_or(|owner| {
                owner
                    .sessions
                    .par_iter()
                    .all(|session| blocked.binary_search(&session.event.person).is_err())
            })
        {
            return Ok(());
        }
        let interval = self
            .core
            .physical
            .work_interval()
            .ok_or("no physical interval")?;
        let revision = interval.custody_revision;
        if self.work_checked_revision == Some(revision) && self.work_checked_blocked == blocked {
            return Ok(());
        }
        let mut rates: Vec<_> = interval
            .rates
            .par_iter()
            .map(|&(job, rate)| (job, rate, false))
            .collect();
        rates.par_extend(
            self.paused_work_rates
                .par_iter()
                .map(|&(job, rate)| (job, rate, true)),
        );
        rates.par_sort_unstable_by_key(|row| row.0);
        let view = if self.tool_requirements.is_empty() {
            None
        } else {
            Some(self.core.physical.view(self.core.physical.tick())?)
        };
        let resting = self
            .fatigue
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.fatigue.resting());
        let choices: Vec<_> = rates
            .par_iter()
            .filter_map(|&(id, rate, was_paused)| {
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&id, |row| row.job.id)
                    .ok()?;
                let work = &self.core.physical.work()[index];
                if work.phase != game_lot_runtime::Phase::Working {
                    return None;
                }
                if work.job.remaining == 0 {
                    return Some((id, rate, false));
                }
                let present = view.as_ref().is_none_or(|view| {
                    game_economy_work::tools::available(
                        &self.tool_requirements,
                        view,
                        work.job.worker,
                        work.job.recipe,
                    )
                });
                let rests = resting.binary_search(&work.job.worker).is_ok();
                let worker = self
                    .core
                    .people
                    .binary_search_by_key(&work.job.worker, |person| person.id)
                    .ok()
                    .map(|i| &self.core.people[i]);
                let able = worker.is_some_and(|person| {
                    person.health > 0
                        && person.hunger <= self.core.rules.max_work_hunger
                        && person.region == Some(work.job.region)
                        && blocked.binary_search(&person.id).is_err()
                });
                let dead = worker.is_none_or(|person| person.health == 0);
                Some((id, rate, !present || rests || dead || (was_paused && !able)))
            })
            .collect();
        if self.shared_work.as_ref().is_some_and(|owner| {
            owner.sessions.par_iter().any(|session| {
                choices
                    .binary_search_by_key(&session.event.job, |row| row.0)
                    .ok()
                    .is_none_or(|i| choices[i].2)
                    || blocked.binary_search(&session.event.person).is_ok()
                    || resting.binary_search(&session.event.person).is_ok()
                    || view.as_ref().is_some_and(|view| {
                        !game_economy_work::tools::available(
                            &self.tool_requirements,
                            view,
                            session.event.person,
                            session.event.recipe,
                        )
                    })
                    || self
                        .core
                        .people
                        .binary_search_by_key(&session.event.person, |person| person.id)
                        .ok()
                        .is_none_or(|i| {
                            let helper = &self.core.people[i];
                            helper.health == 0
                                || helper.hunger > self.core.rules.max_work_hunger
                                || self
                                    .core
                                    .physical
                                    .work()
                                    .binary_search_by_key(&session.event.job, |row| row.job.id)
                                    .ok()
                                    .is_none_or(|j| {
                                        helper.region
                                            != Some(self.core.physical.work()[j].job.region)
                                    })
                        })
            })
        }) {
            self.release_shared_helpers()?;
            return self.sync_work_rates(blocked);
        }
        let enabled: Vec<_> = choices
            .par_iter()
            .filter(|row| !row.2)
            .map(|row| (row.0, row.1))
            .collect();
        let paused: Vec<_> = choices
            .par_iter()
            .filter(|row| row.2)
            .map(|row| (row.0, row.1))
            .collect();
        self.core.physical.replace_interval_rates(&enabled)?;
        self.paused_work_rates = paused;
        self.work_checked_revision = Some(revision);
        self.work_checked_blocked = blocked.to_vec();
        Ok(())
    }
    /// Stage each person's repair and its actual consumed material together.
    pub fn repair_tools_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreToolRepairResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        let Some(owner) = self
            .tool_wear
            .as_ref()
            .filter(|owner| !owner.repairs.is_empty())
        else {
            return Ok(CoreToolRepairResults::default());
        };
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before tool repair".into());
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
        let repairs = owner.prepare_repairs(
            &self.core.physical,
            &self.core.people,
            &busy,
            &[],
            self.core.rules.max_work_hunger,
            1,
        )?;
        self.commit_tool_repairs(repairs)
    }

    /// Commit only requests already selected and revalidated by the caller.
    pub(super) fn commit_tool_repairs(
        &mut self,
        mut repairs: Vec<(u64, u64, game_lots::Transaction)>,
    ) -> Result<CoreToolRepairResults, String> {
        if repairs.is_empty() {
            return Ok(CoreToolRepairResults::default());
        }
        let tick = self.core.physical.tick();
        let owner = self
            .tool_wear
            .as_ref()
            .ok_or("tool repair requires an owner")?;
        repairs
            .par_iter_mut()
            .for_each(|(_, _, action)| action.movements[0].id = action.id);
        let mut actors: Vec<_> = repairs.par_iter().map(|row| row.0).collect();
        actors.par_sort_unstable();
        actors.dedup();
        let actions: Vec<_> = repairs.par_iter().map(|row| row.2.clone()).collect();
        let physical = self
            .core
            .physical
            .prepare_interval_consumption(&actions, self.core.seed)?;
        let mut outcomes = physical.transactions().to_vec();
        outcomes.par_sort_unstable_by_key(|outcome| outcome.id);
        let mut next = self
            .stage_tools(&[], None, physical.item_updates())?
            .unwrap_or_else(|| owner.clone());
        let writing = self.stage_written_page_removals(physical.item_updates());
        next.advance(&[], &repairs, &outcomes, physical.item_updates(), tick);
        let events = next.events.clone();
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        self.tool_wear = Some(next);
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        Ok(CoreToolRepairResults { actors, events })
    }
}

//! Personal repair of physically identified tools.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreToolRepairResults {
    pub actors: Vec<u64>,
    pub events: Vec<game_economy_work::tool_wear::RepairEvent>,
}

impl CoreHost {
    /// Stage each person's repair and its actual consumed material together.
    pub fn repair_tools_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreToolRepairResults, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self
            .tool_wear
            .as_ref()
            .filter(|owner| !owner.repairs.is_empty())
        else {
            return Ok(CoreToolRepairResults::default());
        };
        let tick = self.core.physical.tick();
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
        let mut repairs = owner.prepare_repairs(
            &self.core.physical,
            &self.core.people,
            &busy,
            &[],
            self.core.rules.max_work_hunger,
            1,
        )?;
        if repairs.is_empty() {
            return Ok(CoreToolRepairResults::default());
        }
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
        next.advance(&[], &repairs, &outcomes, physical.item_updates(), tick);
        let events = next.events.clone();
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        self.tool_wear = Some(next);
        Ok(CoreToolRepairResults { actors, events })
    }
}

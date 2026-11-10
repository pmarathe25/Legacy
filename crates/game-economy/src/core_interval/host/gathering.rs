//! Renewable gathering into each individual's physical custody.
use super::*;

impl CoreHost {
    pub(super) fn plan_gathering(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
    ) -> Result<Option<(crate::gathering::Plan, Vec<game_lot_runtime::Gathered>)>, String> {
        let Some(owner) = &self.gathering else {
            return Ok(None);
        };
        let farm_end = cultivation
            .and_then(|plan| plan.changes.par_iter().map(|row| row.1).max())
            .map_or(Ok(1), |id| {
                id.checked_add(1).ok_or("gathering action overflow")
            })?;
        let base = farm_end
            .checked_add(
                u64::try_from(irrigation.map_or(0, |plan| plan.len()))
                    .map_err(|_| "gathering action overflow")?,
            )
            .ok_or("gathering action overflow")?;
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        if let Some(plan) = cultivation {
            busy.par_extend(
                plan.actors
                    .par_iter()
                    .chain(plan.requesters.par_iter())
                    .copied(),
            );
        }
        if let Some(plan) = irrigation {
            busy.par_extend(plan.parallel_actors());
        }
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
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let plan = crate::gathering::Context {
            owner,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            busy: &busy,
            goods: self.core.physical.goods(),
            view: &view,
            tick_millis: self.core.physical.tick_millis(),
            seed: self.core.seed,
            tick,
            manual: None,
            personal_output: true,
        }
        .prepare(base)?;
        let outputs = plan.carried_outputs(owner);
        Ok(Some((plan, outputs)))
    }
}

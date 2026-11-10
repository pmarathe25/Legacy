//! Chosen physical feeding of nearby individual animals.
use super::*;

impl CoreHost {
    pub(super) fn plan_animal_care(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        gathering: Option<&crate::gathering::Plan>,
        base: u64,
    ) -> Result<Option<crate::animal_care::Plan>, String> {
        let Some(owner) = &self.animal_care else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
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
        if let Some(plan) = gathering {
            busy.par_extend(plan.actors.par_iter().copied());
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
        crate::animal_care::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            busy: &busy,
            blocked_animals: &[],
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            tick,
        }
        .prepare(base)
        .map(Some)
    }
}

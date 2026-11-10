//! Personal hunting choices and physical yield reservations.
use super::*;

impl CoreHost {
    pub(super) fn plan_hunting(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        gathering: Option<&crate::gathering::Plan>,
        base: u64,
    ) -> Result<Option<crate::hunting::Plan>, String> {
        let Some(owner) = &self.hunting else {
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
        crate::hunting::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            requests: &[],
            busy: &busy,
            feed_targets: &[],
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            tick,
            manual: None,
        }
        .prepare(base)
        .map(Some)
    }
    pub(super) fn stage_animal_feelings(
        &self,
        plan: Option<&crate::hunting::Plan>,
        hunted: &[u64],
    ) -> Option<crate::AnimalFeelings> {
        let owner = self.animal_feelings.as_ref()?;
        let plan = plan?;
        let events = owner.relevant(
            plan.yields
                .par_iter()
                .filter_map(|yield_| {
                    hunted.binary_search(&yield_.animal).is_ok().then_some(
                        crate::animal_feelings::Event {
                            person: yield_.hunter,
                            animal: yield_.animal,
                            kind: crate::animal_feelings::Experience::Hunt,
                        },
                    )
                })
                .collect(),
        );
        (!events.is_empty()).then(|| owner.advance(&events))
    }
}

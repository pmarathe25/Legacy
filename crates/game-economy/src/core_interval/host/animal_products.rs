//! Renewable animal products collected through individual choices and physical custody.
use super::*;

type PriorPlans<'a> = (
    Option<&'a game_economy_agriculture::Plan>,
    Option<&'a game_economy_agriculture::irrigation::Plan>,
    Option<&'a crate::gathering::Plan>,
    Option<&'a crate::animal_care::Plan>,
    Option<&'a game_animal_restraint::Plan>,
    Option<&'a crate::animal_handling::Plan>,
    Option<&'a crate::hunting::Plan>,
);

impl CoreHost {
    pub(super) fn plan_animal_products(
        &self,
        occupied: &[u64],
        (cultivation, irrigation, gathering, care, restraints, handling, hunting): PriorPlans<'_>,
        base: u64,
    ) -> Result<Option<crate::animal_products::Plan>, String> {
        let Some(owner) = &self.animal_products else {
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
        let mut animals = care.map_or_else(Vec::new, |plan| {
            busy.par_extend(plan.actors.par_iter().copied());
            plan.targets()
        });
        if let Some(plan) = restraints {
            busy.par_extend(plan.actors.par_iter().copied());
            animals.par_extend(plan.reserved.par_iter().copied());
        }
        if let Some(plan) = handling {
            busy.par_extend(plan.actors.par_iter().copied());
            animals.par_extend(plan.events.par_iter().map(|event| event.animal));
        }
        if let Some(plan) = hunting {
            busy.par_extend(plan.actors.par_iter().copied());
            animals.par_extend(plan.yields.par_iter().map(|yield_| yield_.animal));
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
        animals.par_sort_unstable();
        animals.dedup();
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        crate::animal_products::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            holds: self.animal_restraints.as_ref(),
            busy: &busy,
            blocked: &animals,
            max_hunger: self.core.rules.max_work_hunger,
            manual: None,
            seed: self.core.seed,
            tick,
        }
        .prepare(base)
        .map(Some)
    }
}

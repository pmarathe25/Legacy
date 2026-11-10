//! Personal capture, release, resistance, and defense of physical animal holds.
use super::*;

type PriorPlans<'a> = (
    Option<&'a game_economy_agriculture::Plan>,
    Option<&'a game_economy_agriculture::irrigation::Plan>,
    Option<&'a crate::gathering::Plan>,
    Option<&'a crate::animal_care::Plan>,
);

impl CoreHost {
    pub(super) fn plan_animal_restraints(
        &self,
        occupied: &[u64],
        (cultivation, irrigation, gathering, care): PriorPlans<'_>,
    ) -> Result<Option<game_animal_restraint::Plan>, String> {
        let Some(owner) = &self.animal_restraints else {
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
        if let Some(plan) = care {
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
        let blocked = care.map_or_else(Vec::new, |plan| plan.targets());
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let affinity = |person, animal| {
            self.animal_feelings
                .as_ref()
                .and_then(|feelings| feelings.affinity(person, animal))
                .unwrap_or(0)
        };
        game_animal_restraint::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            affinity: self
                .animal_feelings
                .as_ref()
                .map(|_| &affinity as &(dyn Fn(u64, u64) -> i32 + Sync)),
            busy: &busy,
            blocked: &blocked,
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            tick,
            manual: None,
        }
        .prepare()
        .map(Some)
    }
}

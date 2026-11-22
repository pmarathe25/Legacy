//! Individually chosen encounters with nearby animals.
use super::*;

impl CoreHost {
    pub(super) fn handle_animals_at_decision(
        &mut self,
        occupied: &[u64],
        animals_fed: &[u64],
    ) -> Result<crate::animal_handling::Plan, String> {
        let Some(owner) = self
            .animal_handling
            .as_ref()
            .filter(|owner| owner.elapsed_actions)
        else {
            return Ok(crate::animal_handling::Plan::default());
        };
        let busy = self.local_walk_exclusions(occupied);
        let away = self.block_away_from_stock(&busy);
        let busy = away.as_deref().unwrap_or(&busy);
        let held = self.block_held(busy);
        let busy = held.as_deref().unwrap_or(busy);
        let plan = crate::animal_handling::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            feelings: self.animal_feelings.as_ref(),
            busy,
            blocked: animals_fed,
            max_hunger: u32::MAX,
            seed: self.core.seed,
            draw_key: self.core.decision_time_key(),
            tick: self.core.physical.tick(),
            manual: None,
        }
        .prepare()?;
        if !plan.events.is_empty() {
            let feelings = self.stage_animal_feelings(None, &[], &[], Some(&plan), &[], None);
            let fauna = self.fauna.as_mut().ok_or("missing wildlife for handling")?;
            game_fauna::handling::apply(fauna, &plan.events);
            self.animal_handling
                .as_mut()
                .expect("handling owner")
                .events = plan.events.clone();
            if let Some(feelings) = feelings {
                self.animal_feelings = Some(feelings);
            }
        }
        Ok(plan)
    }

    pub(super) fn plan_animal_handling(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        gathering: Option<&crate::gathering::Plan>,
        care: Option<&crate::animal_care::Plan>,
        restraints: Option<&game_animal_restraint::Plan>,
    ) -> Result<Option<crate::animal_handling::Plan>, String> {
        let Some(owner) = self
            .animal_handling
            .as_ref()
            .filter(|owner| !owner.elapsed_actions)
        else {
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
        if let Some(plan) = restraints {
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
        let mut blocked = care.map_or_else(Vec::new, |plan| plan.targets());
        if let Some(plan) = restraints {
            blocked.par_extend(plan.reserved.par_iter().copied());
            blocked.par_sort_unstable();
            blocked.dedup();
        }
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        crate::animal_handling::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            feelings: self.animal_feelings.as_ref(),
            busy: &busy,
            blocked: &blocked,
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            draw_key: tick,
            tick,
            manual: None,
        }
        .prepare()
        .map(Some)
    }
}

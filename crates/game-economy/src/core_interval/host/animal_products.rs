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

#[derive(Default)]
pub(super) struct ProductRound {
    pub actors: Vec<u64>,
    pub events: Vec<crate::animal_products::Event>,
}

impl CoreHost {
    pub(super) fn collect_products_at_decision(
        &mut self,
        occupied: &[u64],
    ) -> Result<ProductRound, String> {
        let Some(owner) = self
            .animal_products
            .as_ref()
            .filter(|owner| owner.elapsed_actions)
        else {
            return Ok(ProductRound::default());
        };
        let busy = self.local_walk_exclusions(occupied);
        let away = self.block_away_from_stock(&busy);
        let busy = away.as_deref().unwrap_or(&busy);
        let held = self.block_held(busy);
        let busy = held.as_deref().unwrap_or(busy);
        let tick = self.core.physical.tick();
        let key = self.core.decision_time_key();
        let plan = crate::animal_products::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            holds: self.animal_restraints.as_ref(),
            busy,
            blocked: &[],
            max_hunger: u32::MAX,
            manual: None,
            seed: self.core.seed,
            draw_key: key,
            tick,
        }
        .prepare(1)?;
        let custody = (!plan.yields.is_empty())
            .then(|| {
                self.core
                    .physical
                    .prepare_interval_animal_products(&plan.yields, self.core.seed ^ key)
            })
            .transpose()?;
        let (events, handling) =
            plan.publish_receipts(custody.as_ref().map_or(&[], |draft| draft.transactions()));
        let mut actors: Vec<_> = events.par_iter().map(|row| row.person).collect();
        actors.par_sort_unstable();
        actors.dedup();
        let feelings = (!handling.is_empty())
            .then(|| self.stage_animal_feelings(None, &[], &[], None, &handling, None))
            .flatten();
        if let Some(custody) = custody
            && custody.transactions().iter().any(|row| row.accepted)
        {
            self.core.physical.validate_interval_consumption(&custody)?;
            self.core.physical.commit_interval_consumption(custody)?;
        }
        if let Some(feelings) = feelings {
            self.animal_feelings = Some(feelings);
        }
        self.animal_products
            .as_mut()
            .expect("checked animal products")
            .record_collection(events.clone());
        Ok(ProductRound { actors, events })
    }

    pub(super) fn plan_animal_products(
        &self,
        occupied: &[u64],
        (cultivation, irrigation, gathering, care, restraints, handling, hunting): PriorPlans<'_>,
        base: u64,
    ) -> Result<Option<crate::animal_products::Plan>, String> {
        let Some(owner) = self
            .animal_products
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
            draw_key: tick,
            tick,
        }
        .prepare(base)
        .map(Some)
    }
}

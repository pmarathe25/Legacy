//! Chosen physical feeding of nearby individual animals.
use super::*;

#[derive(Default)]
pub(super) struct FeedingRound {
    pub actors: Vec<u64>,
    pub targets: Vec<u64>,
    pub receipts: Vec<game_fauna::Feeding>,
}

impl CoreHost {
    pub(super) fn feed_at_decision(&mut self, occupied: &[u64]) -> Result<FeedingRound, String> {
        let Some(owner) = self
            .animal_care
            .as_ref()
            .filter(|owner| owner.elapsed_actions)
        else {
            return Ok(FeedingRound::default());
        };
        let busy = self.local_walk_exclusions(occupied);
        let away = self.block_away_from_stock(&busy);
        let busy = away.as_deref().unwrap_or(&busy);
        let held = self.block_held(busy);
        let busy = held.as_deref().unwrap_or(busy);
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let draw_key = self.core.decision_time_key();
        let plan = crate::animal_care::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            busy,
            blocked_animals: &[],
            max_hunger: u32::MAX,
            seed: self.core.seed,
            draw_key,
        }
        .prepare(1)?;
        if plan.actions.is_empty() {
            return Ok(FeedingRound::default());
        }
        let custody = self
            .core
            .physical
            .prepare_interval_consumption(&plan.actions, self.core.seed ^ draw_key)?;
        let receipts = plan.accepted(custody.transactions());
        let targets = plan.targets();
        if !receipts.is_empty() {
            let animals = self
                .fauna
                .as_ref()
                .ok_or("missing wildlife owner")?
                .prepare_encounters(&[], &receipts)
                .map_err(str::to_owned)?;
            let feelings = self.stage_animal_feelings(None, &[], &receipts, None, &[], None);
            self.core.physical.validate_interval_consumption(&custody)?;
            self.core.physical.commit_interval_consumption(custody)?;
            self.fauna
                .as_mut()
                .expect("prepared wildlife")
                .publish_encounter_receipts(animals);
            if let Some(feelings) = feelings {
                self.animal_feelings = Some(feelings);
            }
        }
        Ok(FeedingRound {
            actors: plan.actors,
            targets,
            receipts,
        })
    }

    pub(super) fn plan_animal_care(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        gathering: Option<&crate::gathering::Plan>,
        base: u64,
    ) -> Result<Option<crate::animal_care::Plan>, String> {
        let Some(owner) = self
            .animal_care
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
            draw_key: tick,
        }
        .prepare(base)
        .map(Some)
    }
}

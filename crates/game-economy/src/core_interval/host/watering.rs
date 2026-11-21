//! Publish chosen field watering and its custody use at the actual decision time.
use super::*;
use game_economy_agriculture::irrigation::{Choice, Context, Event};

impl CoreHost {
    pub(super) fn has_elapsed_watering(&self) -> bool {
        self.irrigation
            .as_ref()
            .is_some_and(|owner| owner.elapsed_actions && !owner.choices.is_empty())
    }

    pub(super) fn water_at_decision(&mut self, blocked: &[u64]) -> Result<Vec<Event>, String> {
        self.apply_elapsed_watering(blocked, None)
    }

    pub fn water_controlled(
        &mut self,
        person: u64,
        field: u64,
        quantity: u64,
    ) -> Result<Option<Event>, String> {
        let actor = self.controlled_person(person)?;
        let region = actor.region.ok_or("field is outside physical reach")?;
        if quantity == 0 || !self.regional_stock_reachable(person, region) {
            return Err("watering needs positive water and local reach".into());
        }
        let busy = self.choice_busy(&[]);
        if busy.binary_search(&person).is_ok()
            || self.assisting_people().binary_search(&person).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before watering".into());
        }
        let choice = Choice {
            person,
            field,
            quantity,
            willing: true,
            hunger_limit: None,
            pressured_by: None,
            fear_at_least: None,
            obligation_at_least: None,
            disregard_claims: true,
        };
        Ok(self
            .apply_elapsed_watering(&[], Some(&choice))?
            .into_iter()
            .next())
    }

    fn apply_elapsed_watering(
        &mut self,
        blocked: &[u64],
        manual: Option<&Choice>,
    ) -> Result<Vec<Event>, String> {
        let owner = self
            .irrigation
            .as_ref()
            .filter(|row| row.elapsed_actions)
            .ok_or("elapsed watering is disabled")?;
        let agriculture = self
            .agriculture
            .as_ref()
            .ok_or("watering requires cultivated fields")?;
        let busy = self.choice_busy(blocked);
        let away = self.block_away_from_stock(&busy);
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let round = self.core.decision_time_key();
        let seed = self.core.seed ^ round;
        let context = Context {
            agriculture,
            people: &self.core.people,
            busy: away.as_deref().unwrap_or(&busy),
            claims: claims.as_ref(),
            opinions: self.opinions.as_ref(),
            view: &view,
            max_hunger: self.core.rules.max_work_hunger,
            manual,
            seed: self.core.seed,
            tick: round,
        };
        let plan = if manual.is_some() {
            owner.prepare_manual(context)?
        } else {
            owner.prepare(context, 1)?
        };
        if plan.is_empty() {
            return Ok(Vec::new());
        }
        let actions: Vec<_> = plan.actions().cloned().collect();
        let (draft, outcomes) = self
            .core
            .physical
            .prepare_interval_cultivation(actions, seed)?;
        let uses: Vec<_> = plan.uses().collect();
        let (_, claims) = self.stage_cultivation(None, &outcomes, &uses)?;
        let next = owner.advance(plan, &outcomes, tick);
        let events = next.events.clone();
        if events.par_iter().any(|row| row.accepted) {
            self.core.physical.commit_interval_consumption(draft)?;
        }
        self.irrigation = Some(next);
        if let Some((claims, opinions)) = claims {
            self.claims = Some(claims);
            self.opinions = opinions;
        }
        Ok(events)
    }
}

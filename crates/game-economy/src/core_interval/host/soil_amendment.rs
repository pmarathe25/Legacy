//! Settle soil treatment at the time an individual chooses to apply material.
use super::*;
use game_economy_agriculture::amendment::{Choice, Context, Event};

impl CoreHost {
    pub(super) fn has_elapsed_amendment(&self) -> bool {
        self.agriculture
            .as_ref()
            .and_then(|farm| farm.soil.as_ref())
            .and_then(|soil| soil.amendment.as_ref())
            .is_some_and(|owner| owner.elapsed_actions && !owner.choices.is_empty())
    }

    pub(super) fn amend_at_decision(&mut self, blocked: &[u64]) -> Result<Vec<Event>, String> {
        self.amend_fields(blocked, None)
    }

    pub fn amend_controlled(&mut self, person: u64, field: u64) -> Result<bool, String> {
        let actor = self.controlled_person(person)?;
        let region = actor.region.ok_or("field is outside physical reach")?;
        if !self.regional_stock_reachable(person, region) {
            return Err("soil amendment requires local material".into());
        }
        if self.choice_busy(&[]).binary_search(&person).is_ok()
            || self.assisting_people().binary_search(&person).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before treating soil".into());
        }
        let choice = Choice {
            person,
            field,
            willing: true,
            hunger_limit: None,
            pressured_by: None,
            fear_at_least: None,
            obligation_at_least: None,
            disregard_claims: true,
        };
        Ok(self
            .amend_fields(&[], Some(&choice))?
            .first()
            .is_some_and(|event| event.accepted))
    }

    fn amend_fields(
        &mut self,
        blocked: &[u64],
        manual: Option<&Choice>,
    ) -> Result<Vec<Event>, String> {
        let agriculture = self
            .agriculture
            .as_ref()
            .ok_or("soil treatment requires fields")?;
        let soil = agriculture.soil.as_ref().ok_or("no field soil")?;
        let owner = soil
            .amendment
            .as_ref()
            .filter(|owner| owner.elapsed_actions)
            .ok_or("elapsed soil treatment is disabled")?;
        let busy = self.choice_busy(blocked);
        let away = self.block_away_from_stock(&busy);
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let round = self.core.decision_time_key();
        let context = Context {
            soil,
            agriculture,
            people: &self.core.people,
            busy: &busy,
            nonstock: away.as_deref().unwrap_or(&[]),
            occupied_fields: &[],
            claims: claims.as_ref(),
            opinions: self.opinions.as_ref(),
            view: &view,
            seed: self.core.seed,
            tick: round,
            manual,
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
            .prepare_interval_cultivation(actions, self.core.seed ^ round)?;
        let uses: Vec<_> = plan.uses().collect();
        let (_, claims) = self.stage_cultivation(None, &outcomes, &uses)?;
        let events = plan.outcomes(&outcomes, tick);
        if events.par_iter().any(|row| row.accepted) {
            self.core.physical.commit_interval_consumption(draft)?;
        }
        self.agriculture
            .as_mut()
            .and_then(|farm| farm.soil.as_mut())
            .expect("prepared field soil")
            .apply_amendments(events.clone());
        if let Some((claims, opinions)) = claims {
            self.claims = Some(claims);
            self.opinions = opinions;
        }
        Ok(events)
    }
}

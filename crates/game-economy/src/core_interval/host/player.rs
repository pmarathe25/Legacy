//! Limited observations for an explicitly controlled person.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSeenPerson {
    pub id: u64,
    pub health: u32,
}

/// A visible injury without the simulation's hidden recovery deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerWound {
    pub severity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerJob {
    pub id: u64,
    pub recipe: u32,
    pub remaining: u64,
}

/// Only the controlled person's own state and physically co-located people.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePlayerView {
    pub tick: u64,
    pub offset_millis: u64,
    pub person: Person,
    pub journey: Option<travel::Journey>,
    pub possessions: Vec<game_lots::Lot>,
    pub nearby: Vec<CoreSeenPerson>,
    pub wound: Option<CorePlayerWound>,
    pub job: Option<CorePlayerJob>,
}

impl CoreHost {
    /// Start the controlled person's chosen work using actual capability and supplies.
    pub fn start_controlled_work(
        &mut self,
        id: u64,
        recipe: u32,
        units: u32,
    ) -> Result<(u64, bool), String> {
        let person = self.controlled_person(id)?;
        let region = person
            .region
            .filter(|_| person.health > 0)
            .ok_or("the controlled person cannot start local work")?;
        if self
            .core
            .physical
            .work()
            .par_iter()
            .any(|work| work.phase == game_lot_runtime::Phase::Working && work.job.worker == id)
            || self
                .blocked_with_rest(&[])
                .as_ref()
                .is_some_and(|resting| resting.binary_search(&id).is_ok())
            || self.assisting_people().binary_search(&id).is_ok()
        {
            return Err("the controlled person is occupied".into());
        }
        let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
        let after_live = self
            .core
            .physical
            .work()
            .last()
            .map_or(Some(1), |row| row.job.id.checked_add(1))
            .ok_or("job id overflow")?;
        let job = schedule
            .next_player_job_id
            .max(after_live)
            .max(self.autowork.as_ref().map_or(1, |owner| owner.next_id));
        let next = job.checked_add(1).ok_or("job id overflow")?;
        if !self.work_capability_checks(&[(id, recipe)])[0] {
            return Ok((job, false));
        }
        let start = game_production_model::Start {
            id: job,
            worker: id,
            region,
            recipe,
            units,
        };
        let claims = if self
            .claims
            .as_ref()
            .is_some_and(|owner| !owner.claims.is_empty())
        {
            game_economy_claims::work(&[start], self.core.physical.recipes())?
        } else {
            Vec::new()
        };
        let tick = self.core.physical.tick();
        let rate = schedule.work_rate;
        let outcomes = self.start_with_sites_hunger_limit(&[(start, rate)], u32::MAX, true)?;
        self.schedule
            .as_mut()
            .expect("schedule checked")
            .next_player_job_id = next;
        if let Some(owner) = &mut self.autowork {
            owner.next_id = owner.next_id.max(next);
        }
        self.publish_claim_work(&claims, &outcomes, tick);
        Ok((job, outcomes.first().is_some_and(|outcome| outcome.1)))
    }

    /// Consume an explicitly chosen carried meal without a hunger preference gate.
    pub fn eat_controlled(&mut self, id: u64, good: u32) -> Result<bool, String> {
        let person = self.controlled_person(id)?;
        if person.health == 0
            || self
                .core
                .physical
                .work()
                .par_iter()
                .any(|work| work.phase == game_lot_runtime::Phase::Working && work.job.worker == id)
            || self
                .blocked_with_rest(&[])
                .as_ref()
                .is_some_and(|resting| resting.binary_search(&id).is_ok())
            || self.assisting_people().binary_search(&id).is_ok()
        {
            return Err("the controlled person cannot eat during their current activity".into());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let meal = game_lot_diet::Meal {
            action: 1,
            person: id,
            good,
        };
        let draft = self
            .core
            .prepare_meals(tick, offset, &[meal])?
            .ok_or("no meal was prepared")?;
        let accepted = draft
            .transactions()
            .first()
            .is_some_and(|outcome| outcome.accepted);
        let tools = self.stage_tools(&[], None, draft.item_updates())?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        self.core.validate_meals(&draft)?;
        self.core.commit_meals(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        Ok(accepted)
    }

    /// Validate control on each action; control does not grant access to other minds.
    pub fn controlled_person(&self, id: u64) -> Result<&Person, String> {
        self.core
            .people
            .binary_search_by_key(&id, |person| person.id)
            .ok()
            .map(|index| &self.core.people[index])
            .filter(|person| person.controlled)
            .ok_or_else(|| "person is not under player control".into())
    }

    /// Construct a filtered observation without cloning the world or remote state.
    pub fn player_view(&self, id: u64) -> Result<CorePlayerView, String> {
        let person = self.controlled_person(id)?;
        if person.health == 0 {
            return Err("the controlled person cannot observe while incapacitated".into());
        }
        let locations = travel::Colocation::new(self.core.travel.as_ref());
        let nearby = self
            .core
            .people
            .par_iter()
            .filter(|other| {
                other.id != person.id
                    && locations
                        .region((person.id, person.region), (other.id, other.region))
                        .is_some()
            })
            .map(|other| CoreSeenPerson {
                id: other.id,
                health: other.health,
            })
            .collect();
        let journey = self.core.travel.as_ref().and_then(|travel| {
            travel
                .journeys
                .binary_search_by_key(&id, |journey| journey.person)
                .ok()
                .map(|index| travel.journeys[index].clone())
        });
        let possessions = self
            .core
            .physical
            .custody(game_lots::Place::Person(id))
            .map_or_else(Vec::new, |holder| holder.lots.clone());
        let wound = self
            .personal_conflict
            .as_ref()
            .and_then(|owner| owner.wounds.as_ref())
            .and_then(|wounds| {
                wounds
                    .cases
                    .binary_search_by_key(&id, |case| case.person)
                    .ok()
                    .map(|index| CorePlayerWound {
                        severity: wounds.cases[index].severity,
                    })
            });
        let job = self
            .core
            .physical
            .work()
            .par_iter()
            .filter(|work| work.job.worker == id && work.phase == game_lot_runtime::Phase::Working)
            .min_by_key(|work| work.job.id)
            .map(|work| CorePlayerJob {
                id: work.job.id,
                recipe: work.job.recipe,
                remaining: work.job.remaining,
            });
        Ok(CorePlayerView {
            tick: self.core.physical.tick(),
            offset_millis: self.core.elapsed_millis().ok_or("no physical interval")?,
            person: person.clone(),
            journey,
            possessions,
            nearby,
            wound,
            job,
        })
    }
}

//! Personal food-seeking travel based on each person's remembered reports.
use super::*;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreFoodSearch {
    pub search: crate::FoodSearch,
    pub knowledge: game_economy_knowledge::Knowledge,
    #[serde(default)]
    pub claims: Option<game_economy_claims::Claims>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreFoodProvision {
    pub actors: Vec<u64>,
    pub outcomes: Vec<game_lots::TransactionOutcome>,
}

impl Clone for CoreFoodSearch {
    fn clone(&self) -> Self {
        Self {
            search: self.search.clone(),
            claims: self.claims.clone(),
            knowledge: game_economy_knowledge::Knowledge {
                max_records: self.knowledge.max_records,
                minds: self.knowledge.minds.par_iter().cloned().collect(),
                retention: self.knowledge.retention.par_iter().cloned().collect(),
            },
        }
    }
}

impl CoreHost {
    /// People personally seek a portion from their local regional stock.
    pub fn acquire_food_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<CoreFoodProvision, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        if self.food_search.is_none() {
            return Ok(CoreFoodProvision::default());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before food acquisition".into());
        }
        let extra = self.blocked_with_rest(occupied);
        let occupied = extra.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
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
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let claims = self
            .food_search
            .as_ref()
            .and_then(|owner| owner.claims.as_ref());
        let index = claims.map(game_economy_claims::Claims::index);
        let plan = game_economy_person::provision::prepare(
            &self.core.people,
            &self.core.rules,
            self.core.physical.goods(),
            (&view, &[], None),
            &busy,
            1,
            |person, region, good| {
                index
                    .as_ref()
                    .map_or(Some(0), |index| index.food_access(person, region, good))
            },
        )?;
        if plan.actions.is_empty() {
            return Ok(CoreFoodProvision::default());
        }
        let draft = self
            .core
            .physical
            .prepare_interval_transfer(&plan.actions, self.core.seed)?;
        let mut staged_claims = claims.cloned();
        let mut staged_opinions = claims.is_some().then(|| self.opinions.clone());
        if let Some(claims) = &mut staged_claims {
            let uses = game_economy_claims::pickups(&plan);
            let old_events: Vec<_> = std::mem::take(&mut claims.events)
                .into_par_iter()
                .filter(|event| event.tick == tick)
                .collect();
            let events = game_lot_runtime::Events {
                tick,
                physical_actions: draft.transactions().par_iter().cloned().collect(),
                started: vec![],
                start_denied: vec![],
                abandoned: vec![],
                suspended: vec![],
                adopted: vec![],
                finished: vec![],
                delivered: vec![],
                output_lost: vec![],
                interrupted: vec![],
                returned: vec![],
                empty_returns: vec![],
                spoiled: vec![],
                physical_movements: Default::default(),
            };
            game_economy_claims::publish(
                claims,
                &uses,
                &events,
                &self.core.people,
                staged_opinions.as_mut().expect("claims stage opinions"),
                None,
            );
            claims.events.par_extend(old_events);
            claims
                .events
                .par_sort_unstable_by_key(|e| (e.tick, e.claimant, e.person, e.region, e.good));
        }
        let outcomes = self.core.physical.commit_interval_consumption(draft)?;
        if let Some(owner) = &mut self.food_search {
            owner.claims = staged_claims;
        }
        if let Some(opinions) = staged_opinions {
            self.opinions = opinions;
        }
        Ok(CoreFoodProvision {
            actors: plan.actors,
            outcomes: outcomes.transactions,
        })
    }

    /// Stage personal search decisions and route observations together.
    pub fn seek_food_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = &self.food_search else {
            return Ok(Vec::new());
        };
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before food search".into());
        }
        let tick = self.core.physical.tick();
        let travel = self
            .core
            .travel
            .as_ref()
            .ok_or("food search needs travel")?;
        let extra = self.blocked_with_rest(occupied);
        let occupied = extra.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
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
        let view = self.core.physical.view(tick)?;
        let knowledge = game_economy_knowledge::advance(
            Some(&owner.knowledge),
            &self.core.people,
            self.core.physical.goods(),
            &view,
            None,
            None,
            game_economy_knowledge::ObservationAccess {
                now: tick,
                not_at_stock: &busy,
            },
        )?
        .ok_or("food search needs knowledge")?;
        let mut next_owner = CoreFoodSearch {
            search: owner.search.clone(),
            knowledge,
            claims: owner.claims.clone(),
        };
        let claims = next_owner
            .claims
            .as_ref()
            .map(game_economy_claims::Claims::index);
        let mut plan = crate::food_search::Context {
            owner: &next_owner.search,
            knowledge: &next_owner.knowledge,
            travel,
            people: &self.core.people,
            goods: self.core.physical.goods(),
            view: &view,
            claims: claims.as_ref(),
            busy: &busy,
            now: tick,
        }
        .prepare();
        plan.departures.par_sort_unstable_by_key(|row| row.person);
        next_owner.search.publish(
            &plan.departures,
            &plan.targets,
            &plan.abandoned,
            tick,
            travel.personal_routing,
        )?;
        if plan.departures.is_empty() {
            self.food_search = Some(next_owner);
            return Ok(Vec::new());
        }
        let generation = self
            .core
            .generation
            .checked_add(1)
            .ok_or("core revision overflow")?;
        let (next_travel, people, outcomes) =
            travel.attempt(&self.core.people, &plan.departures, &busy, tick)?;
        if !plan.memory_uses.is_empty() {
            let mut departed: Vec<_> = outcomes
                .par_iter()
                .filter(|row| row.departed)
                .map(|row| row.person)
                .collect();
            departed.par_sort_unstable();
            let uses: Vec<_> = plan
                .memory_uses
                .into_par_iter()
                .filter(|row| departed.binary_search(&row.0).is_ok())
                .collect();
            next_owner
                .knowledge
                .reinforce_uses(&uses, tick)
                .map_err(str::to_owned)?;
        }
        self.food_search = Some(next_owner);
        self.core.travel = Some(next_travel);
        self.core.people = people;
        self.core.generation = generation;
        self.work_checked_revision = None;
        Ok(outcomes)
    }
}

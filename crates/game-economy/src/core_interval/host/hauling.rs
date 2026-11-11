//! Independent choices to carry goods between regions.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreHaulingResults {
    pub actors: Vec<u64>,
    pub transfers: Vec<game_lots::TransactionOutcome>,
    pub departures: Vec<travel::DepartureOutcome>,
}

impl CoreHost {
    pub fn haul_autonomously(&mut self, blocked: &[u64]) -> Result<CoreHaulingResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.hauling.as_ref() else {
            return Ok(CoreHaulingResults::default());
        };
        if owner.policies.is_empty() {
            return Ok(CoreHaulingResults::default());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before hauling".into());
        }
        let travel = self
            .core
            .travel
            .as_ref()
            .ok_or("hauling needs travel routes")?;
        let rest = self.blocked_with_rest(blocked);
        let mut busy = rest.as_deref().unwrap_or(blocked).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let view = self.core.physical.view(tick)?;
        let claims_index = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = crate::hauling::Context {
            owner,
            travel,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: claims_index.as_ref(),
            view: &view,
            busy: &busy,
            max_hunger: self.core.rules.max_work_hunger,
            tick,
            seed: self.core.seed,
        }
        .prepare(1)?;
        if plan.actors.is_empty() {
            return Ok(CoreHaulingResults::default());
        }
        let movement = if plan.departures.is_empty() {
            None
        } else {
            let generation = self
                .core
                .generation
                .checked_add(1)
                .ok_or("core revision overflow")?;
            let (travel, people, outcomes) =
                travel.attempt(&self.core.people, &plan.departures, &busy, tick)?;
            Some((generation, travel, people, outcomes))
        };
        let physical = if plan.actions.is_empty() {
            None
        } else {
            Some(
                self.core
                    .physical
                    .prepare_interval_transfer(&plan.actions, self.core.seed)?,
            )
        };
        let mut claims = (!plan.uses.is_empty())
            .then(|| self.claims.clone())
            .flatten();
        let mut opinions = claims.as_ref().map(|_| self.opinions.clone());
        if let (Some(owner), Some(physical)) = (&mut claims, &physical) {
            let old: Vec<_> = std::mem::take(&mut owner.events)
                .into_par_iter()
                .filter(|event| event.tick == tick)
                .collect();
            let events = game_lot_runtime::Events {
                tick,
                physical_actions: physical.transactions().par_iter().cloned().collect(),
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
                owner,
                &plan.uses,
                &events,
                &self.core.people,
                opinions.as_mut().expect("claims stage opinions"),
                None,
            );
            owner.events.par_extend(old);
            owner
                .events
                .par_sort_unstable_by_key(|e| (e.tick, e.claimant, e.person, e.region, e.good));
        }
        let mut next = (!plan.empty_returns.is_empty()).then(|| owner.clone());
        if let Some(owner) = &mut next {
            owner.policies.par_iter_mut().for_each(|policy| {
                if let Ok(index) = plan
                    .empty_returns
                    .binary_search_by_key(&policy.person, |row| row.0)
                {
                    policy.retry_at = plan.empty_returns[index].1;
                }
            });
        }
        let tools = physical
            .as_ref()
            .map(|draft| self.stage_tools(&[], None, draft.item_updates()))
            .transpose()?
            .flatten();
        let transfers = if let Some(physical) = physical {
            self.core
                .physical
                .validate_interval_consumption(&physical)?;
            self.core
                .physical
                .commit_interval_consumption(physical)?
                .transactions
        } else {
            vec![]
        };
        let departures = if let Some((generation, travel, people, outcomes)) = movement {
            self.core.generation = generation;
            self.core.travel = Some(travel);
            self.core.people = people;
            outcomes
        } else {
            vec![]
        };
        if let Some(owner) = claims {
            self.claims = Some(owner);
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(owner) = next {
            self.hauling = Some(owner);
        }
        self.work_checked_revision = None;
        Ok(CoreHaulingResults {
            actors: plan.actors,
            transfers,
            departures,
        })
    }
}

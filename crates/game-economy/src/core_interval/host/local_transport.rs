//! Personal carriage between regional stock and local stores.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreLocalTransportResults {
    pub actors: Vec<u64>,
    pub events: Vec<game_economy_local_transport::Event>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NearbyStore {
    pub id: u64,
    pub name: String,
    pub reachable: bool,
}

impl CoreHost {
    /// Each person chooses a pickup or delivery based on their own policy and actual custody.
    pub fn carry_locally_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreLocalTransportResults, String> {
        self.carry_locally_with_hunger_limit(blocked, self.core.rules.max_work_hunger)
    }

    pub(super) fn carry_locally_with_hunger_limit(
        &mut self,
        blocked: &[u64],
        max_hunger: u32,
    ) -> Result<CoreLocalTransportResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.local_transport.as_ref() else {
            return Ok(CoreLocalTransportResults::default());
        };
        if !owner.has_choices() {
            return Ok(CoreLocalTransportResults::default());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before local carrying".into());
        }
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
        let held = self.block_held(&busy);
        let busy = held.as_deref().unwrap_or(&busy);
        let view = self.core.physical.view(tick)?;
        let claims_index = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let context = game_economy_local_transport::Context {
            now: self.core.simulated_millis(),
            owner,
            people: &self.core.people,
            view: &view,
            opinions: self.opinions.as_ref(),
            claims: claims_index.as_ref(),
            busy,
            max_hunger,
            manual: None,
            walking: owner.walking.as_ref(),
            storage_celsius: &|place| self.core.physical.storage_celsius(place),
        };
        let needs = self.goal_fetch_needs(owner);
        let plan = if needs.is_empty() {
            context.prepare(1)?
        } else {
            context.prepare_with_goals(1, &self.goal_carry_context(owner, &view, busy, &needs))?
        };
        self.commit_local_carries(plan)
    }

    pub fn controlled_local_stores(&self, person: u64) -> Result<Vec<NearbyStore>, String> {
        use game_economy_local_transport::Endpoint;
        let person = self.controlled_person(person)?;
        let Some(owner) = self.local_transport.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(owner.stores.par_iter().filter_map(|store| {
            (person.region == Some(store.region)).then(|| NearbyStore {
                id: store.id,
                name: store.name.clone(),
                reachable: owner.walking.as_ref().is_none_or(|walking| {
                    walking.present(person.id, store.region, Endpoint::Store { id: store.id })
                }),
            })
        }).collect())
    }

    pub fn carry_at_controlled_store(
        &mut self,
        person: u64,
        store: u64,
        good: u32,
        quantity: u64,
        deposit: bool,
    ) -> Result<bool, String> {
        self.controlled_person(person)?;
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
            || self.blocked_with_rest(&[]).is_some_and(|rows| rows.binary_search(&person).is_ok())
            || self.block_held(&[]).is_some_and(|rows| rows.binary_search(&person).is_ok())
            || self.assisting_people().binary_search(&person).is_ok()
            || self.core.physical.work().par_iter().any(|work| {
                work.job.worker == person && work.phase == game_lot_runtime::Phase::Working
            })
        {
            return Err("the controlled person is unavailable for carrying".into());
        }
        let owner = self.local_transport.as_ref().ok_or("local stores are unavailable")?;
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let plan = game_economy_local_transport::Context {
            now: self.core.simulated_millis(),
            owner,
            people: &self.core.people,
            view: &view,
            opinions: None,
            claims: None,
            busy: &[],
            max_hunger: u32::MAX,
            manual: Some(game_economy_local_transport::Manual { person, store, good, quantity, deposit }),
            walking: owner.walking.as_ref(),
            storage_celsius: &|place| self.core.physical.storage_celsius(place),
        }.prepare(1)?;
        Ok(self.commit_local_carries(plan)?.events.iter().any(|row| row.moved))
    }

    fn commit_local_carries(
        &mut self,
        plan: game_economy_local_transport::Plan,
    ) -> Result<CoreLocalTransportResults, String> {
        if plan.actions.is_empty() {
            return Ok(CoreLocalTransportResults::default());
        }
        let tick = self.core.physical.tick();
        let owner = self.local_transport.as_ref().expect("staged local carrying owner");
        let physical = self
            .core
            .physical
            .prepare_interval_transfer(&plan.actions, self.core.seed)?;
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
        let mut claims = self.claims.clone();
        let mut opinions = claims.as_ref().map(|_| self.opinions.clone());
        if let Some(owner) = &mut claims {
            let old: Vec<_> = std::mem::take(&mut owner.events)
                .into_par_iter()
                .filter(|event| event.tick == tick)
                .collect();
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
        let actors: Vec<_> = plan.actors.par_iter().copied().collect();
        let published = plan.publish(tick, &events);
        let mut next = owner.clone();
        next.events = owner
            .events
            .par_iter()
            .filter(|event| event.tick == tick)
            .cloned()
            .chain(published.par_iter().cloned())
            .collect();
        next.events
            .par_sort_unstable_by_key(|event| (event.tick, event.person));
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        self.claims = claims;
        self.local_transport = Some(next);
        self.work_checked_revision = None;
        Ok(CoreLocalTransportResults {
            actors,
            events: published,
        })
    }
}

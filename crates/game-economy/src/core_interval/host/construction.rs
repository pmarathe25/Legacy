//! Individually chosen site construction from physical kits.
use super::*;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreConstructionResults {
    pub actors: Vec<u64>,
    pub events: Vec<game_economy_building::Event>,
}

impl CoreHost {
    pub fn build_sites_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreConstructionResults, String> {
        self.build_sites_with_hunger_limit(blocked, self.core.rules.max_work_hunger)
    }

    pub(super) fn build_sites_with_hunger_limit(
        &mut self,
        blocked: &[u64],
        max_hunger: u32,
    ) -> Result<CoreConstructionResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.construction.as_ref() else {
            return Ok(CoreConstructionResults::default());
        };
        if owner.policies.is_empty() {
            return Ok(CoreConstructionResults::default());
        }
        let sites = self
            .sites
            .as_ref()
            .ok_or("construction requires work sites")?;
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before construction".into());
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
        let view = self.core.physical.view(tick)?;
        let claims_index = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = game_economy_building::Context {
            owner,
            sites: &sites.facilities,
            people: &self.core.people,
            view: &view,
            opinions: self.opinions.as_ref(),
            claims: claims_index.as_ref(),
            busy: &busy,
            max_hunger,
            tick,
            manual: None,
            goal_needs: &[],
            unwilling: &[],
        }
        .prepare(1)?;
        if plan.actions.is_empty() {
            return Ok(CoreConstructionResults::default());
        }
        let physical = self.core.physical.prepare_interval_construction(
            &plan.actions,
            &plan.custody,
            self.core.seed,
        )?;
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
        let mut next = owner.clone();
        let old: Vec<_> = next
            .events
            .par_iter()
            .filter(|event| event.tick == tick)
            .cloned()
            .collect();
        let mut sites = sites.clone();
        plan.publish(&mut next, &mut sites.facilities, &events);
        if let Some(maintenance) = &mut sites.maintenance {
            maintenance.enroll(&sites.facilities);
        }
        let published: Vec<_> = next.events.par_iter().cloned().collect();
        next.events.par_extend(old);
        next.events
            .par_sort_unstable_by_key(|event| (event.tick, event.person));
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        let writing = self.stage_written_page_removals(physical.item_updates());
        self.core
            .physical
            .validate_interval_consumption(&physical)?;
        self.core.physical.commit_interval_consumption(physical)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        if let Some(opinions) = opinions {
            self.opinions = opinions;
        }
        self.claims = claims;
        self.sites = Some(sites);
        self.construction = Some(next);
        self.work_checked_revision = None;
        Ok(CoreConstructionResults {
            actors,
            events: published,
        })
    }
}

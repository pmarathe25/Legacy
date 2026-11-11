//! Individual claim choices for autonomous production work.
use super::*;

impl CoreHost {
    /// Filter an individual's work policy by their chosen regard for claims.
    pub(super) fn claim_aware_work_policies(&self, effective: &Autowork) -> Option<Autowork> {
        let claims = self
            .claims
            .as_ref()
            .filter(|owner| !owner.claims.is_empty())?;
        let index = claims.index();
        let recipes: std::collections::HashMap<_, _> = self
            .core
            .physical
            .recipes()
            .par_iter()
            .map(|recipe| (recipe.id, recipe))
            .collect();
        Some(Autowork {
            next_id: effective.next_id,
            requests: None,
            policies: effective
                .policies
                .par_iter()
                .filter(|policy| {
                    if policy.disregard_claims {
                        return true;
                    }
                    let region = self
                        .core
                        .people
                        .binary_search_by_key(&policy.person, |p| p.id)
                        .ok()
                        .and_then(|i| self.core.people[i].region);
                    region
                        .zip(recipes.get(&policy.recipe))
                        .is_some_and(|(region, recipe)| {
                            index.allows_inputs(policy.person, region, recipe)
                        })
                })
                .cloned()
                .collect(),
        })
    }

    pub(super) fn start_claim_aware_work(
        &mut self,
        blocked: &[u64],
        rate: u64,
        effective: Option<&Autowork>,
    ) -> Result<Option<Vec<(u64, bool)>>, String> {
        let Some(_claims) = self
            .claims
            .as_ref()
            .filter(|owner| !owner.claims.is_empty())
        else {
            return Ok(None);
        };
        let Some(owner) = &self.autowork else {
            return Ok(Some(Vec::new()));
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let effective = effective.unwrap_or(owner);
        let filtered = self
            .claim_aware_work_policies(effective)
            .expect("claims checked");
        let Some(plan) = self
            .core
            .plan_autonomous_work(tick, offset, &filtered, blocked, rate)?
        else {
            return Ok(Some(Vec::new()));
        };
        let uses = game_economy_claims::work(&plan.requests, self.core.physical.recipes())?;
        let requests: Vec<_> = plan
            .requests
            .par_iter()
            .map(|start| (*start, rate))
            .collect();
        let outcomes = self.start_with_sites(&requests)?;
        self.autowork.as_mut().expect("checked work owner").next_id = plan.next_id;
        self.publish_claim_work(&uses, &outcomes, tick);
        Ok(Some(outcomes))
    }

    pub(super) fn publish_claim_work(
        &mut self,
        uses: &[game_economy_claims::Use],
        outcomes: &[(u64, bool)],
        tick: u64,
    ) {
        let Some(mut next_claims) = self.claims.clone().filter(|owner| !owner.claims.is_empty())
        else {
            return;
        };
        let mut opinions = self.opinions.clone();
        let old: Vec<_> = std::mem::take(&mut next_claims.events)
            .into_par_iter()
            .filter(|event| event.tick == tick)
            .collect();
        let mut started: Vec<_> = outcomes
            .par_iter()
            .filter(|row| row.1)
            .map(|row| row.0)
            .collect();
        started.par_sort_unstable();
        let events = game_lot_runtime::Events {
            tick,
            started,
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
            physical_actions: vec![],
            physical_movements: Default::default(),
        };
        game_economy_claims::publish(
            &mut next_claims,
            uses,
            &events,
            &self.core.people,
            &mut opinions,
            None,
        );
        next_claims.events.par_extend(old);
        next_claims
            .events
            .par_sort_unstable_by_key(|e| (e.tick, e.claimant, e.person, e.region, e.good));
        self.claims = Some(next_claims);
        self.opinions = opinions;
    }
}

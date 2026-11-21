//! Renewable gathering into each individual's physical custody.
use super::*;

pub(super) struct CoreGathered {
    pub actors: Vec<u64>,
    pub choices: Vec<CoreChoiceEvent>,
    pub receipts: Vec<crate::gathering::Harvested>,
}

impl CoreHost {
    pub(super) fn gathers_at_decisions(&self) -> bool {
        self.gathering
            .as_ref()
            .is_some_and(|owner| owner.elapsed_actions)
    }

    pub(super) fn gather_at_decision(
        &mut self,
        blocked: &[u64],
        intents: &[CoreChoiceIntent],
    ) -> Result<CoreGathered, String> {
        let owner = self
            .gathering
            .as_ref()
            .filter(|owner| owner.elapsed_actions)
            .ok_or("elapsed gathering is disabled")?;
        let tick = self.core.physical.tick();
        let offset_millis = self.core.elapsed_millis().ok_or("no physical interval")?;
        let profiled: Vec<_> = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_ref())
            .map_or_else(Vec::new, |profiles| {
                profiles.par_iter().map(|profile| profile.id).collect()
            });
        let nodes: std::collections::HashMap<_, _> =
            owner.nodes.par_iter().map(|node| (node.id, node)).collect();
        let selected: Vec<_> = intents
            .par_iter()
            .filter(|row| row.key.domain == personal_choice::GATHERING)
            .filter_map(|row| {
                let node = nodes.get(&row.key.action)?;
                Some(crate::gathering::Opportunity {
                    person: row.actor,
                    node: node.id,
                    good: node.good,
                    quantity: node.yield_per_action,
                })
            })
            .collect();
        let busy = self.choice_busy(blocked);
        let unreachable = self.block_away_from_stock(&busy);
        let view = self.core.physical.view(tick)?;
        let round = self.core.decision_time_key();
        let plan = crate::gathering::Context {
            owner,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            busy: unreachable.as_deref().unwrap_or(&busy),
            view: &view,
            tick_millis: 0,
            seed: self.core.seed,
            tick: round,
            manual: None,
        }
        .prepare_selected(1, &profiled, &selected)?;
        let outputs = plan.carried_outputs(owner);
        let physical = (!outputs.is_empty())
            .then(|| {
                self.core
                    .physical
                    .prepare_interval_gathering(&outputs, self.core.seed ^ round)
            })
            .transpose()?;
        let transactions = physical
            .as_ref()
            .map_or(&[][..], |draft| draft.transactions());
        let outcomes = plan.outcomes(transactions);
        let receipts = plan.receipts(&outputs, &outcomes);
        let choices = intents
            .par_iter()
            .filter(|row| row.key.domain == personal_choice::GATHERING)
            .map(|row| CoreChoiceEvent {
                tick,
                offset_millis,
                actor: row.actor,
                key: Some(row.key),
                outcome: outcomes
                    .binary_search_by_key(&row.actor, |entry| entry.0)
                    .ok()
                    .map_or(CoreChoiceOutcome::Stale, |index| {
                        if outcomes[index].1 {
                            CoreChoiceOutcome::Accepted
                        } else {
                            CoreChoiceOutcome::Denied
                        }
                    }),
            })
            .collect();
        let mut actors = plan.actors.clone();
        actors.par_extend(selected.par_iter().map(|row| row.person));
        actors.par_sort_unstable();
        actors.dedup();
        if let Some(physical) = physical {
            let next = plan.publish_elapsed(owner, physical.transactions())?;
            self.core.physical.commit_interval_consumption(physical)?;
            self.gathering = Some(next);
        }
        Ok(CoreGathered {
            actors,
            choices,
            receipts,
        })
    }
}

impl CoreHost {
    pub(super) fn stage_resource_renewal(
        &self,
        millis: u64,
    ) -> Result<Option<Vec<crate::ResourceNode>>, String> {
        let Some(owner) = self
            .gathering
            .as_ref()
            .filter(|owner| owner.elapsed_renewal)
        else {
            return Ok(None);
        };
        let exposure = self.environment.as_ref().and_then(|environment| {
            environment.food_growth_exposure(self.core.simulated_millis(), millis)
        });
        owner.progress_renewal(millis, exposure)
    }

    pub(super) fn publish_resource_renewal(&mut self, nodes: Option<Vec<crate::ResourceNode>>) {
        if let Some(nodes) = nodes {
            self.gathering
                .as_mut()
                .expect("prepared resource renewal")
                .nodes = nodes;
        }
    }

    pub(super) fn plan_gathering(
        &self,
        occupied: &[u64],
        external: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        base: u64,
    ) -> Result<Option<(crate::gathering::Plan, Vec<game_lot_runtime::Gathered>)>, String> {
        let Some(owner) = self
            .gathering
            .as_ref()
            .filter(|owner| !owner.elapsed_actions)
        else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let occupied = rest.as_deref().unwrap_or(occupied);
        let protected = self.blocked_with_rest(external);
        let protected = protected.as_deref().unwrap_or(external);
        let pending = self
            .schedule
            .as_ref()
            .map_or(&[][..], |schedule| schedule.pending_gathering.as_slice());
        let mut busy: Vec<_> = occupied
            .par_iter()
            .copied()
            .filter(|person| {
                protected.binary_search(person).is_ok()
                    || pending
                        .binary_search_by_key(person, |row| row.person)
                        .is_err()
            })
            .collect();
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
        let context = crate::gathering::Context {
            owner,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            busy: &busy,
            view: &view,
            tick_millis: self.core.physical.tick_millis(),
            seed: self.core.seed,
            tick,
            manual: None,
        };
        let profiled: Vec<_> = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_ref())
            .map_or_else(Vec::new, |people| {
                people.par_iter().map(|person| person.id).collect()
            });
        let plan = context.prepare_selected(base, &profiled, pending)?;
        let outputs = plan.carried_outputs(owner);
        Ok(Some((plan, outputs)))
    }
}

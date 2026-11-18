//! Renewable gathering into each individual's physical custody.
use super::*;

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
        let Some(owner) = &self.gathering else {
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
            goods: self.core.physical.goods(),
            view: &view,
            tick_millis: self.core.physical.tick_millis(),
            seed: self.core.seed,
            tick,
            manual: None,
            personal_output: true,
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

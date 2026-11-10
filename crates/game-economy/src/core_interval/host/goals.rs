//! Record personal goal work attempts at the decision that actually occurred.
use super::*;

impl CoreHost {
    pub(super) fn goal_work_candidates(
        &self,
        blocked: &[u64],
        effective: Option<&Autowork>,
    ) -> Vec<(u64, u32)> {
        let Some((effective, auto)) = effective.zip(self.autowork.as_ref()) else {
            return Vec::new();
        };
        let mut busy = blocked.to_vec();
        if let Some(owner) = &self.fatigue {
            busy.par_extend(owner.fatigue.resting());
        }
        if let Some(family) = &self.family {
            busy.par_extend(game_economy_work::underage(family));
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
        let mut direct: Vec<_> = auto
            .policies
            .par_iter()
            .map(|policy| policy.person)
            .collect();
        direct.par_sort_unstable();

        let mut candidates: Vec<_> = effective
            .policies
            .par_iter()
            .filter_map(|policy| {
                if !policy.enabled
                    || direct.binary_search(&policy.person).is_ok()
                    || busy.binary_search(&policy.person).is_ok()
                {
                    return None;
                }
                let person = self
                    .core
                    .people
                    .binary_search_by_key(&policy.person, |row| row.id)
                    .ok()
                    .map(|index| &self.core.people[index])?;
                (person.health > 0
                    && person.region.is_some()
                    && !person.controlled
                    && person.willing_to_work
                    && person.hunger <= self.core.rules.max_work_hunger)
                    .then_some((person.id, policy.recipe))
            })
            .collect();
        candidates.par_sort_unstable();
        candidates.dedup();
        candidates
    }

    pub(super) fn record_goal_setbacks(
        &mut self,
        candidates: &[(u64, u32)],
        outcomes: &[(u64, bool)],
    ) {
        if candidates.is_empty() {
            return;
        }
        let mut started: Vec<_> = outcomes
            .par_iter()
            .filter(|(_, accepted)| *accepted)
            .filter_map(|(job, _)| {
                self.core
                    .physical
                    .work()
                    .binary_search_by_key(job, |row| row.job.id)
                    .ok()
                    .map(|index| self.core.physical.work()[index].job.worker)
            })
            .collect();
        started.par_sort_unstable();
        self.goal_setbacks.par_extend(
            candidates
                .par_iter()
                .copied()
                .filter(|(person, _)| started.binary_search(person).is_err()),
        );
        self.goal_setbacks.par_sort_unstable();
        self.goal_setbacks.dedup();
    }
}

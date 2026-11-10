//! Individual dispositions guide voluntary choices and may be inherited by newborns.
use super::*;

impl CoreHost {
    pub(super) fn refresh_personality_work_refusals(&mut self) {
        let tick = self.core.physical.tick();
        if self.personality_refusal_tick != Some(tick) {
            self.personality_refusals = self
                .personality
                .as_ref()
                .map_or_else(Vec::new, |owner| owner.work_refusals(self.core.seed, tick));
            self.personality_refusals.par_sort_unstable();
            self.personality_refusal_tick = Some(tick);
        }
    }

    pub(super) fn stage_personality_births(
        &self,
        family: Option<&super::family::FamilyFrame>,
    ) -> Option<game_economy_dynamics::personality::Personality> {
        let family = family
            .filter(|family| family.owner.rules.enroll_births && !family.newborns.is_empty())?;
        let owner = self
            .personality
            .as_ref()
            .filter(|owner| owner.inherit_births)?;
        let mut members: Vec<_> = family.owner.members.par_iter().collect();
        members.par_sort_unstable_by_key(|member| member.id);
        let births: Vec<_> = family
            .newborns
            .par_iter()
            .filter_map(|id| {
                let index = members.binary_search_by_key(id, |member| member.id).ok()?;
                let member = members[index];
                let mut parents = [0; 2];
                let count = member.parents.len().min(2);
                parents[..count].copy_from_slice(&member.parents[..count]);
                Some((*id, parents, count))
            })
            .collect();
        let mut next = owner.clone();
        next.enroll_births(&births);
        Some(next)
    }
}

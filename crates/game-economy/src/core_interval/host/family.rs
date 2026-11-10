//! Staged human life history, including real newborn custody.
use super::*;

impl CoreHost {
    pub(super) fn stage_family(
        &mut self,
        draft: &mut CoreSettlementDraft,
    ) -> Result<Option<game_economy_family::Families>, String> {
        let Some(owner) = &self.family else {
            return Ok(None);
        };
        let tick = draft.events().tick;
        let next = game_economy_family::advance_validated(
            owner,
            &mut draft.people.people,
            &mut draft.travel,
            self.core.seed,
            tick,
            false,
        )?;
        let newborns: Vec<_> = next
            .newborns
            .par_iter()
            .map(|&id| (id, self.core.rules.carrying_capacity))
            .collect();
        let deceased: Vec<_> = draft
            .people
            .people
            .par_iter()
            .filter(|person| {
                person.health == 0
                    && self
                        .core
                        .people
                        .binary_search_by_key(&person.id, |old| old.id)
                        .ok()
                        .is_some_and(|index| self.core.people[index].health > 0)
            })
            .map(|person| person.id)
            .collect();
        if !newborns.is_empty() || !deceased.is_empty() {
            self.core
                .physical
                .stage_interval_people(&mut draft.physical, &newborns, &deceased)?;
        }
        Ok(Some(next.family))
    }
}

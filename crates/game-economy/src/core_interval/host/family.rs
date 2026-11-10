//! Staged human life history, including real newborn custody.
use super::*;

pub(super) struct FamilyFrame {
    pub owner: game_economy_family::Families,
    pub newborns: Vec<u64>,
    pub births: Vec<(u64, u64)>,
}

impl CoreHost {
    pub(super) fn stage_family(
        &mut self,
        draft: &mut CoreSettlementDraft,
    ) -> Result<Option<FamilyFrame>, String> {
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
            self.dependent_care.is_some(),
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
        let births = next
            .births
            .par_iter()
            .map(|birth| (birth.id, birth.mother))
            .collect();
        Ok(Some(FamilyFrame {
            owner: next.family,
            newborns: next.newborns,
            births,
        }))
    }

    pub(super) fn stage_birth_health(
        &self,
        family: Option<&FamilyFrame>,
        tick: u64,
    ) -> Option<CoreHealth> {
        let family = family.filter(|family| !family.births.is_empty())?;
        let health = self.health.as_ref().filter(|health| {
            !health.owner.cases.is_empty()
                && health
                    .owner
                    .strains
                    .iter()
                    .any(|strain| strain.birth_transmission_per_million > 0)
        })?;
        let mut next = health.clone();
        next.owner
            .admit_births_validated(&health.owner, &family.births, self.core.seed, tick);
        (next.owner.cases.len() != health.owner.cases.len()).then_some(next)
    }
}

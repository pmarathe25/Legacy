//! Staged human life history, including real newborn custody.
use super::*;

pub(super) struct FamilyFrame {
    pub owner: game_economy_family::Families,
    pub newborns: Vec<u64>,
    pub births: Vec<(u64, u64)>,
}

impl CoreHost {
    /// Enroll only configured personal preferences; relationships and claims are not inherited.
    pub(super) fn inherit_birth_preferences(&mut self, newborns: &[u64], tick: u64) {
        let Some(family) = self
            .family
            .as_ref()
            .filter(|family| family.rules.enroll_births && !newborns.is_empty())
        else {
            return;
        };
        if let Some(social) = self
            .social
            .as_mut()
            .filter(|social| social.owner.inherit_births)
        {
            let mut parents: Vec<_> = social
                .owner
                .participants
                .par_iter()
                .map(|row| (row.person, row.willing))
                .collect();
            parents.par_sort_unstable_by_key(|row| row.0);
            let choices: Vec<_> = newborns
                .par_iter()
                .filter_map(|person| {
                    let member = &family.members[family
                        .members
                        .binary_search_by_key(person, |member| member.id)
                        .ok()?];
                    let willing = member.parents.iter().find_map(|parent| {
                        parents
                            .binary_search_by_key(parent, |row| row.0)
                            .ok()
                            .map(|index| parents[index].1)
                    })?;
                    Some((*person, willing))
                })
                .collect();
            social.owner.enroll_inherited(&choices);
        }
        crate::animal_restraint_inheritance::enroll(
            self.animal_restraints.as_mut(),
            Some(family),
            tick,
        );
        crate::giving_inheritance::enroll(self.giving.as_mut(), Some(family), tick);
        let hunting = crate::hunting::enroll(self.hunting.as_ref(), Some(family), tick);
        let animal_care = crate::animal_care::enroll(self.animal_care.as_ref(), Some(family), tick);
        let handling =
            crate::animal_handling::enroll(self.animal_handling.as_ref(), Some(family), tick);
        if !hunting.is_empty() {
            self.hunting
                .as_mut()
                .expect("hunting owner")
                .policies
                .par_extend(hunting);
        }
        if !animal_care.is_empty() {
            self.animal_care
                .as_mut()
                .expect("animal care owner")
                .policies
                .par_extend(animal_care);
        }
        if !handling.is_empty() {
            self.animal_handling
                .as_mut()
                .expect("handling owner")
                .policies
                .par_extend(handling);
        }
        crate::gathering::enroll(self.gathering.as_mut(), Some(family), tick);
        game_economy_agriculture::enroll(self.agriculture.as_mut(), Some(family), tick);
        game_economy_claims::enroll(self.claims.as_mut(), Some(family), tick);
        game_economy_knowledge::inherit(&mut self.knowledge, family, newborns);
        crate::food_search::enroll(
            self.food_search.as_mut().map(|owner| &mut owner.search),
            Some(family),
            tick,
        );
    }

    pub(super) fn stage_family(
        &mut self,
        draft: &mut CoreSettlementDraft,
    ) -> Result<Option<FamilyFrame>, String> {
        let Some(owner) = &self.family else {
            return Ok(None);
        };
        let tick = draft.events().tick;
        let opinions = self
            .opinions
            .as_ref()
            .filter(|_| owner.rules.subjective_mating)
            .map(game_opinions::Opinions::index);
        let feelings = |observer, target| {
            opinions
                .as_ref()
                .map_or(game_economy_family::MateFeelings::default(), |index| {
                    let (trust, affinity, fear, obligation) = index.subjective(observer, target);
                    game_economy_family::MateFeelings {
                        trust,
                        affinity,
                        fear,
                        obligation,
                    }
                })
        };
        let next = game_economy_family::advance_validated_with_context(
            owner,
            &mut draft.people.people,
            &mut draft.travel,
            self.core.seed,
            tick,
            self.dependent_care.is_some(),
            None,
            Some(&feelings),
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
            let (eligible, until) =
                self.succession_death_suspensions(&deceased, draft.events().tick)?;
            self.core.physical.stage_interval_people_with_suspension(
                &mut draft.physical,
                &newborns,
                &deceased,
                &eligible,
                until,
            )?;
        }
        let births = if self.health.as_ref().is_some_and(|health| {
            !health.owner.cases.is_empty()
                && health
                    .owner
                    .strains
                    .iter()
                    .any(|strain| strain.birth_transmission_per_million > 0)
        }) {
            next.births
                .par_iter()
                .map(|birth| (birth.id, birth.mother))
                .collect()
        } else {
            Vec::new()
        };
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

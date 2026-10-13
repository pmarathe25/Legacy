//! Candidate membership and local review indexing from canonical Society rows.
use crate::{deliberation::Recruit, institutions::*};
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};

pub fn office_holders(store: &Store, active_only: bool) -> BTreeSet<PersonId> {
    crate::institutions(store)
        .filter(|(_, row)| !active_only || row.status == InstitutionStatus::Active)
        .flat_map(|(_, row)| row.charter.offices.iter().map(|office| office.holder))
        .collect()
}
pub fn formation_voters(store: &Store) -> BTreeSet<PersonId> {
    crate::institutions(store)
        .filter(|(_, row)| row.status == InstitutionStatus::Proposed)
        .flat_map(|(_, row)| row.charter.founders.iter().copied())
        .collect()
}
pub fn proposal_proposers(store: &Store) -> BTreeSet<PersonId> {
    crate::proposals(store)
        .map(|(_, row)| row.proposer)
        .collect()
}
pub fn authority_people(institution: &Institution) -> BTreeSet<PersonId> {
    institution
        .charter
        .offices
        .iter()
        .map(|office| office.holder)
        .chain(
            institution
                .office_support
                .values()
                .flat_map(|people| people.iter().copied()),
        )
        .collect()
}
pub fn missing_knowledge(
    store: &Store,
    people: impl IntoIterator<Item = PersonId>,
) -> Vec<PersonId> {
    people
        .into_iter()
        .filter(|id| crate::knowledge(store, *id).is_none())
        .collect()
}
pub fn initialize_knowledge(
    store: &mut Store,
    people: impl IntoIterator<Item = PersonId>,
) -> Result<(), Error> {
    let missing = missing_knowledge(store, people);
    let mut batch = game_ecs::Batch::new();
    for id in missing {
        batch.insert(game_ecs::StableId(id.0), crate::Knowledge::default());
    }
    store.apply_batch(batch)
}
pub struct DiscoveryIndex {
    by_area: BTreeMap<AreaId, Vec<Recruit>>,
    candidates: Vec<(PersonId, AreaId)>,
}
impl DiscoveryIndex {
    pub fn new(roster: &[Recruit]) -> Self {
        let mut by_area = BTreeMap::<AreaId, Vec<Recruit>>::new();
        let mut candidates = Vec::new();
        for row in roster {
            if let Some(area) = row.area {
                by_area.entry(area).or_default().push(*row);
                candidates.push((row.actor, area));
            }
        }
        Self {
            by_area,
            candidates,
        }
    }
    pub fn actors(&self) -> impl Iterator<Item = (PersonId, AreaId)> + '_ {
        self.candidates.iter().copied()
    }
    pub fn recruits(&self, area: AreaId) -> &[Recruit] {
        self.by_area.get(&area).map_or(&[], Vec::as_slice)
    }
}
pub struct ExclusionFacts<'a> {
    pub subject: crate::reactions::SubjectFacts,
    pub available_area: Option<AreaId>,
    pub known_stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
}
impl Runtime {
    pub fn clear_learning(
        &self,
        store: &mut Store,
        actor: PersonId,
        values: bool,
        precedents: bool,
    ) -> Result<(), Error> {
        let memory = crate::knowledge_mut(store, actor).ok_or(Error::Unavailable)?;
        if values {
            memory.values.clear();
        }
        if precedents {
            memory.cases.clear();
        }
        Ok(())
    }
    pub fn observed_exclusions(
        &self,
        store: &Store,
        facts: &[ExclusionFacts<'_>],
        now: SimTime,
    ) -> Vec<crate::reactions::CaseReaction> {
        let mut reactions = Vec::new();
        for fact in facts {
            let Some(area) = fact.available_area else {
                continue;
            };
            let mut subject = fact.subject;
            subject.area = Some(area);
            reactions.extend(self.exclusion_reactions(store, subject, fact.known_stocks, now));
        }
        reactions
    }
}
pub fn receipt_actor(receipt: &crate::reactions::SocialReceipt) -> PersonId {
    use crate::reactions::SocialReceipt::*;
    match *receipt {
        Performed { actor, .. }
        | Produced { actor, .. }
        | Defaulted { actor, .. }
        | SupportReceived { actor, .. }
        | Ate { actor, .. }
        | Transferred { actor, .. } => actor,
    }
}

pub struct SocialAreaFacts {
    pub area: Option<AreaId>,
    pub independent: bool,
    pub combat_restricted: bool,
    pub alive: bool,
}
pub fn social_area(facts: SocialAreaFacts) -> Result<AreaId, Error> {
    if !facts.independent || facts.combat_restricted || !facts.alive {
        return Err(Error::Unavailable);
    }
    facts.area.ok_or(Error::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_fixtures::*;
    #[test]
    fn owner_review_selects_relevant_offices_and_preserves_local_recruitment_order() {
        let (_, mut store) = world();
        let mut active = institution(10, vec![labor()], InstitutionStatus::Active);
        active.charter.offices[0].holder = PersonId(3);
        seed_institution(&mut store, active);
        let mut proposed = institution(20, vec![labor()], InstitutionStatus::Proposed);
        proposed.charter.offices[0].holder = PersonId(4);
        proposed.charter.founders = BTreeSet::from([PersonId(4), PersonId(5)]);
        seed_institution(&mut store, proposed);
        assert_eq!(office_holders(&store, true), BTreeSet::from([PersonId(3)]));
        assert_eq!(
            office_holders(&store, false),
            BTreeSet::from([PersonId(3), PersonId(4)])
        );
        assert_eq!(
            formation_voters(&store),
            BTreeSet::from([PersonId(4), PersonId(5)])
        );
        let roster = [
            Recruit {
                actor: PersonId(5),
                area: Some(AreaId(1)),
                autonomous: true,
                citizen: true,
            },
            Recruit {
                actor: PersonId(4),
                area: Some(AreaId(2)),
                autonomous: true,
                citizen: true,
            },
            Recruit {
                actor: PersonId(3),
                area: Some(AreaId(1)),
                autonomous: true,
                citizen: true,
            },
            Recruit {
                actor: PersonId(2),
                area: None,
                autonomous: true,
                citizen: true,
            },
        ];
        let index = DiscoveryIndex::new(&roster);
        assert_eq!(
            index.actors().collect::<Vec<_>>(),
            vec![
                (PersonId(5), AreaId(1)),
                (PersonId(4), AreaId(2)),
                (PersonId(3), AreaId(1))
            ]
        );
        assert_eq!(
            index
                .recruits(AreaId(1))
                .iter()
                .map(|row| row.actor)
                .collect::<Vec<_>>(),
            vec![PersonId(5), PersonId(3)]
        );
    }
}

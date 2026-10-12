//! Social interpretations of actual receipts and personally known exclusion.
use crate::institutions::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub struct SubjectFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub hunger: u64,
}
#[derive(Clone, Copy)]
pub struct CaseAsset {
    pub area: Option<AreaId>,
    pub inventory: InventoryId,
    pub good: Good,
}
#[derive(Clone, Copy)]
pub enum CooperationTarget {
    Labor { site: SiteId, asset: CaseAsset },
    Delivery { asset: CaseAsset },
}
#[derive(Clone, Copy)]
pub enum SocialReceipt {
    Performed {
        actor: PersonId,
        counterpart: PersonId,
        agreement: AgreementId,
        target: CooperationTarget,
    },
    Produced {
        actor: PersonId,
        site: SiteId,
        asset: CaseAsset,
        custodian: Option<PersonId>,
    },
    Defaulted {
        actor: PersonId,
        counterpart: PersonId,
        agreement: AgreementId,
        asset: CaseAsset,
    },
    SupportReceived {
        actor: PersonId,
        source: InventoryId,
    },
    Ate {
        actor: PersonId,
        source: InventoryId,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        good: Good,
        destination_holder: Option<PersonId>,
    },
}
pub struct ReceiptIndex {
    attributable: BTreeSet<(PersonId, SiteId)>,
}
impl ReceiptIndex {
    pub fn new(receipts: &[SocialReceipt]) -> Self {
        Self {
            attributable: receipts
                .iter()
                .filter_map(|receipt| match receipt {
                    SocialReceipt::Performed {
                        actor,
                        target: CooperationTarget::Labor { site, .. },
                        ..
                    } => Some((*actor, *site)),
                    _ => None,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseReaction {
    pub case: SocialCaseView,
    pub concept: Option<ConceptId>,
    pub public: bool,
    clear_exclusion: Option<(PersonId, InventoryId)>,
    set_exclusion: Option<(PersonId, InventoryId)>,
    exclusion_guard: Option<(PersonId, InventoryId, Option<CaseId>)>,
}
pub fn need_band(hunger: u64) -> u8 {
    if hunger >= 540_000 {
        3
    } else if hunger >= 300_000 {
        2
    } else if hunger >= 120_000 {
        1
    } else {
        0
    }
}
impl World {
    pub fn linked_rule_concept(&self, agreement: AgreementId) -> Option<ConceptId> {
        self.institutions.values().find_map(|institution| {
            institution.applied.iter().find_map(|(concept, people)| {
                people
                    .values()
                    .any(|id| *id == agreement)
                    .then_some(*concept)
            })
        })
    }
    pub fn sharing_concept(&self, actor: PersonId, source: InventoryId) -> Option<ConceptId> {
        self.institutions.values().find_map(|institution|institution.concepts.iter().find(|id|
            matches!(self.concepts[id].effect,SocialEffect::Access{inventory,allowed:true}if inventory==source)
                &&institution.grants.get(id).is_some_and(|people|people.contains(&actor))).copied())
    }
    /// Interpret one receipt against state after the previous receipt has committed.
    pub fn receipt_reaction(
        &self,
        receipt: &SocialReceipt,
        subject: SubjectFacts,
        at: SimTime,
        index: &ReceiptIndex,
    ) -> Option<CaseReaction> {
        let make = |counterpart, asset: CaseAsset, kind, outcome, reciprocal, concept, public| {
            Some(CaseReaction {
                case: SocialCaseView {
                    id: CaseId(0),
                    kind,
                    subject: subject.actor,
                    counterpart,
                    area: asset.area?,
                    inventory: Some(asset.inventory),
                    good: asset.good,
                    need_band: need_band(subject.hunger),
                    reciprocal,
                    outcome,
                    observed_at: at,
                },
                concept,
                public,
                clear_exclusion: None,
                set_exclusion: None,
                exclusion_guard: None,
            })
        };
        match *receipt {
            SocialReceipt::Performed {
                actor,
                counterpart,
                agreement,
                target,
            } => {
                if actor != subject.actor {
                    return None;
                }
                let asset = match target {
                    CooperationTarget::Labor { asset, .. }
                    | CooperationTarget::Delivery { asset } => asset,
                };
                make(
                    counterpart,
                    asset,
                    SocialCaseKind::Cooperation,
                    SocialOutcome::Contributed,
                    true,
                    self.linked_rule_concept(agreement),
                    true,
                )
            }
            SocialReceipt::Produced {
                actor,
                site,
                asset,
                custodian,
            } => {
                if actor != subject.actor || index.attributable.contains(&(actor, site)) {
                    return None;
                }
                let counterpart = custodian.filter(|owner| *owner != actor)?;
                make(
                    counterpart,
                    asset,
                    SocialCaseKind::Cooperation,
                    SocialOutcome::Contributed,
                    true,
                    None,
                    true,
                )
            }
            SocialReceipt::Defaulted {
                actor,
                counterpart,
                agreement,
                mut asset,
            } => {
                if actor != subject.actor {
                    return None;
                }
                asset.area = subject.area;
                let concept = self.linked_rule_concept(agreement);
                make(
                    counterpart,
                    asset,
                    SocialCaseKind::Default,
                    SocialOutcome::Defaulted,
                    self.known_case_count(actor, actor, SocialCaseKind::Cooperation) > 0,
                    concept,
                    concept.is_some(),
                )
            }
            SocialReceipt::Transferred {
                actor,
                source,
                good,
                destination_holder,
            } => {
                if good != Good::Food || destination_holder != Some(actor) {
                    return None;
                }
                self.receipt_reaction(
                    &SocialReceipt::SupportReceived { actor, source },
                    subject,
                    at,
                    index,
                )
            }
            SocialReceipt::Ate { actor, source } => self.receipt_reaction(
                &SocialReceipt::SupportReceived { actor, source },
                subject,
                at,
                index,
            ),
            SocialReceipt::SupportReceived { actor, source } => {
                if actor != subject.actor {
                    return None;
                }
                let old = self
                    .exclusions
                    .get(&actor)?
                    .get(&source)
                    .and_then(|id| self.cases.get(id))?;
                Some(CaseReaction {
                    case: SocialCaseView {
                        id: CaseId(0),
                        outcome: SocialOutcome::Shared,
                        observed_at: at,
                        ..old.clone()
                    },
                    concept: self.sharing_concept(actor, source),
                    public: true,
                    clear_exclusion: Some((actor, source)),
                    set_exclusion: None,
                    exclusion_guard: Some((
                        actor,
                        source,
                        self.exclusions
                            .get(&actor)
                            .and_then(|rows| rows.get(&source))
                            .copied(),
                    )),
                })
            }
        }
    }
    pub fn exclusion_reactions(
        &self,
        subject: SubjectFacts,
        known: &BTreeMap<InventoryId, StockKnowledge>,
        custodians: &BTreeMap<InventoryId, PersonId>,
        at: SimTime,
    ) -> Vec<CaseReaction> {
        let Some(area) = subject.area else {
            return Vec::new();
        };
        let band = need_band(subject.hunger);
        if band == 0
            || known.values().any(|stock| {
                stock.area == area
                    && stock.good == Good::Food
                    && stock.accessible
                    && stock.quantity.0 >= 1_000
            })
        {
            return Vec::new();
        }
        known
            .values()
            .filter(|stock| {
                stock.area == area
                    && stock.good == Good::Food
                    && !stock.accessible
                    && stock.quantity.0 >= 1_000
            })
            .take(8)
            .filter_map(|stock| {
                let owner = *custodians.get(&stock.inventory)?;
                if self
                    .exclusions
                    .get(&subject.actor)
                    .and_then(|rows| rows.get(&stock.inventory))
                    .and_then(|id| self.cases.get(id))
                    .is_some_and(|case| case.need_band == band)
                {
                    return None;
                }
                Some(CaseReaction {
                    case: SocialCaseView {
                        id: CaseId(0),
                        kind: SocialCaseKind::Exclusion,
                        subject: subject.actor,
                        counterpart: owner,
                        area,
                        inventory: Some(stock.inventory),
                        good: stock.good,
                        need_band: band,
                        reciprocal: self.known_case_count(
                            subject.actor,
                            subject.actor,
                            SocialCaseKind::Cooperation,
                        ) > 0,
                        outcome: SocialOutcome::Unresolved,
                        observed_at: at,
                    },
                    concept: None,
                    public: false,
                    clear_exclusion: None,
                    set_exclusion: Some((subject.actor, stock.inventory)),
                    exclusion_guard: Some((
                        subject.actor,
                        stock.inventory,
                        self.exclusions
                            .get(&subject.actor)
                            .and_then(|rows| rows.get(&stock.inventory))
                            .copied(),
                    )),
                })
            })
            .collect()
    }
    pub fn commit_case_reaction(
        &mut self,
        mut reaction: CaseReaction,
        id: CaseId,
        observers: &BTreeMap<PersonId, u16>,
    ) -> Result<(), Error> {
        if id.0 == 0
            || self.cases.contains_key(&id)
            || reaction
                .concept
                .is_some_and(|concept| !self.concepts.contains_key(&concept))
            || reaction
                .exclusion_guard
                .is_some_and(|(actor, source, before)| {
                    self.exclusions
                        .get(&actor)
                        .and_then(|rows| rows.get(&source))
                        .copied()
                        != before
                })
        {
            return Err(Error::Unavailable);
        }
        if let Some((actor, source)) = reaction.clear_exclusion
            && self
                .exclusions
                .get(&actor)
                .and_then(|rows| rows.get(&source))
                .and_then(|id| self.cases.get(id))
                .is_none_or(|case| {
                    case.subject != reaction.case.subject
                        || case.inventory != reaction.case.inventory
                        || case.area != reaction.case.area
                })
        {
            return Err(Error::Unavailable);
        }
        reaction.case.id = id;
        self.remember_case(reaction.case, reaction.concept, observers);
        if let Some((actor, source)) = reaction.clear_exclusion {
            self.exclusions.get_mut(&actor).unwrap().remove(&source);
        }
        if let Some((actor, source)) = reaction.set_exclusion {
            self.exclusions.entry(actor).or_default().insert(source, id);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;

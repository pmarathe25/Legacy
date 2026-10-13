//! Social interpretations of actual receipts and personally known exclusion.
use crate::institutions::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
mod receipt_batch;
pub use receipt_batch::*;
mod publication;
pub use publication::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubjectFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub hunger: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseAsset {
    pub area: Option<AreaId>,
    pub inventory: InventoryId,
    pub good: Good,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CooperationTarget {
    Labor { site: SiteId, asset: CaseAsset },
    Delivery { asset: CaseAsset },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
/// Factual receipt links required by a bounded native receiving projection.
pub fn receipt_people(receipt: &SocialReceipt) -> BTreeSet<PersonId> {
    match *receipt {
        SocialReceipt::Performed {
            actor, counterpart, ..
        }
        | SocialReceipt::Defaulted {
            actor, counterpart, ..
        } => BTreeSet::from([actor, counterpart]),
        SocialReceipt::Produced {
            actor, custodian, ..
        } => [Some(actor), custodian].into_iter().flatten().collect(),
        SocialReceipt::Transferred {
            actor,
            destination_holder,
            ..
        } => [Some(actor), destination_holder]
            .into_iter()
            .flatten()
            .collect(),
        SocialReceipt::SupportReceived { actor, .. } | SocialReceipt::Ate { actor, .. } => {
            BTreeSet::from([actor])
        }
    }
}
pub fn receipt_asset(receipt: &SocialReceipt) -> Option<CaseAsset> {
    match *receipt {
        SocialReceipt::Performed { target, .. } => Some(match target {
            CooperationTarget::Labor { asset, .. } | CooperationTarget::Delivery { asset } => asset,
        }),
        SocialReceipt::Produced { asset, .. } | SocialReceipt::Defaulted { asset, .. } => {
            Some(asset)
        }
        _ => None,
    }
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
impl Runtime {
    pub fn linked_rule_concept(&self, store: &Store, agreement: AgreementId) -> Option<ConceptId> {
        receipt_batch::Selector::Linked(agreement)
            .select(store, None)
            .map(|(_, concept)| concept)
    }
    pub fn sharing_concept(
        &self,
        store: &Store,
        actor: PersonId,
        source: InventoryId,
    ) -> Option<ConceptId> {
        receipt_batch::Selector::Sharing(actor, source)
            .select(store, None)
            .map(|(_, concept)| concept)
    }
    /// Interpret one receipt against state after the previous receipt has committed.
    pub fn receipt_reaction(
        &self,
        store: &Store,
        receipt: &SocialReceipt,
        subject: SubjectFacts,
        at: SimTime,
        index: &ReceiptIndex,
    ) -> Option<CaseReaction> {
        self.receipt_reaction_in(
            store,
            receipt,
            subject,
            at,
            index,
            &receipt_batch::ReceiptRows::opening(store),
        )
    }
    fn receipt_reaction_in(
        &self,
        store: &Store,
        receipt: &SocialReceipt,
        subject: SubjectFacts,
        at: SimTime,
        index: &ReceiptIndex,
        rows: &receipt_batch::ReceiptRows<'_>,
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
                    self.linked_rule_concept(store, agreement),
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
                let concept = self.linked_rule_concept(store, agreement);
                make(
                    counterpart,
                    asset,
                    SocialCaseKind::Default,
                    SocialOutcome::Defaulted,
                    rows.known_count(actor, actor, SocialCaseKind::Cooperation) > 0,
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
                self.receipt_reaction_in(
                    store,
                    &SocialReceipt::SupportReceived { actor, source },
                    subject,
                    at,
                    index,
                    rows,
                )
            }
            SocialReceipt::Ate { actor, source } => self.receipt_reaction_in(
                store,
                &SocialReceipt::SupportReceived { actor, source },
                subject,
                at,
                index,
                rows,
            ),
            SocialReceipt::SupportReceived { actor, source } => {
                if actor != subject.actor {
                    return None;
                }
                let old = rows.excluded_case(actor, source)?;
                Some(CaseReaction {
                    case: SocialCaseView {
                        id: CaseId(0),
                        outcome: SocialOutcome::Shared,
                        observed_at: at,
                        ..old.clone()
                    },
                    concept: self.sharing_concept(store, actor, source),
                    public: true,
                    clear_exclusion: Some((actor, source)),
                    set_exclusion: None,
                    exclusion_guard: Some((actor, source, rows.exclusion_id(actor, source))),
                })
            }
        }
    }
    pub fn exclusion_reactions(
        &self,
        store: &Store,
        subject: SubjectFacts,
        known: &BTreeMap<InventoryId, StockKnowledge>,
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
                let owner = game_property::inventory_custodian(store, stock.inventory)?;
                if crate::storage::exclusions(store, subject.actor)
                    .and_then(|rows| rows.stores.get(&stock.inventory))
                    .and_then(|id| crate::storage::case(store, id))
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
                            store,
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
                        crate::storage::exclusions(store, subject.actor)
                            .and_then(|rows| rows.stores.get(&stock.inventory))
                            .copied(),
                    )),
                })
            })
            .collect()
    }
    pub fn stage_case_reaction(
        &self,
        store: &Store,
        reaction: CaseReaction,
        id: CaseId,
        observers: &BTreeMap<PersonId, u16>,
        batch: &mut Batch,
    ) -> Result<(), Error> {
        let at = reaction.case.observed_at;
        let publication = self
            .prepare_case_reaction(store, reaction, id, observers)?
            .stage_into(self, store, at, batch)?;
        publication.validate(self, store, at, batch)
    }
    pub fn commit_case_reaction(
        &self,
        store: &mut Store,
        reaction: CaseReaction,
        id: CaseId,
        observers: &BTreeMap<PersonId, u16>,
    ) -> Result<(), Error> {
        let at = reaction.case.observed_at;
        let mut batch = Batch::new();
        let publication = self
            .prepare_case_reaction(store, reaction, id, observers)?
            .stage_into(self, store, at, &mut batch)?;
        publication.validate(self, store, at, &batch)?;
        store.apply_batch(batch)
    }
}
#[cfg(test)]
mod tests;

use game_agreements::AgreementTerm;
use game_core::*;
use game_missions::MissionProposal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub institution: InstitutionId,
    pub settlement: AreaId,
    pub representative: PersonId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    Delivery {
        provider: InstitutionId,
        beneficiary: InstitutionId,
        term: AgreementTerm,
    },
    Passage {
        grantor: InstitutionId,
        beneficiary: InstitutionId,
        areas: BTreeSet<AreaId>,
        until: SimTime,
    },
    /// A specified patrol service, not a guarantee that no harm can occur.
    Protection {
        provider: InstitutionId,
        beneficiary: InstitutionId,
        patrol: MissionProposal,
    },
}
impl Term {
    pub fn provider(&self) -> InstitutionId {
        match self {
            Self::Delivery { provider, .. } | Self::Protection { provider, .. } => *provider,
            Self::Passage { grantor, .. } => *grantor,
        }
    }
    pub fn beneficiary(&self) -> InstitutionId {
        match self {
            Self::Delivery { beneficiary, .. }
            | Self::Protection { beneficiary, .. }
            | Self::Passage { beneficiary, .. } => *beneficiary,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub proposer: PersonId,
    pub participants: BTreeMap<InstitutionId, Participant>,
    pub terms: Vec<Term>,
    pub ratify_by: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Propose {
        proposal: Proposal,
    },
    Revise {
        commitment: RealmCommitmentId,
        proposal: Proposal,
    },
    RequestWork {
        key: TermKey,
    },
    Report {
        commitment: RealmCommitmentId,
        recipient: PersonId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownCommitment {
    pub id: RealmCommitmentId,
    pub knowledge: Knowledge,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub commitments: Vec<KnownCommitment>,
}

/// Scoped facts supplied from native institutional authority, never persisted as
/// another office or used to bypass that owner's chosen decision procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentativeFacts {
    pub settlement: AreaId,
    pub representative: PersonId,
    pub may_negotiate: bool,
    pub delivery_caps: BTreeMap<InventoryId, Quantity>,
    pub supply_caps: BTreeMap<InventoryId, Quantity>,
    pub passage_areas: BTreeSet<AreaId>,
    pub protection_areas: BTreeSet<AreaId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryFacts {
    pub good: Good,
    /// Unknown remote capacities stay unknown. A total promise can exceed one
    /// compartment and be fulfilled through several physical deliveries.
    pub capacity: Option<Quantity>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProposalFacts {
    pub representatives: BTreeMap<InstitutionId, RepresentativeFacts>,
    pub known_people: BTreeSet<PersonId>,
    pub known_areas: BTreeSet<AreaId>,
    pub inventories: BTreeMap<InventoryId, InventoryFacts>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TermKey {
    pub commitment: RealmCommitmentId,
    pub revision: u32,
    pub index: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Adopted,
    Rejected,
}
/// Adapter input from a completed native proposal, not a player command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ratification {
    pub proposal: RuleProposalId,
    pub outcome: Decision,
    pub decided_at: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Backing {
    Delivery {
        agreement: AgreementId,
        obligation: Option<ObligationId>,
    },
    Passage {
        institution: InstitutionId,
        proposal: RuleProposalId,
    },
    Protection {
        mission: MissionId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Progress {
    AwaitingWork,
    Active,
    Fulfilled,
    Defaulted,
    Refused,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackedTerm {
    pub backing: Backing,
    pub progress: Progress,
    pub performed: Quantity,
    pub changed_at: SimTime,
    pub defaulted_at: Option<SimTime>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    AwaitingRatification,
    Rejected,
    Ratified,
    Active,
    Blocked,
    Fulfilled,
    Expired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub number: u32,
    pub proposal: Proposal,
    pub proposed_at: SimTime,
    pub ratification_proposals: BTreeMap<InstitutionId, RuleProposalId>,
    pub ratifications: BTreeMap<InstitutionId, Ratification>,
    pub requested: BTreeSet<u16>,
    pub backed: BTreeMap<u16, BackedTerm>,
    pub expired_at: Option<SimTime>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub id: RealmCommitmentId,
    /// Revising a commitment preserves old work and obligation references.
    pub revisions: Vec<Revision>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Ratification {
        commitment: RealmCommitmentId,
        revision: u32,
        institution: InstitutionId,
        representative: PersonId,
    },
    Delivery {
        key: TermKey,
        parties: BTreeSet<PersonId>,
        term: AgreementTerm,
    },
    Passage {
        key: TermKey,
        institution: InstitutionId,
        beneficiary: InstitutionId,
        areas: BTreeSet<AreaId>,
        until: SimTime,
    },
    Protection {
        key: TermKey,
        sponsor: PersonId,
        patrol: MissionProposal,
    },
}

/// Actual owner receipts bind the precisely requested work. IDs alone are not
/// sufficient proof: the owner supplies the accepted term or applied scope too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkReceipt {
    DeliveryProposed {
        agreement: AgreementId,
        term: AgreementTerm,
    },
    DeliveryOpened {
        agreement: AgreementId,
        obligation: Option<ObligationId>,
        accepted: bool,
        term: AgreementTerm,
    },
    PassageApplied {
        institution: InstitutionId,
        proposal: RuleProposalId,
        adopted: bool,
        beneficiary: InstitutionId,
        areas: BTreeSet<AreaId>,
        until: SimTime,
    },
    ProtectionOpened {
        mission: MissionId,
        accepted: bool,
        patrol: MissionProposal,
    },
    ProtectionProposed {
        mission: MissionId,
        patrol: MissionProposal,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeReceipt {
    Delivery {
        obligation: ObligationId,
        performed: Quantity,
        defaulted: bool,
    },
    Passage {
        proposal: RuleProposalId,
        available: bool,
    },
    Protection {
        mission: MissionId,
        progress: Progress,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dated<T> {
    pub value: T,
    pub observed_at: SimTime,
    pub learned_at: SimTime,
    pub source: KnowledgeSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownRevision {
    pub proposal: Dated<Proposal>,
    pub ratifications: BTreeMap<InstitutionId, Dated<Ratification>>,
    pub backed: BTreeMap<u16, Dated<BackedTerm>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knowledge {
    pub revisions: BTreeMap<u32, KnownRevision>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub commitment: RealmCommitmentId,
    pub speaker: PersonId,
    pub sent_at: SimTime,
    /// Frozen personal knowledge, never a pointer to the latest global record.
    pub snapshot: Knowledge,
}

/// Captured by the physical/event owner when the statement is witnessed. A
/// later arrival cannot supply presence retroactively from its current location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalWitness {
    pub observer: PersonId,
    pub observer_area: AreaId,
    pub statement_area: AreaId,
    pub observed_at: SimTime,
}
impl LocalWitness {
    pub(crate) fn actor(self, now: SimTime) -> Result<PersonId, Error> {
        if self.observer.0 == 0
            || self.observer_area != self.statement_area
            || self.observed_at > now
        {
            Err(Error::Unavailable)
        } else {
            Ok(self.observer)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub commitments: BTreeMap<RealmCommitmentId, Commitment>,
    pub known: BTreeMap<PersonId, BTreeMap<RealmCommitmentId, Knowledge>>,
}

/// Existing records supplied by their owners for save validation. The realm
/// keeps references to these entities, not competing authoritative copies.
#[derive(Clone, Debug, Default)]
pub struct References {
    pub people: BTreeSet<PersonId>,
    pub areas: BTreeSet<AreaId>,
    pub institutions: BTreeSet<InstitutionId>,
    pub inventories: BTreeSet<InventoryId>,
    pub agreements: BTreeSet<AgreementId>,
    pub obligations: BTreeSet<ObligationId>,
    pub proposals: BTreeSet<RuleProposalId>,
    pub missions: BTreeSet<MissionId>,
    pub records: BTreeSet<RecordId>,
}

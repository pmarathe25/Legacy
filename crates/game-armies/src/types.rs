use game_core::*;
use game_missions::{MissionObjective, MissionStatus, MissionView};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DETACHMENTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmyAction {
    Approach {
        army: ArmyId,
        mission: MissionId,
        encounter: EncounterId,
    },
    Form {
        missions: BTreeSet<MissionId>,
    },
    Respond {
        army: ArmyId,
        mission: MissionId,
        accept: bool,
    },
    Order {
        army: ArmyId,
        missions: BTreeSet<MissionId>,
        objective: MissionObjective,
        expires: SimTime,
    },
    RespondOrder {
        army: ArmyId,
        order: ArmyOrderId,
        mission: MissionId,
        response: OrderResponse,
    },
    Report {
        army: ArmyId,
        mission: MissionId,
        recipient: PersonId,
    },
    Leave {
        army: ArmyId,
        mission: MissionId,
        reason: Departure,
    },
    Disband {
        army: ArmyId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Participation {
    Offered,
    Accepted,
    Refused,
    Withdrawn,
    Retreated,
    Defected,
}
impl Participation {
    pub fn active(self) -> bool {
        self == Self::Accepted
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Departure {
    Withdraw,
    Retreat,
    Defect,
}
impl Departure {
    pub(crate) fn participation(self) -> Participation {
        match self {
            Self::Withdraw => Participation::Withdrawn,
            Self::Retreat => Participation::Retreated,
            Self::Defect => Participation::Defected,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderResponse {
    Accept,
    Refuse,
    Retreat,
    Defect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    AwaitingDelivery,
    Received,
    Accepted,
    Refused,
    Superseded,
    Expired,
    Retreated,
    Defected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detachment {
    pub mission: MissionId,
    pub leader: PersonId,
    pub participation: Participation,
    pub invited_at: SimTime,
    pub heard_at: Option<SimTime>,
    pub changed_at: SimTime,
    pub explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderRecipient {
    pub leader: PersonId,
    pub status: OrderStatus,
    pub received_at: Option<SimTime>,
    pub responded_at: Option<SimTime>,
    pub explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub id: ArmyOrderId,
    pub commander: PersonId,
    pub objective: MissionObjective,
    pub issued_at: SimTime,
    pub expires: SimTime,
    pub recipients: BTreeMap<MissionId, OrderRecipient>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmyReport {
    pub id: ArmyReportId,
    pub army: ArmyId,
    pub mission: MissionId,
    pub speaker: PersonId,
    pub sent_at: SimTime,
    /// A frozen personally known mission account, not a live army roster.
    pub snapshot: MissionView,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Army {
    pub id: ArmyId,
    pub commander: PersonId,
    pub formed_at: SimTime,
    pub disbanded_at: Option<SimTime>,
    pub detachments: BTreeMap<MissionId, Detachment>,
    pub orders: BTreeMap<ArmyOrderId, Order>,
    pub reports: BTreeMap<ArmyReportId, ArmyReport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dated<T> {
    pub value: T,
    pub observed_at: SimTime,
    pub learned_at: SimTime,
    pub source: KnowledgeSource,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmyKnowledge {
    pub commander: Option<Dated<PersonId>>,
    pub disbanded_at: Option<Dated<SimTime>>,
    pub detachments: BTreeMap<MissionId, Dated<Detachment>>,
    /// Leaders hear only the recipient portion addressed to them.
    pub orders: BTreeMap<ArmyOrderId, Dated<Order>>,
    pub responses: BTreeMap<ArmyOrderId, BTreeMap<MissionId, Dated<OrderRecipient>>>,
    pub reports: BTreeMap<ArmyReportId, Dated<ArmyReport>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmyView {
    pub id: ArmyId,
    pub knowledge: ArmyKnowledge,
    pub local_engagements: Vec<LocalEngagement>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalEngagement {
    pub mission: MissionId,
    pub encounter: EncounterId,
    pub target: PersonId,
    pub observed_at: SimTime,
}
/// Authoritative mission-owner input used only for execution validation.
/// No copy of members, supplies or positions is retained by the army.
#[derive(Clone, Debug)]
pub struct DetachmentFacts {
    pub mission: MissionId,
    pub leader: PersonId,
    pub members: BTreeSet<PersonId>,
    pub status: MissionStatus,
    pub leader_capable: bool,
}
impl DetachmentFacts {
    pub(crate) fn available(&self) -> bool {
        self.leader_capable
            && self.members.contains(&self.leader)
            && matches!(
                self.status,
                MissionStatus::Preparing
                    | MissionStatus::Traveling
                    | MissionStatus::Operating
                    | MissionStatus::Returning
            )
    }
}

/// The coordinator supplies this only after actual communication completes,
/// having checked physical contact, hearing and elapsed conversation time.
#[derive(Clone, Copy, Debug)]
pub struct Communication {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub started_at: SimTime,
    pub completed_at: SimTime,
}
impl Communication {
    pub(crate) fn valid(
        self,
        speaker: PersonId,
        listener: PersonId,
        earliest: SimTime,
        now: SimTime,
    ) -> bool {
        self.speaker == speaker
            && self.listener == listener
            && speaker != listener
            && self.started_at >= earliest
            && self.started_at < self.completed_at
            && self.completed_at == now
    }
}

/// These are requests to existing owners, not authority to move goods, people
/// or overwrite a companion's decision. Preflight them before patch commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    OfferObjective {
        mission: MissionId,
        leader: PersonId,
        objective: MissionObjective,
    },
    OfferRetreat {
        mission: MissionId,
        leader: PersonId,
    },
    ReleaseCommand {
        mission: MissionId,
        leader: PersonId,
        reason: Departure,
    },
}

/// Native Mission operations selected by Army departure and command policy.
pub enum MissionRequest<'a> {
    Directive {
        mission: MissionId,
        leader: PersonId,
        objective: &'a MissionObjective,
    },
    Retreat {
        mission: MissionId,
        leader: PersonId,
    },
    Leave {
        mission: MissionId,
        leader: PersonId,
    },
}

#[derive(Clone, Debug)]
pub struct ParticipationDecision {
    pub army: ArmyId,
    pub mission: MissionId,
    pub actor: PersonId,
    pub accept: bool,
    pub explanation: String,
}
#[derive(Clone, Debug)]
pub struct OrderProposal {
    pub army: ArmyId,
    pub id: ArmyOrderId,
    pub commander: PersonId,
    pub missions: BTreeSet<MissionId>,
    pub objective: MissionObjective,
    pub expires: SimTime,
}
#[derive(Clone, Debug)]
pub struct OrderDecision {
    pub army: ArmyId,
    pub order: ArmyOrderId,
    pub mission: MissionId,
    pub actor: PersonId,
    pub response: OrderResponse,
    pub explanation: String,
}

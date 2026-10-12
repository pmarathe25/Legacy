use crate::institutions::*;
use crate::voting::*;
use game_core::*;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[derive(Default)]
pub struct Knowledge {
    pub stocks: BTreeMap<InventoryId, StockKnowledge>,
    pub custodians: BTreeMap<InventoryId, PersonId>,
    pub trust: BTreeMap<PersonId, i32>,
}
impl VotingKnowledge for Knowledge {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        self.custodians.get(&id).copied()
    }
    fn trust(&self, id: PersonId) -> i32 {
        self.trust.get(&id).copied().unwrap_or(0)
    }
}
pub fn voter(knowledge: &Knowledge) -> VoterFacts<'_> {
    VoterFacts {
        actor: PersonId(1),
        local_area: Some(AreaId(3)),
        autonomous: true,
        empathy: 500,
        supports_others: false,
        knowledge,
    }
}
pub fn world() -> World {
    World {
        institutions: BTreeMap::new(),
        proposals: BTreeMap::new(),
        concepts: BTreeMap::new(),
        cases: BTreeMap::new(),
        memories: BTreeMap::new(),
        last_event: 0,
        exclusions: BTreeMap::new(),
        heard: BTreeMap::new(),
    }
}
pub fn labor() -> InstitutionalRule {
    InstitutionalRule {
        subjects: Subjects::named(PersonId(1)),
        when: RuleExpression::Fact(SocialCondition::Member),
        effect: SocialEffect::Labor {
            site: SiteId(20),
            millis: 60_000,
            beneficiary: Beneficiary::Person(PersonId(2)),
            due_after: SimDuration(180_000),
        },
        authority: 0,
    }
}
pub fn charter(rules: Vec<InstitutionalRule>) -> InstitutionCharter {
    InstitutionCharter {
        purpose: "Chosen work".into(),
        jurisdiction: AreaId(3),
        founders: BTreeSet::from([PersonId(1), PersonId(2)]),
        open_membership: true,
        procedure: DecisionProcedure {
            proposers: Subjects::members(),
            voters: Subjects::members(),
            minimum_support: 1,
        },
        offices: vec![OfficeMandate {
            slot: 0,
            holder: PersonId(2),
            inventories: BTreeSet::from([InventoryId(4)]),
            sites: BTreeSet::from([SiteId(20)]),
            powers: BTreeSet::from([OfficePower::Access, OfficePower::Labor]),
            minimum_support: 1,
        }],
        rules,
        contributions: Vec::new(),
        evidence: BTreeSet::new(),
    }
}
pub fn institution(
    id: u64,
    rules: Vec<InstitutionalRule>,
    status: InstitutionStatus,
) -> Institution {
    Institution {
        id: InstitutionId(id),
        origin: None,
        charter: charter(rules),
        members: BTreeSet::new(),
        declined: BTreeSet::new(),
        invited: BTreeSet::new(),
        status,
        concepts: Vec::new(),
        office_support: BTreeMap::new(),
        grants: BTreeMap::new(),
        applied: BTreeMap::new(),
        funding: Vec::new(),
        revision: 0,
    }
}
pub fn stock(amount: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(4),
        area: AreaId(3),
        good: Good::Food,
        quantity: Quantity(amount),
        accessible: false,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(1)),
    }
}

pub fn case(id: u64, actor: u64, kind: SocialCaseKind, outcome: SocialOutcome) -> SocialCaseView {
    SocialCaseView {
        id: CaseId(id),
        kind,
        subject: PersonId(actor),
        counterpart: PersonId(2),
        area: AreaId(3),
        inventory: Some(InventoryId(4)),
        good: Good::Food,
        need_band: 3,
        reciprocal: false,
        outcome,
        observed_at: SimTime(0),
    }
}
pub fn refs() -> ReferenceFacts {
    let people = (1..=12)
        .map(PersonId)
        .collect::<std::collections::BTreeSet<_>>();
    ReferenceFacts {
        people: people.clone(),
        eligible_locations: people.iter().map(|id| (*id, AreaId(3))).collect(),
        stocks: BTreeMap::from([(
            InventoryId(4),
            StockReference {
                area: Some(AreaId(3)),
                custodian: Some(PersonId(2)),
                good: Good::Food,
                capacity: Quantity(100_000),
            },
        )]),
        sites: BTreeMap::from([(SiteId(20), AreaId(3))]),
    }
}

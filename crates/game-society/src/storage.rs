//! Society's canonical entity rows in the shared store.
use crate::institutions::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Concept {
    pub rule: InstitutionalRule,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub cases: BTreeSet<CaseId>,
    pub values: BTreeMap<ConceptId, ConceptValue>,
    pub institutions: BTreeMap<InstitutionId, InstitutionView>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub search_after: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusions {
    pub stores: BTreeMap<InventoryId, CaseId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hearing {
    pub shared: bool,
}
fn bad() -> Error {
    Error::InvalidConfiguration("invalid society component".into())
}
fn subjects_valid(subjects: &Subjects) -> bool {
    (subjects.members || !subjects.named.is_empty() || !subjects.offices.is_empty())
        && subjects.named.len() <= 64
        && subjects.named.iter().all(|id| id.0 > 0)
        && subjects.offices.len() <= 8
}
fn expression_valid(expression: &RuleExpression, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > 4 || *nodes > 24 {
        return false;
    }
    match expression {
        RuleExpression::All(items) | RuleExpression::Any(items) => {
            !items.is_empty()
                && items.len() <= 8
                && items
                    .iter()
                    .all(|item| expression_valid(item, depth + 1, nodes))
        }
        RuleExpression::Not(item) => expression_valid(item, depth + 1, nodes),
        RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
            inventory,
            quantity,
        }) => inventory.0 > 0 && quantity.0 > 0,
        RuleExpression::Fact(SocialCondition::Excluded { inventory }) => inventory.0 > 0,
        RuleExpression::Fact(
            SocialCondition::KnownCooperationAtLeast(n) | SocialCondition::KnownDefaultsAtLeast(n),
        ) => (1..=64).contains(n),
        RuleExpression::Fact(SocialCondition::HungerAtLeast(n)) => *n > 0,
        _ => true,
    }
}
pub fn validate_rule_shape(rule: &InstitutionalRule) -> Result<(), Error> {
    let beneficiary = |who: &Beneficiary| match who {
        Beneficiary::Person(id) => id.0 > 0,
        Beneficiary::Office(_) => true,
    };
    let effect = match &rule.effect {
        SocialEffect::Access { inventory, .. } => inventory.0 > 0,
        SocialEffect::Labor {
            site,
            millis,
            beneficiary: who,
            due_after,
        } => site.0 > 0 && *millis > 0 && due_after.0 >= *millis && beneficiary(who),
        SocialEffect::Delivery {
            source,
            destination,
            quantity,
            beneficiary: who,
            due_after,
        } => {
            source.0 > 0
                && destination.0 > 0
                && source != destination
                && quantity.0 > 0
                && due_after.0 >= 120_000
                && beneficiary(who)
        }
    };
    if subjects_valid(&rule.subjects) && expression_valid(&rule.when, 0, &mut 0) && effect {
        Ok(())
    } else {
        Err(bad())
    }
}
impl Institution {
    pub fn validate_shape(&self) -> Result<(), Error> {
        let charter = &self.charter;
        if self.id.0 == 0
            || self.origin.is_some_and(|id| id.0 == 0 || id == self.id)
            || charter.jurisdiction.0 == 0
            || charter.purpose.is_empty()
            || charter.purpose.len() > 160
            || !(2..=8).contains(&charter.founders.len())
            || charter.founders.iter().any(|id| id.0 == 0)
            || self.members.len() > 64
            || self
                .members
                .iter()
                .chain(&self.declined)
                .chain(&self.invited)
                .any(|id| id.0 == 0)
            || !self.declined.is_disjoint(&self.members)
            || self.concepts.len() != charter.rules.len()
            || self.concepts.iter().any(|id| id.0 == 0)
            || self.concepts.iter().collect::<BTreeSet<_>>().len() != self.concepts.len()
            || charter.rules.len() > 16
            || charter.offices.is_empty()
            || charter.offices.len() > 8
            || charter.contributions.len() > 16
            || charter.evidence.len() > 8
            || !subjects_valid(&charter.procedure.proposers)
            || !subjects_valid(&charter.procedure.voters)
            || !(1..=64).contains(&charter.procedure.minimum_support)
        {
            return Err(bad());
        }
        let mut slots = BTreeSet::new();
        for office in &charter.offices {
            if !slots.insert(office.slot)
                || office.holder.0 == 0
                || office.minimum_support == 0
                || usize::from(office.minimum_support) > charter.founders.len()
                || office.inventories.len() > 16
                || office.sites.len() > 16
                || office.powers.is_empty()
                || office.inventories.iter().any(|id| id.0 == 0)
                || office.sites.iter().any(|id| id.0 == 0)
            {
                return Err(bad());
            }
        }
        for rule in &charter.rules {
            validate_rule_shape(rule)?;
        }
        if self.office_support.keys().any(|slot| !slots.contains(slot))
            || self
                .office_support
                .values()
                .any(|people| people.iter().any(|id| id.0 == 0))
            || self.grants.keys().any(|id| !self.concepts.contains(id))
            || self
                .grants
                .values()
                .any(|people| people.iter().any(|id| id.0 == 0))
            || self.applied.values().any(|people| {
                people
                    .iter()
                    .any(|(actor, agreement)| actor.0 == 0 || agreement.0 == 0)
            })
            || self.funding.iter().any(|id| id.0 == 0)
        {
            return Err(bad());
        }
        Ok(())
    }
}
impl SocialCaseView {
    pub fn validate_shape(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.subject.0 == 0
            || self.counterpart.0 == 0
            || self.area.0 == 0
            || self.need_band > 3
            || self.inventory.is_some_and(|id| id.0 == 0)
        {
            return Err(bad());
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn validate_shape(&self) -> Result<(), Error> {
        if self.cases.iter().any(|id| id.0 == 0)
            || self
                .values
                .iter()
                .any(|(id, value)| id.0 == 0 || !(-2_000..=2_000).contains(&value.weight))
            || self
                .institutions
                .iter()
                .any(|(id, view)| id.0 == 0 || *id != view.id)
        {
            return Err(bad());
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Institution>("society/institution", Institution::validate_shape)?;
    schema.register::<RuleProposal>("society/proposal", |row| {
        if row.id.0 > 0
            && row.institution.0 > 0
            && row.proposer.0 > 0
            && row.votes.keys().all(|id| id.0 > 0)
        {
            Ok(())
        } else {
            Err(bad())
        }
    })?;
    schema.register::<Concept>("society/concept", |row| validate_rule_shape(&row.rule))?;
    schema.register::<SocialCaseView>("society/case", SocialCaseView::validate_shape)?;
    schema.register::<Knowledge>("society/knowledge", Knowledge::validate_shape)?;
    schema.register::<Requests>("society/requests", |_| Ok(()))?;
    schema.register::<Exclusions>("society/exclusions", |row| {
        if row
            .stores
            .iter()
            .all(|(inventory, case)| inventory.0 > 0 && case.0 > 0)
        {
            Ok(())
        } else {
            Err(bad())
        }
    })?;
    schema.register::<Hearing>("society/hearing", |_| Ok(()))
}
macro_rules! ports {
    ($row:ty,$key:ident,$get:ident,$mut:ident,$iter:ident,$insert:ident,$remove:ident,$same:expr) => {
        pub fn $get(store: &Store, id: impl Borrow<$key>) -> Option<&$row> {
            store.get(StableId(id.borrow().0))
        }
        pub fn $mut(store: &mut Store, id: impl Borrow<$key>) -> Option<&mut $row> {
            store.get_mut(StableId(id.borrow().0))
        }
        pub fn $iter(store: &Store) -> impl DoubleEndedIterator<Item = ($key, &$row)> {
            store
                .iter::<$row>()
                .map(|(id, row)| ($key(id.0), row))
                .collect::<Vec<_>>()
                .into_iter()
        }
        pub fn $insert(store: &mut Store, id: $key, row: $row) -> Result<(), Error> {
            if id.0 == 0 || !($same)(id, &row) {
                return Err(bad());
            }
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $key) -> Result<Option<$row>, Error> {
            store.remove(StableId(id.0))
        }
    };
}
ports!(
    Institution,
    InstitutionId,
    institution,
    institution_mut,
    institutions,
    insert_institution,
    remove_institution,
    |id, row: &Institution| id == row.id
);
ports!(
    RuleProposal,
    RuleProposalId,
    proposal,
    proposal_mut,
    proposals,
    insert_proposal,
    remove_proposal,
    |id, row: &RuleProposal| id == row.id
);
ports!(
    Concept,
    ConceptId,
    concept_row,
    concept_row_mut,
    concept_rows,
    insert_concept,
    remove_concept,
    |_, _: &Concept| true
);
ports!(
    SocialCaseView,
    CaseId,
    case,
    case_mut,
    cases,
    insert_case,
    remove_case,
    |id, row: &SocialCaseView| id == row.id
);
ports!(
    Knowledge,
    PersonId,
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge,
    remove_knowledge,
    |_, _: &Knowledge| true
);
ports!(
    Requests,
    PersonId,
    requests,
    requests_mut,
    request_rows,
    insert_requests,
    remove_requests,
    |_, _: &Requests| true
);
ports!(
    Exclusions,
    PersonId,
    exclusions,
    exclusions_mut,
    exclusion_rows,
    insert_exclusions,
    remove_exclusions,
    |_, _: &Exclusions| true
);
ports!(
    Hearing,
    CaseId,
    hearing,
    hearing_mut,
    hearings,
    insert_hearing,
    remove_hearing,
    |_, _: &Hearing| true
);

pub fn concept(store: &Store, id: impl Borrow<ConceptId>) -> Option<&InstitutionalRule> {
    concept_row(store, id).map(|row| &row.rule)
}
pub fn concepts(store: &Store) -> impl DoubleEndedIterator<Item = (ConceptId, &InstitutionalRule)> {
    concept_rows(store).map(|(id, row)| (id, &row.rule))
}

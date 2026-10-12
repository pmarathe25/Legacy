//! Reference validation belongs to Society; foreign owners supply raw identity facts.
use crate::institutions::*;
use crate::storage::*;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;
pub struct ValidationFacts<'a> {
    pub at: SimTime,
    pub next_id: u64,
    pub next_event: u64,
    pub areas: &'a BTreeSet<AreaId>,
    pub references: &'a ReferenceFacts,
}
fn fail() -> Error {
    Error::InvalidSave("invalid institution state".into())
}
fn expression_references(e: &RuleExpression, refs: &ReferenceFacts) -> bool {
    match e {
        RuleExpression::All(xs) | RuleExpression::Any(xs) => {
            xs.iter().all(|x| expression_references(x, refs))
        }
        RuleExpression::Not(x) => expression_references(x, refs),
        RuleExpression::Fact(
            SocialCondition::KnownStockAtLeast { inventory, .. }
            | SocialCondition::Excluded { inventory },
        ) => refs.stocks.contains_key(inventory),
        _ => true,
    }
}
fn rule_references(r: &InstitutionalRule, refs: &ReferenceFacts) -> bool {
    let beneficiary = |b: &Beneficiary| match b {
        Beneficiary::Person(id) => refs.people.contains(id),
        Beneficiary::Office(_) => true,
    };
    validate_rule_shape(r).is_ok()
        && r.subjects.named.is_subset(&refs.people)
        && expression_references(&r.when, refs)
        && match &r.effect {
            SocialEffect::Access { inventory, .. } => refs.stocks.contains_key(inventory),
            SocialEffect::Labor {
                site,
                beneficiary: b,
                ..
            } => refs.sites.contains_key(site) && beneficiary(b),
            SocialEffect::Delivery {
                source,
                destination,
                quantity,
                beneficiary: b,
                ..
            } => {
                refs.stocks
                    .get(source)
                    .zip(refs.stocks.get(destination))
                    .is_some_and(|(a, b)| a.good == b.good && *quantity <= b.capacity)
                    && beneficiary(b)
            }
        }
}
impl Runtime {
    pub fn validation_references(&self, store: &Store) -> ReferenceRequest {
        let mut request = ReferenceRequest::default();
        for (_, rule) in concepts(store) {
            request.rule(rule);
        }
        for (_, i) in institutions(store) {
            request.charter(&i.charter);
            request.people.extend(&i.members);
            request.people.extend(&i.invited);
            request.people.extend(&i.declined);
        }
        for (_, p) in proposals(store) {
            request.people.insert(p.proposer);
            request.people.extend(p.votes.keys());
            request.change(&p.change);
        }
        for (_, c) in cases(store) {
            request.people.extend([c.subject, c.counterpart]);
            request.stocks.extend(c.inventory);
        }
        for (actor, k) in knowledges(store) {
            request.people.insert(actor);
            for v in k.institutions.values() {
                request.people.extend(&v.members);
                request.people.extend(&v.charter.founders);
            }
        }
        for (actor, _) in request_rows(store) {
            request.people.insert(actor);
        }
        for (actor, _) in exclusion_rows(store) {
            request.people.insert(actor);
        }
        request
    }
    pub fn validate(&self, store: &Store, f: &ValidationFacts<'_>) -> Result<(), Error> {
        let refs = f.references;
        if self.last_event >= f.next_event {
            return Err(fail());
        }
        for (id, r) in concepts(store) {
            if id.0 >= f.next_id || !rule_references(r, refs) {
                return Err(fail());
            }
        }
        for (id, i) in institutions(store) {
            i.validate_shape().map_err(|_| fail())?;
            let c = &i.charter;
            if id != i.id
                || id.0 >= f.next_id
                || i.origin
                    .is_some_and(|origin| institution(store, origin).is_none() || origin == id)
                || !f.areas.contains(&c.jurisdiction)
                || !c.founders.is_subset(&refs.people)
                || !i.members.is_subset(&refs.people)
                || !i.declined.is_subset(&c.founders)
                || !i.invited.is_subset(&refs.people)
                || c.evidence.iter().any(|id| case(store, id).is_none())
                || !refs.validate_subjects(&c.procedure.proposers, &c.offices)
                || !refs.validate_subjects(&c.procedure.voters, &c.offices)
            {
                return Err(fail());
            }
            let slots = c.offices.iter().map(|o| o.slot).collect::<BTreeSet<_>>();
            for office in &c.offices {
                if !refs.people.contains(&office.holder)
                    || office
                        .inventories
                        .iter()
                        .any(|id| !refs.stocks.contains_key(id))
                    || office.sites.iter().any(|id| !refs.sites.contains_key(id))
                {
                    return Err(fail());
                }
            }
            for (rule, id) in c.rules.iter().zip(&i.concepts) {
                let office = c
                    .offices
                    .iter()
                    .find(|o| o.slot == rule.authority)
                    .ok_or_else(fail)?;
                if concept(store, id) != Some(rule)
                    || !refs.validate_subjects(&rule.subjects, &c.offices)
                {
                    return Err(fail());
                }
                let authorized = match &rule.effect {
                    SocialEffect::Access { inventory, .. } => {
                        office.powers.contains(&OfficePower::Access)
                            && office.inventories.contains(inventory)
                    }
                    SocialEffect::Labor { site, .. } => {
                        office.powers.contains(&OfficePower::Labor) && office.sites.contains(site)
                    }
                    SocialEffect::Delivery {
                        source,
                        destination,
                        ..
                    } => {
                        office.powers.contains(&OfficePower::Delivery)
                            && office.inventories.contains(source)
                            && office.inventories.contains(destination)
                    }
                };
                if !authorized {
                    return Err(fail());
                }
            }
            if i.office_support
                .iter()
                .any(|(slot, ids)| !slots.contains(slot) || !ids.is_subset(&i.members))
                || i.grants.iter().any(|(id, actors)| {
                    !i.concepts.contains(id)
                        || !actors.is_subset(&refs.people)
                        || !concept(store, id).is_some_and(|r| {
                            matches!(r.effect, SocialEffect::Access { allowed: true, .. })
                        })
                })
                || i.applied.iter().any(|(id, actors)| {
                    concept(store, id).is_none()
                        || actors.iter().any(|(actor, id)| {
                            !refs.people.contains(actor)
                                || !game_agreements::agreement(store, *id).is_some()
                        })
                })
                || i.funding
                    .iter()
                    .any(|id| !game_agreements::agreement(store, *id).is_some())
            {
                return Err(fail());
            }
            if matches!(
                i.status,
                InstitutionStatus::Proposed | InstitutionStatus::Rejected
            ) && (!i.members.is_subset(&c.founders)
                || !i.funding.is_empty()
                || !i.applied.is_empty()
                || !i.grants.is_empty())
            {
                return Err(fail());
            }
            if matches!(
                i.status,
                InstitutionStatus::Active | InstitutionStatus::Dissolved
            ) && i.funding.len() != c.contributions.len()
            {
                return Err(fail());
            }
            for (term, id) in c.contributions.iter().zip(&i.funding) {
                if game_agreements::agreement(store, *id).unwrap().terms != vec![term.clone()] {
                    return Err(fail());
                }
            }
        }
        for (id, p) in proposals(store) {
            let i = institution(store, p.institution).ok_or_else(fail)?;
            if id != p.id
                || id.0 >= f.next_id
                || !refs.people.contains(&p.proposer)
                || p.base_revision > i.revision
                || (p.status == RuleProposalStatus::Proposed && p.base_revision != i.revision)
                || p.votes.keys().any(|id| !refs.people.contains(id))
            {
                return Err(fail());
            }
            match &p.change {
                InstitutionChange::AddRule(r) | InstitutionChange::ReplaceRule { rule: r, .. }
                    if !rule_references(r, refs) =>
                {
                    return Err(fail());
                }
                InstitutionChange::Appoint { holder, .. } | InstitutionChange::Invite(holder)
                    if !refs.people.contains(holder) =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
        for (id, c) in cases(store) {
            if id != c.id
                || id.0 >= f.next_id
                || !refs.people.contains(&c.subject)
                || !refs.people.contains(&c.counterpart)
                || !f.areas.contains(&c.area)
                || c.inventory.is_some_and(|id| !refs.stocks.contains_key(&id))
                || c.need_band > 3
                || c.observed_at > f.at
            {
                return Err(fail());
            }
        }
        for (actor, row) in exclusion_rows(store) {
            if !refs.people.contains(&actor)
                || row.stores.iter().any(|(inventory, id)| {
                    case(store, id).is_none_or(|c| {
                        c.subject != actor
                            || c.inventory != Some(*inventory)
                            || c.kind != SocialCaseKind::Exclusion
                    })
                })
            {
                return Err(fail());
            }
        }
        if hearings(store).any(|(id, _)| case(store, id).is_none())
            || request_rows(store).any(|(actor, _)| !refs.people.contains(&actor))
        {
            return Err(fail());
        }
        for (actor, k) in knowledges(store) {
            if !refs.people.contains(&actor)
                || k.cases.iter().any(|id| case(store, id).is_none())
                || k.values.iter().any(|(id, v)| {
                    concept(store, id).is_none()
                        || !(-2000..=2000).contains(&v.weight)
                        || v.experiences == 0
                })
                || k.institutions.iter().any(|(id, v)| {
                    institution(store, id).is_none()
                        || *id != v.id
                        || v.observed_at > f.at
                        || !v.members.is_subset(&refs.people)
                        || !v.charter.founders.is_subset(&refs.people)
                        || v.proposals.iter().any(|p| proposal(store, p.id).is_none())
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}

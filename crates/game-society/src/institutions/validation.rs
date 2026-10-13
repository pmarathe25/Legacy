use super::*;
use game_ecs::Store;

/// Authoritative facts for referenced IDs only. They confer neither title nor
/// knowledge: the adapter obtains them to validate an attempted transition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceFacts {
    pub people: BTreeSet<PersonId>,
    pub eligible_locations: BTreeMap<PersonId, AreaId>,
    pub stocks: BTreeMap<InventoryId, StockReference>,
    pub sites: BTreeMap<SiteId, AreaId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StockReference {
    pub area: Option<AreaId>,
    pub custodian: Option<PersonId>,
    pub good: Good,
    pub capacity: Quantity,
}
impl ReferenceFacts {
    pub fn validate_subjects(&self, subjects: &Subjects, offices: &[OfficeMandate]) -> bool {
        (subjects.members || !subjects.named.is_empty() || !subjects.offices.is_empty())
            && subjects.named.len() <= 64
            && subjects.named.iter().all(|id| self.people.contains(id))
            && subjects.offices.len() <= 8
            && subjects
                .offices
                .iter()
                .all(|slot| offices.iter().any(|office| office.slot == *slot))
    }
    fn validate_expression(
        &self,
        expression: &RuleExpression,
        area: AreaId,
        depth: usize,
        nodes: &mut usize,
    ) -> bool {
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
                        .all(|item| self.validate_expression(item, area, depth + 1, nodes))
            }
            RuleExpression::Not(item) => self.validate_expression(item, area, depth + 1, nodes),
            RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
                inventory,
                quantity,
            }) => {
                quantity.0 > 0
                    && self
                        .stocks
                        .get(inventory)
                        .is_some_and(|stock| stock.area == Some(area))
            }
            RuleExpression::Fact(SocialCondition::Excluded { inventory }) => self
                .stocks
                .get(inventory)
                .is_some_and(|stock| stock.area == Some(area)),
            RuleExpression::Fact(
                SocialCondition::KnownCooperationAtLeast(n)
                | SocialCondition::KnownDefaultsAtLeast(n),
            ) => (1..=64).contains(n),
            RuleExpression::Fact(SocialCondition::HungerAtLeast(n)) => *n > 0,
            RuleExpression::Fact(_) => true,
        }
    }
    pub fn validate_rule(
        &self,
        charter: &InstitutionCharter,
        rule: &InstitutionalRule,
    ) -> Result<(), Error> {
        let office = charter
            .offices
            .iter()
            .find(|office| office.slot == rule.authority)
            .ok_or(Error::Unavailable)?;
        if !self.validate_subjects(&rule.subjects, &charter.offices)
            || !self.validate_expression(&rule.when, charter.jurisdiction, 0, &mut 0)
        {
            return Err(Error::Unavailable);
        }
        let beneficiary = |who: &Beneficiary| match who {
            Beneficiary::Person(id) => charter.founders.contains(id),
            Beneficiary::Office(slot) => charter.offices.iter().any(|office| office.slot == *slot),
        };
        let valid = match &rule.effect {
            SocialEffect::Access { inventory, .. } => {
                office.powers.contains(&OfficePower::Access)
                    && office.inventories.contains(inventory)
            }
            SocialEffect::Labor {
                site,
                millis,
                beneficiary: who,
                due_after,
            } => {
                office.powers.contains(&OfficePower::Labor)
                    && office.sites.contains(site)
                    && *millis > 0
                    && due_after.0 >= *millis
                    && beneficiary(who)
            }
            SocialEffect::Delivery {
                source,
                destination,
                quantity,
                beneficiary: who,
                due_after,
            } => {
                office.powers.contains(&OfficePower::Delivery)
                    && office.inventories.contains(source)
                    && office.inventories.contains(destination)
                    && source != destination
                    && quantity.0 > 0
                    && due_after.0 >= 120_000
                    && beneficiary(who)
                    && self
                        .stocks
                        .get(source)
                        .zip(self.stocks.get(destination))
                        .is_some_and(|(a, b)| a.good == b.good && *quantity <= b.capacity)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    fn validate_procedure(
        &self,
        procedure: &DecisionProcedure,
        charter: &InstitutionCharter,
    ) -> bool {
        self.validate_subjects(&procedure.proposers, &charter.offices)
            && self.validate_subjects(&procedure.voters, &charter.offices)
            && procedure.minimum_support > 0
            && procedure.minimum_support <= 64
    }
    pub fn validate_charter(
        &self,
        store: &Store,
        charter: &InstitutionCharter,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
    ) -> Result<(), Error> {
        if charter.jurisdiction != area
            || charter.purpose.is_empty()
            || charter.purpose.len() > 160
            || !(2..=8).contains(&charter.founders.len())
            || !charter.founders.contains(&actor)
            || charter
                .founders
                .iter()
                .any(|person| self.eligible_locations.get(person) != Some(&area))
            || charter.offices.is_empty()
            || charter.offices.len() > 8
            || charter.rules.len() > 16
            || charter.contributions.len() > 16
            || charter.evidence.len() > 8
            || !crate::storage::knowledge(store, actor)
                .is_some_and(|memory| charter.evidence.is_subset(&memory.cases))
            || !self.validate_procedure(&charter.procedure, charter)
        {
            return Err(Error::Unavailable);
        }
        let mut slots = BTreeSet::new();
        for office in &charter.offices {
            if !slots.insert(office.slot)
                || !charter.founders.contains(&office.holder)
                || office.minimum_support == 0
                || usize::from(office.minimum_support) > charter.founders.len()
                || office.inventories.len() > 16
                || office.sites.len() > 16
                || office.powers.is_empty()
                || office.inventories.iter().any(|id| {
                    self.stocks.get(id).is_none_or(|stock| {
                        stock.area != Some(area)
                            || stock
                                .custodian
                                .is_none_or(|person| !charter.founders.contains(&person))
                    })
                })
                || office
                    .sites
                    .iter()
                    .any(|site| self.sites.get(site) != Some(&area))
            {
                return Err(Error::Unavailable);
            }
        }
        for rule in &charter.rules {
            self.validate_rule(charter, rule)?;
        }
        for term in &charter.contributions {
            if term.amount() == 0
                || term.parties()[0] == term.parties()[1]
                || term
                    .parties()
                    .iter()
                    .any(|person| !charter.founders.contains(person))
                || term.deadline().is_some_and(|deadline| deadline <= now)
            {
                return Err(Error::Unavailable);
            }
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    quantity,
                    ..
                } => {
                    if source == destination
                        || !self
                            .stocks
                            .get(source)
                            .zip(self.stocks.get(destination))
                            .is_some_and(|(a, b)| a.good == b.good && *quantity <= b.capacity)
                    {
                        return Err(Error::Unavailable);
                    }
                }
                AgreementTerm::Labor { site, .. } => {
                    if !self.sites.contains_key(site) {
                        return Err(Error::Unavailable);
                    }
                }
                _ => return Err(Error::Unavailable),
            }
        }
        Ok(())
    }
    pub fn validate_change(
        &self,
        institution: &Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        match change {
            InstitutionChange::AddRule(rule) => {
                if institution.charter.rules.len() >= 16 {
                    return Err(Error::Unavailable);
                }
                self.validate_rule(&institution.charter, rule)
            }
            InstitutionChange::ReplaceRule { index, rule } => {
                if *index as usize >= institution.charter.rules.len() {
                    return Err(Error::Unavailable);
                }
                self.validate_rule(&institution.charter, rule)
            }
            InstitutionChange::RemoveRule { index }
                if (*index as usize) < institution.charter.rules.len() =>
            {
                Ok(())
            }
            InstitutionChange::Procedure(procedure)
                if self.validate_procedure(procedure, &institution.charter) =>
            {
                Ok(())
            }
            InstitutionChange::Appoint { office, holder }
                if institution
                    .charter
                    .offices
                    .iter()
                    .any(|mandate| mandate.slot == *office)
                    && institution.members.contains(holder) =>
            {
                Ok(())
            }
            InstitutionChange::Invite(person) if self.people.contains(person) => Ok(()),
            InstitutionChange::Dissolve => Ok(()),
            InstitutionChange::Adjudicate {
                remedy:
                    EnforcementRemedy::Depose {
                        office,
                        replacement,
                    },
                ..
            } => {
                if !institution.members.contains(replacement)
                    || !institution
                        .charter
                        .offices
                        .iter()
                        .any(|mandate| mandate.slot == *office)
                {
                    return Err(Error::Unavailable);
                }
                Ok(())
            }
            // The external owner supplies a matching permit before command preparation.
            InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. }
            | InstitutionChange::EnforcementPolicy { .. }
            | InstitutionChange::Adjudicate { .. } => Ok(()),
            _ => Err(Error::Unavailable),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceRequest {
    pub people: BTreeSet<PersonId>,
    pub stocks: BTreeSet<InventoryId>,
    pub sites: BTreeSet<SiteId>,
}
impl ReferenceRequest {
    pub fn subjects(&mut self, subjects: &Subjects) {
        self.people.extend(&subjects.named);
    }
    pub fn rule(&mut self, rule: &InstitutionalRule) {
        self.subjects(&rule.subjects);
        self.expression(&rule.when);
        match &rule.effect {
            SocialEffect::Access { inventory, .. } => {
                self.stocks.insert(*inventory);
            }
            SocialEffect::Labor {
                site, beneficiary, ..
            } => {
                self.sites.insert(*site);
                self.beneficiary(beneficiary);
            }
            SocialEffect::Delivery {
                source,
                destination,
                beneficiary,
                ..
            } => {
                self.stocks.extend([*source, *destination]);
                self.beneficiary(beneficiary);
            }
        }
    }
    fn expression(&mut self, expression: &RuleExpression) {
        match expression {
            RuleExpression::All(items) | RuleExpression::Any(items) => {
                for item in items {
                    self.expression(item);
                }
            }
            RuleExpression::Not(item) => self.expression(item),
            RuleExpression::Fact(
                SocialCondition::KnownStockAtLeast { inventory, .. }
                | SocialCondition::Excluded { inventory },
            ) => {
                self.stocks.insert(*inventory);
            }
            _ => {}
        }
    }
    fn beneficiary(&mut self, who: &Beneficiary) {
        if let Beneficiary::Person(person) = who {
            self.people.insert(*person);
        }
    }
    pub fn charter(&mut self, charter: &InstitutionCharter) {
        self.people.extend(&charter.founders);
        self.subjects(&charter.procedure.proposers);
        self.subjects(&charter.procedure.voters);
        for office in &charter.offices {
            self.people.insert(office.holder);
            self.stocks.extend(&office.inventories);
            self.sites.extend(&office.sites);
        }
        for rule in &charter.rules {
            self.rule(rule);
        }
        for term in &charter.contributions {
            self.people.extend(term.parties());
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    ..
                } => {
                    self.stocks.extend([*source, *destination]);
                }
                AgreementTerm::Labor { site, .. } => {
                    self.sites.insert(*site);
                }
                _ => {}
            }
        }
    }
    pub fn change(&mut self, change: &InstitutionChange) {
        match change {
            InstitutionChange::AddRule(rule) | InstitutionChange::ReplaceRule { rule, .. } => {
                self.rule(rule)
            }
            InstitutionChange::Procedure(procedure) => {
                self.subjects(&procedure.proposers);
                self.subjects(&procedure.voters);
            }
            InstitutionChange::Appoint { holder, .. } | InstitutionChange::Invite(holder) => {
                self.people.insert(*holder);
            }
            _ => {}
        }
    }
}
impl Runtime {
    pub fn command_references(
        &self,
        store: &Store,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<ReferenceRequest, Error> {
        let mut request = ReferenceRequest::default();
        request.people.insert(actor);
        let institution = match action {
            InstitutionAction::Found { charter, .. } => {
                request.charter(charter);
                None
            }
            InstitutionAction::Join { institution }
            | InstitutionAction::Decline { institution }
            | InstitutionAction::Leave { institution }
            | InstitutionAction::SupportOffice { institution, .. } => Some(*institution),
            InstitutionAction::Propose {
                institution,
                change,
            } => {
                request.change(change);
                Some(*institution)
            }
            InstitutionAction::Vote { proposal, .. } => {
                let proposal =
                    crate::storage::proposal(store, proposal).ok_or(Error::Unavailable)?;
                request.change(&proposal.change);
                Some(proposal.institution)
            }
            InstitutionAction::HearCase {
                institution, case, ..
            } => {
                let case = crate::storage::case(store, case).ok_or(Error::Unavailable)?;
                request.people.extend([case.subject, case.counterpart]);
                Some(*institution)
            }
            InstitutionAction::TellCase { recipient, case } => {
                let case = crate::storage::case(store, case).ok_or(Error::Unavailable)?;
                request
                    .people
                    .extend([*recipient, case.subject, case.counterpart]);
                None
            }
        };
        if let Some(id) = institution {
            let institution = crate::storage::institution(store, id).ok_or(Error::Unavailable)?;
            request.charter(&institution.charter);
            request.people.extend(&institution.members);
        }
        Ok(request)
    }
}

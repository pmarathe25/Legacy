//! Symbolic starting commitments, resolved to the existing owners' native types.
use crate::*;
use game_agreements::AgreementTerm;
use game_property::ClaimRight;
use game_society::institutions::{
    DecisionProcedure, InstitutionCharter, InstitutionalRule, OfficeMandate, OfficePower,
    RuleExpression, SocialCondition, SocialEffect, Subjects,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomSubjects {
    pub members: bool,
    #[serde(default)]
    pub residents: Vec<usize>,
    #[serde(default)]
    pub offices: BTreeSet<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProcedure {
    pub proposers: CustomSubjects,
    pub voters: CustomSubjects,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomOffice {
    pub slot: u16,
    pub holder: usize,
    pub inventories: Vec<String>,
    pub powers: BTreeSet<OfficePower>,
    pub minimum_support: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomCondition {
    Local,
    Member,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomAccessRule {
    pub subjects: CustomSubjects,
    pub condition: CustomCondition,
    pub inventory: String,
    pub allowed: bool,
    pub authority: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomDelivery {
    pub debtor: usize,
    pub creditor: usize,
    pub source: String,
    pub destination: String,
    pub quantity: Quantity,
    pub deadline_after: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomFund {
    pub key: String,
    pub good: Good,
    pub area: String,
    pub quantity: Quantity,
    pub capacity: Quantity,
    pub access: Access,
    pub custodian: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartingCustom {
    pub source: String,
    pub source_access: Access,
    pub claimant: usize,
    pub claim_rights: BTreeSet<ClaimRight>,
    pub fund: CustomFund,
    pub founders: Vec<usize>,
    pub join_remaining: bool,
    pub purpose: String,
    pub jurisdiction: String,
    pub open_membership: bool,
    pub procedure: CustomProcedure,
    pub offices: Vec<CustomOffice>,
    pub access_rules: Vec<CustomAccessRule>,
    pub contributions: Vec<CustomDelivery>,
    pub fulfilled_contribution: Option<usize>,
    pub fulfil_step: SimDuration,
}
pub struct ResolvedCustom {
    pub source: InventoryId,
    pub source_access: BTreeSet<PersonId>,
    pub claimant: PersonId,
    pub claim_rights: BTreeSet<ClaimRight>,
    pub founders: BTreeSet<PersonId>,
    pub join_remaining: bool,
    pub charter: InstitutionCharter,
    pub initial_delivery: Option<AgreementTerm>,
    pub fulfil_step: SimDuration,
}
impl CustomSubjects {
    fn validate(&self, residents: usize, slots: &BTreeSet<u16>) -> Result<(), Error> {
        if (!self.members && self.residents.is_empty() && self.offices.is_empty())
            || self.residents.iter().any(|i| *i >= residents)
            || self.residents.iter().collect::<BTreeSet<_>>().len() != self.residents.len()
            || !self.offices.is_subset(slots)
        {
            return Err(invalid("invalid starting custom subjects"));
        }
        Ok(())
    }
    fn resolve(&self, residents: &[PersonId]) -> Result<Subjects, Error> {
        Ok(Subjects {
            members: self.members,
            named: self
                .residents
                .iter()
                .map(|i| residents.get(*i).copied().ok_or(Error::Unavailable))
                .collect::<Result<_, _>>()?,
            offices: self.offices.clone(),
        })
    }
}
impl StartingCustom {
    pub fn validate(&self, settlement: &SettlementDefinition) -> Result<(), Error> {
        let residents = settlement.minimum_residents;
        let stock = |key: &str| settlement.stocks.iter().find(|stock| stock.key == key);
        let area = |key: &str| settlement.areas.iter().any(|area| area.key == key);
        if self.purpose.trim().is_empty()
            || self.purpose.len() > 160
            || !area(&self.jurisdiction)
            || !(2..=8).contains(&self.founders.len())
            || self.founders.iter().collect::<BTreeSet<_>>().len() != self.founders.len()
            || self.founders.iter().any(|i| *i >= residents)
            || !self.founders.contains(&self.claimant)
            || self.claim_rights.is_empty()
            || stock(&self.source).is_none()
            || !valid_key(&self.fund.key)
            || stock(&self.fund.key).is_some()
            || !area(&self.fund.area)
            || self.fund.area != self.jurisdiction
            || stock(&self.source).is_none_or(|source| !matches!(&source.placement, Placement::Area { area } if *area == self.jurisdiction))
            || self.fund.custodian >= residents
            || self.fund.quantity > self.fund.capacity
            || self.fund.capacity.0 == 0
        {
            return Err(invalid("invalid starting custom references"));
        }
        check_access(&self.source_access, residents, None)?;
        check_access(&self.fund.access, residents, None)?;
        let inventory = |key: &str| key == self.fund.key || stock(key).is_some();
        let good = |key: &str| {
            if key == self.fund.key {
                Some(self.fund.good)
            } else {
                stock(key).map(|stock| stock.good)
            }
        };
        let capacity = |key: &str| {
            if key == self.fund.key {
                Some(self.fund.capacity)
            } else {
                stock(key).map(|stock| stock.capacity)
            }
        };
        let slots = self
            .offices
            .iter()
            .map(|office| office.slot)
            .collect::<BTreeSet<_>>();
        if !(1..=8).contains(&self.offices.len())
            || slots.len() != self.offices.len()
            || self.procedure.minimum_support == 0
            || self.procedure.minimum_support > 64
            || self.access_rules.len() > 16
            || self.contributions.len() > 16
        {
            return Err(invalid("invalid starting custom charter bounds"));
        }
        self.procedure.proposers.validate(residents, &slots)?;
        self.procedure.voters.validate(residents, &slots)?;
        for office in &self.offices {
            if !self.founders.contains(&office.holder)
                || office.inventories.is_empty()
                || office.inventories.len() > 16
                || office.inventories.iter().collect::<BTreeSet<_>>().len()
                    != office.inventories.len()
                || office.inventories.iter().any(|key| !inventory(key))
                || office.powers.is_empty()
                || office.minimum_support == 0
                || usize::from(office.minimum_support) > self.founders.len()
            {
                return Err(invalid("invalid starting custom office"));
            }
        }
        for rule in &self.access_rules {
            rule.subjects.validate(residents, &slots)?;
            if !inventory(&rule.inventory) || !slots.contains(&rule.authority) {
                return Err(invalid("invalid starting custom access rule"));
            }
        }
        for term in &self.contributions {
            if term.debtor == term.creditor
                || !self.founders.contains(&term.debtor)
                || !self.founders.contains(&term.creditor)
                || term.source == term.destination
                || good(&term.source).is_none()
                || good(&term.source) != good(&term.destination)
                || term.quantity.0 == 0
                || capacity(&term.destination).is_none_or(|cap| term.quantity > cap)
                || term.deadline_after.0 == 0
            {
                return Err(invalid("invalid starting custom contribution"));
            }
        }
        if let Some(index) = self.fulfilled_contribution {
            let term = self
                .contributions
                .get(index)
                .ok_or_else(|| invalid("unknown initial custom contribution"))?;
            if self.fulfil_step.0 == 0 || good(&term.source) != Some(Good::Food) {
                return Err(invalid("invalid starting custom fulfillment"));
            }
        }
        Ok(())
    }
    pub fn resolve(
        &self,
        residents: &[PersonId],
        areas: &BTreeMap<String, AreaId>,
        stocks: &BTreeMap<String, InventoryId>,
        fund: InventoryId,
        now: SimTime,
    ) -> Result<ResolvedCustom, Error> {
        let person = |index: usize| residents.get(index).copied().ok_or(Error::Unavailable);
        let inventory = |key: &str| {
            if key == self.fund.key {
                Ok(fund)
            } else {
                stocks.get(key).copied().ok_or(Error::Unavailable)
            }
        };
        let founders = self
            .founders
            .iter()
            .map(|i| person(*i))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let contributions = self
            .contributions
            .iter()
            .map(|term| {
                Ok(AgreementTerm::Delivery {
                    debtor: person(term.debtor)?,
                    creditor: person(term.creditor)?,
                    source: inventory(&term.source)?,
                    destination: inventory(&term.destination)?,
                    quantity: term.quantity,
                    deadline: SimTime(
                        now.0
                            .checked_add(term.deadline_after.0)
                            .ok_or(Error::Overflow)?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let initial_delivery = self
            .fulfilled_contribution
            .map(|index| contributions.get(index).cloned().ok_or(Error::Unavailable))
            .transpose()?;
        Ok(ResolvedCustom {
            source: inventory(&self.source)?,
            source_access: self.source_access.resolve(residents, None)?,
            claimant: person(self.claimant)?,
            claim_rights: self.claim_rights.clone(),
            founders: founders.clone(),
            join_remaining: self.join_remaining,
            initial_delivery,
            fulfil_step: self.fulfil_step,
            charter: InstitutionCharter {
                purpose: self.purpose.clone(),
                jurisdiction: *areas.get(&self.jurisdiction).ok_or(Error::Unavailable)?,
                founders,
                open_membership: self.open_membership,
                procedure: DecisionProcedure {
                    proposers: self.procedure.proposers.resolve(residents)?,
                    voters: self.procedure.voters.resolve(residents)?,
                    minimum_support: self.procedure.minimum_support,
                },
                offices: self
                    .offices
                    .iter()
                    .map(|office| {
                        Ok(OfficeMandate {
                            slot: office.slot,
                            holder: person(office.holder)?,
                            inventories: office
                                .inventories
                                .iter()
                                .map(|key| inventory(key))
                                .collect::<Result<_, _>>()?,
                            sites: BTreeSet::new(),
                            powers: office.powers.clone(),
                            minimum_support: office.minimum_support,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                rules: self
                    .access_rules
                    .iter()
                    .map(|rule| {
                        Ok(InstitutionalRule {
                            subjects: rule.subjects.resolve(residents)?,
                            when: RuleExpression::Fact(match rule.condition {
                                CustomCondition::Local => SocialCondition::Local,
                                CustomCondition::Member => SocialCondition::Member,
                            }),
                            effect: SocialEffect::Access {
                                inventory: inventory(&rule.inventory)?,
                                allowed: rule.allowed,
                            },
                            authority: rule.authority,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                contributions,
                evidence: BTreeSet::new(),
            },
        })
    }
}

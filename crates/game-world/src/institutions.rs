//! Institutions are supported arrangements of people and scoped authority.
//! Their rules create permissions and recognized duties, never physical payment.
use crate::*;
use game_society::institutions::{
    CommandEffect, CommandFacts, CommandIds, ExternalChangePermit, ReferenceFacts,
    ReferenceRequest, StockReference,
};
pub use game_society::institutions::{Institution, RuleProposal, SocialMemory, World};

impl Simulation {
    pub fn institution_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::agreement_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "institution-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let owner = sim.state.people[3].id;
        let area = sim.state.areas[0].id;
        for inventory in sim
            .state
            .inventories
            .values_mut()
            .filter(|i| i.location == InventoryLocation::Area(area) && i.good == Good::Food)
        {
            inventory.access = BTreeSet::from([owner]);
            sim.state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(inventory.id, owner);
        }
        sim.debug_set_personality(
            owner,
            decisions::Personality {
                empathy: 900,
                ..decisions::Personality::industrious()
            },
            decisions::Aspiration::SupportOthers,
        )?;
        sim.state.institutions = Some(World {
            institutions: BTreeMap::new(),
            proposals: BTreeMap::new(),
            concepts: BTreeMap::new(),
            cases: BTreeMap::new(),
            memories: sim
                .state
                .people
                .iter()
                .map(|p| (p.id, SocialMemory::default()))
                .collect(),
            last_event: sim.state.next_event - 1,
            exclusions: BTreeMap::new(),
            heard: BTreeMap::new(),
        });
        Ok(sim)
    }

    fn social_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let p = self
            .person_index
            .get(&actor)
            .map(|i| &self.state.people[*i])
            .ok_or(Error::Unavailable)?;
        if !self.independent(actor)
            || self.combat_restricts(actor)
            || !self
                .state
                .physical
                .as_ref()
                .is_some_and(|w| w.actors[&actor].vitals.alive)
        {
            return Err(Error::Unavailable);
        }
        match p.location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }

    pub fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView> {
        self.state
            .institutions
            .as_ref()
            .and_then(|w| w.memories.get(&actor))
            .map_or_else(Vec::new, |m| {
                m.institutions.values().rev().take(64).cloned().collect()
            })
    }
    pub fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView> {
        let Some(w) = &self.state.institutions else {
            return Vec::new();
        };
        w.memories.get(&actor).map_or_else(Vec::new, |m| {
            m.cases
                .iter()
                .rev()
                .take(64)
                .filter_map(|id| w.cases.get(id).cloned())
                .collect()
        })
    }
    pub fn debug_institution_action(
        &mut self,
        actor: PersonId,
        action: InstitutionAction,
    ) -> Result<(), Error> {
        self.institution_command(actor, &action)
    }
    pub fn debug_clear_social_learning(
        &mut self,
        actor: PersonId,
        values: bool,
        precedents: bool,
    ) -> Result<(), Error> {
        let m = self
            .state
            .institutions
            .as_mut()
            .and_then(|w| w.memories.get_mut(&actor))
            .ok_or(Error::Unavailable)?;
        if values {
            m.values.clear();
        }
        if precedents {
            m.cases.clear();
        }
        Ok(())
    }

    pub(crate) fn institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let area = self.social_area(actor)?;
        let w = self.state.institutions.as_ref().ok_or(Error::Unavailable)?;
        self.preflight_institutions()?;
        if !matches!(
            action,
            InstitutionAction::HearCase { .. } | InstitutionAction::TellCase { .. }
        ) {
            return self.execute_institution_transition(actor, area, action);
        }
        match action {
            InstitutionAction::HearCase {
                institution,
                office,
                case,
                share,
            } => {
                let i = w.institutions.get(institution).ok_or(Error::Unavailable)?;
                let mandate = i
                    .charter
                    .offices
                    .iter()
                    .find(|o| o.slot == *office)
                    .ok_or(Error::Unavailable)?;
                let c = w.cases.get(case).ok_or(Error::Unavailable)?.clone();
                let inventory = c.inventory.ok_or(Error::Unavailable)?;
                if i.status != InstitutionStatus::Active
                    || i.charter.jurisdiction != area
                    || !self.office_effective(i, mandate)
                    || mandate.holder != actor
                    || !mandate.powers.contains(&OfficePower::HearAccessCases)
                    || !mandate.inventories.contains(&inventory)
                    || !w.memories[&actor].cases.contains(case)
                    || c.kind != SocialCaseKind::Exclusion
                    || c.area != area
                    || w.heard.contains_key(case)
                    || !i.members.contains(&c.subject)
                {
                    return Err(Error::Unavailable);
                }
                let subject = c.subject;
                if *share {
                    let concept = i.concepts.iter().copied().find(|id| matches!(w.concepts[id].effect, SocialEffect::Access { inventory: target, allowed: true } if target == inventory) && w.concepts[id].authority == *office).ok_or(Error::Unavailable)?;
                    self.state
                        .institutions
                        .as_mut()
                        .unwrap()
                        .institutions
                        .get_mut(institution)
                        .unwrap()
                        .grants
                        .entry(concept)
                        .or_default()
                        .insert(subject);
                }
                self.state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .heard
                    .insert(*case, *share);
                if !*share {
                    let decision = self.record_social_case(
                        SocialCaseView {
                            id: CaseId(0),
                            outcome: SocialOutcome::Refused,
                            observed_at: self.state.time,
                            ..c
                        },
                        None,
                        false,
                    )?;
                    self.state
                        .institutions
                        .as_mut()
                        .unwrap()
                        .memories
                        .get_mut(&actor)
                        .unwrap()
                        .cases
                        .insert(decision);
                }
                self.publish_institution(*institution);
            }
            InstitutionAction::TellCase { recipient, case } => {
                if *recipient == actor
                    || self.social_area(*recipient)? != area
                    || !w.memories[&actor].cases.contains(case)
                {
                    return Err(Error::Unavailable);
                }
                self.state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .get_mut(recipient)
                    .ok_or(Error::Unavailable)?
                    .cases
                    .insert(*case);
            }
            _ => unreachable!("core institution command already dispatched"),
        }
        Ok(())
    }

    fn reserve_social_ids(&self, count: u64) -> Result<(), Error> {
        self.state
            .next_id
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        self.state
            .next_event
            .checked_add(count.checked_add(4).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn preflight_institutions(&self) -> Result<(), Error> {
        self.preflight_institutions_at(self.state.time)
    }
    pub(crate) fn preflight_institutions_at(&self, at: SimTime) -> Result<(), Error> {
        let Some(w) = &self.state.institutions else {
            return Ok(());
        };
        for rule in w
            .institutions
            .values()
            .filter(|i| i.status == InstitutionStatus::Active)
            .flat_map(|i| i.charter.rules.iter())
        {
            let due_after = match rule.effect {
                SocialEffect::Labor { due_after, .. }
                | SocialEffect::Delivery { due_after, .. } => Some(due_after),
                _ => None,
            };
            if let Some(duration) = due_after {
                at.0.checked_add(duration.0).ok_or(Error::Overflow)?;
            }
        }
        Ok(())
    }

    pub(crate) fn closing_institution_demand(
        &self,
        population: u64,
        review: bool,
        ledger: game_agreements::ClosingReceiptDemand,
        direct: u64,
    ) -> Result<game_society::institutions::InstitutionResourceDemand, Error> {
        let Some(world) = &self.state.institutions else {
            return Ok(Default::default());
        };
        use game_society::institutions::CaseReceipt;
        let mut receipts = Vec::new();
        for event in self.state.events.iter().skip(world.last_event as usize) {
            let receipt = match event.kind {
                EventKind::ObligationPerformed {
                    obligation, actor, ..
                } => match self.state.agreements.as_ref().unwrap().obligations[&obligation].term {
                    AgreementTerm::Labor { site, .. } => Some(CaseReceipt::Labor { actor, site }),
                    AgreementTerm::Delivery { destination, .. }
                        if self
                            .inventory_area(&self.state.inventories[&destination])
                            .is_some() =>
                    {
                        Some(CaseReceipt::Ledger)
                    }
                    _ => None,
                },
                EventKind::ObligationDefaulted { obligation } => {
                    let term =
                        &self.state.agreements.as_ref().unwrap().obligations[&obligation].term;
                    (matches!(
                        term,
                        AgreementTerm::Labor { .. } | AgreementTerm::Delivery { .. }
                    ) && matches!(
                        self.state.people[self.person_index[&term.parties()[0]]].location,
                        Location::AtArea(_)
                    ))
                    .then_some(CaseReceipt::Ledger)
                }
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    ..
                } if self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&output)
                    .is_some_and(|owner| *owner != actor) =>
                {
                    Some(CaseReceipt::Production { actor, site })
                }
                EventKind::Ate { actor, source, .. }
                    if world
                        .exclusions
                        .get(&actor)
                        .is_some_and(|stores| stores.contains_key(&source)) =>
                {
                    Some(CaseReceipt::Support)
                }
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    ..
                } if self.state.inventories[&source].good == Good::Food
                    && self.state.inventories[&destination].location
                        == InventoryLocation::Carried(actor)
                    && world
                        .exclusions
                        .get(&actor)
                        .is_some_and(|stores| stores.contains_key(&source)) =>
                {
                    Some(CaseReceipt::Support)
                }
                _ => None,
            };
            if let Some(receipt) = receipt {
                receipts.push(receipt);
            }
        }
        let pending = game_society::institutions::case_receipt_demand(&receipts)?;
        world.closing_resource_demand(population, review, ledger, pending, direct)
    }

    fn institution_authority_facts(
        &self,
        i: &Institution,
    ) -> game_society::institutions::AuthorityFacts {
        let needed = i
            .charter
            .offices
            .iter()
            .map(|o| o.holder)
            .chain(
                i.office_support
                    .values()
                    .flat_map(|people| people.iter().copied()),
            )
            .collect::<BTreeSet<_>>();
        game_society::institutions::AuthorityFacts {
            alive: needed
                .into_iter()
                .filter(|id| {
                    self.state
                        .physical
                        .as_ref()
                        .and_then(|w| w.actors.get(id))
                        .is_some_and(|body| body.vitals.alive)
                })
                .collect(),
        }
    }
    pub(crate) fn subjects(&self, i: &Institution, subjects: &Subjects) -> BTreeSet<PersonId> {
        i.subjects(subjects, &self.institution_authority_facts(i))
    }
    pub(crate) fn office_effective(&self, i: &Institution, office: &OfficeMandate) -> bool {
        i.office_effective(office, &self.institution_authority_facts(i))
    }
    pub(crate) fn institution_allows_access(
        &self,
        actor: PersonId,
        inventory: InventoryId,
    ) -> bool {
        let Some(world) = &self.state.institutions else {
            return false;
        };
        let authority = game_society::institutions::AuthorityFacts {
            alive: world
                .institutions
                .values()
                .flat_map(|i| self.institution_authority_facts(i).alive)
                .collect(),
        };
        world.allows_access(actor, inventory, &authority)
    }

    fn actual_institution_view(&self, i: &Institution) -> InstitutionView {
        let w = self.state.institutions.as_ref().unwrap();
        InstitutionView {
            id: i.id,
            origin: i.origin,
            charter: i.charter.clone(),
            members: i.members.clone(),
            declined: i.declined.clone(),
            status: i.status,
            offices: i
                .charter
                .offices
                .iter()
                .map(|o| OfficeView {
                    mandate: o.clone(),
                    supporters: i.office_support.get(&o.slot).cloned().unwrap_or_default(),
                    effective: self.office_effective(i, o),
                })
                .collect(),
            proposals: w
                .proposals
                .values()
                .filter(|p| p.institution == i.id)
                .rev()
                .take(32)
                .map(|p| RuleProposalView {
                    id: p.id,
                    proposer: p.proposer,
                    change: p.change.clone(),
                    votes: p.votes.clone(),
                    status: p.status,
                })
                .collect(),
            funding: i.funding.clone(),
            duties: i
                .applied
                .values()
                .flat_map(|by_actor| by_actor.values().copied())
                .collect(),
            observed_at: self.state.time,
        }
    }
    fn publish_institution(&mut self, id: InstitutionId) {
        let i = &self.state.institutions.as_ref().unwrap().institutions[&id];
        let area = i.charter.jurisdiction;
        let view = self.actual_institution_view(i);
        let public = i.charter.open_membership;
        let witnesses: Vec<_> = i
            .charter
            .founders
            .iter()
            .chain(i.members.iter())
            .chain(i.invited.iter())
            .copied()
            .chain(self.state.people.iter().filter(|_| public).map(|p| p.id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|actor| self.social_area(*actor).ok() == Some(area))
            .take(64)
            .collect();
        for actor in witnesses {
            self.state
                .institutions
                .as_mut()
                .unwrap()
                .memories
                .entry(actor)
                .or_default()
                .institutions
                .insert(id, view.clone());
        }
    }

    fn institution_reference_facts(&self, request: &ReferenceRequest) -> ReferenceFacts {
        ReferenceFacts {
            people: request
                .people
                .iter()
                .filter(|id| self.person_index.contains_key(id))
                .copied()
                .collect(),
            eligible_locations: request
                .people
                .iter()
                .filter_map(|id| self.social_area(*id).ok().map(|area| (*id, area)))
                .collect(),
            stocks: request
                .stocks
                .iter()
                .filter_map(|id| {
                    self.state.inventories.get(id).map(|stock| {
                        (
                            *id,
                            StockReference {
                                area: self.inventory_area(stock),
                                custodian: self
                                    .state
                                    .agreements
                                    .as_ref()
                                    .and_then(|world| world.custodians.get(id))
                                    .copied(),
                                good: stock.good,
                                capacity: stock.capacity,
                            },
                        )
                    })
                })
                .collect(),
            sites: request
                .sites
                .iter()
                .filter_map(|id| {
                    self.state
                        .physical
                        .as_ref()
                        .and_then(|world| world.sites.get(id))
                        .map(|site| (*id, site.area))
                })
                .collect(),
        }
    }
    pub(crate) fn validate_subjects(&self, subjects: &Subjects, offices: &[OfficeMandate]) -> bool {
        let request = ReferenceRequest {
            people: subjects.named.clone(),
            ..Default::default()
        };
        self.institution_reference_facts(&request)
            .validate_subjects(subjects, offices)
    }
    fn validate_rule(
        &self,
        charter: &InstitutionCharter,
        rule: &InstitutionalRule,
    ) -> Result<(), Error> {
        let mut request = ReferenceRequest::default();
        request.charter(charter);
        request.rule(rule);
        self.institution_reference_facts(&request)
            .validate_rule(charter, rule)
    }

    fn execute_institution_transition(
        &mut self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let world = self.state.institutions.as_ref().ok_or(Error::Unavailable)?;
        let request = world.command_references(actor, action)?;
        let references = self.institution_reference_facts(&request);
        let authority = game_society::institutions::AuthorityFacts {
            alive: request
                .people
                .iter()
                .filter(|id| {
                    self.state
                        .physical
                        .as_ref()
                        .and_then(|world| world.actors.get(id))
                        .is_some_and(|body| body.vitals.alive)
                })
                .copied()
                .collect(),
        };
        let external_context = match action {
            InstitutionAction::Propose {
                institution,
                change,
            } => Some((&world.institutions[institution], change, true)),
            InstitutionAction::Vote { proposal, .. } => {
                let proposal = &world.proposals[proposal];
                Some((
                    &world.institutions[&proposal.institution],
                    &proposal.change,
                    false,
                ))
            }
            _ => None,
        };
        let permit = if let Some((institution, change, proposing)) = external_context {
            if matches!(
                change,
                InstitutionChange::RatifyRegional { .. }
                    | InstitutionChange::AuthorizeRegionalPassage { .. }
                    | InstitutionChange::EnforcementPolicy { .. }
                    | InstitutionChange::Adjudicate { .. }
            ) {
                if !self.enforcement_change_actor_allowed(actor, institution, change, proposing)
                    || !self.realm_change_actor_allowed(actor, institution, change, proposing)
                {
                    return Err(Error::Unavailable);
                }
                match change {
                    InstitutionChange::RatifyRegional { .. }
                    | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                        self.validate_realm_change(institution, change)?
                    }
                    _ => self.validate_enforcement_change(institution, change)?,
                }
                Some(ExternalChangePermit {
                    actor,
                    institution: institution.id,
                    institution_revision: institution.revision,
                    change: change.clone(),
                })
            } else {
                None
            }
        } else {
            None
        };
        let plan = world.prepare_command(
            action,
            &CommandFacts {
                actor,
                area,
                now: self.state.time,
                references: &references,
                authority: &authority,
                external_permit: permit.as_ref(),
            },
            CommandIds {
                first: self.state.next_id,
            },
        )?;
        let external_ids = plan
            .effects
            .iter()
            .filter_map(|effect| match effect {
                CommandEffect::ExternalOutcome { change, .. } => {
                    Some(Self::enforcement_change_ids(change))
                }
                _ => None,
            })
            .try_fold(0_u64, |total, count| {
                total.checked_add(count).ok_or(Error::Overflow)
            })?;
        self.reserve_social_ids(
            plan.next_id
                .checked_sub(self.state.next_id)
                .and_then(|count| count.checked_add(external_ids))
                .ok_or(Error::Overflow)?,
        )?;
        let duties: Vec<_> = plan
            .effects
            .iter()
            .filter_map(|effect| match effect {
                CommandEffect::Funding {
                    institution,
                    agreement,
                    obligation,
                    term,
                } => Some(game_agreements::InstitutionDuty {
                    institution: *institution,
                    agreement: *agreement,
                    obligation: *obligation,
                    term: term.clone(),
                }),
                _ => None,
            })
            .collect();
        let funding = if duties.is_empty() {
            None
        } else {
            Some(
                self.state
                    .agreements
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .ledger
                    .prepare_institutional_duties(&duties, self.state.time)?,
            )
        };
        for effect in &plan.effects {
            if let CommandEffect::ExternalOutcome {
                institution,
                proposal,
                outcome,
                change,
            } = effect
            {
                match change {
                    InstitutionChange::RatifyRegional { .. }
                    | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                        self.preflight_realm_outcome(*institution, *proposal, *outcome)?
                    }
                    _ => self.preflight_enforcement_adoption(*institution, *proposal)?,
                }
            }
        }
        plan.verify(self.state.institutions.as_ref().unwrap())?;
        let effects = plan.effects.clone();
        let next_id = plan.next_id;
        if let Some(funding) = &funding {
            funding.check(&self.state.agreements.as_ref().unwrap().ledger)?;
        }
        plan.commit(self.state.institutions.as_mut().unwrap())?;
        if let Some(funding) = funding {
            funding.commit(&mut self.state.agreements.as_mut().unwrap().ledger)?;
        }
        self.state.next_id = next_id;
        for effect in effects {
            match effect {
                CommandEffect::Funding { agreement, .. } => {
                    self.emit(EventKind::AgreementActivated { agreement })
                }
                CommandEffect::Proposed {
                    institution,
                    proposer,
                } => self.emit(EventKind::InstitutionProposed {
                    institution,
                    proposer,
                }),
                CommandEffect::Founded { institution } => {
                    self.emit(EventKind::InstitutionFounded { institution })
                }
                CommandEffect::Left { institution, actor } => {
                    self.emit(EventKind::InstitutionLeft { institution, actor })
                }
                CommandEffect::OfficeSupportChanged {
                    institution,
                    office,
                    actor,
                    support,
                } => self.emit(EventKind::OfficeSupportChanged {
                    institution,
                    office,
                    actor,
                    support,
                }),
                CommandEffect::Changed {
                    institution,
                    proposal,
                } => self.emit(EventKind::InstitutionChanged {
                    institution,
                    proposal,
                }),
                CommandEffect::ExternalOutcome {
                    institution,
                    proposal,
                    outcome,
                    change,
                } => match change {
                    InstitutionChange::RatifyRegional { .. }
                    | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                        self.apply_realm_outcome(institution, proposal, outcome)?
                    }
                    _ => self.adopt_enforcement_change(institution, proposal)?,
                },
                CommandEffect::Publish { institution } => self.publish_institution(institution),
                CommandEffect::ObserveRegional => self.observe_realm()?,
            }
        }
        Ok(())
    }

    fn institutional_agreement(
        &mut self,
        institution: InstitutionId,
        term: AgreementTerm,
    ) -> Result<AgreementId, Error> {
        self.reserve_social_ids(2)?;
        let agreement = AgreementId(self.state.next_id);
        let obligation = ObligationId(self.state.next_id.checked_add(1).ok_or(Error::Overflow)?);
        let next_id = self.state.next_id.checked_add(2).ok_or(Error::Overflow)?;
        let plan = self
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?
            .ledger
            .prepare_institutional_duties(
                &[game_agreements::InstitutionDuty {
                    institution,
                    agreement,
                    obligation,
                    term,
                }],
                self.state.time,
            )?;
        plan.commit(&mut self.state.agreements.as_mut().unwrap().ledger)?;
        self.state.next_id = next_id;
        self.emit(EventKind::AgreementActivated { agreement });
        Ok(agreement)
    }

    fn known_case_count(
        &self,
        observer: PersonId,
        subject: PersonId,
        kind: SocialCaseKind,
    ) -> usize {
        self.state
            .institutions
            .as_ref()
            .unwrap()
            .known_case_count(observer, subject, kind)
    }
    fn apply_institution_rules(&mut self) -> Result<(), Error> {
        let ids = self
            .state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .values()
            .filter(|i| i.status == InstitutionStatus::Active)
            .map(|i| i.id)
            .collect::<Vec<_>>();
        for id in ids {
            let institution = self.state.institutions.as_ref().unwrap().institutions[&id].clone();
            let authority = self.institution_authority_facts(&institution);
            for (index, rule) in institution.charter.rules.iter().enumerate() {
                let office = institution
                    .charter
                    .offices
                    .iter()
                    .find(|o| o.slot == rule.authority)
                    .unwrap();
                if !institution.office_effective(office, &authority)
                    || self.social_area(office.holder).ok()
                        != Some(institution.charter.jurisdiction)
                {
                    continue;
                }
                for actor in institution.subjects(&rule.subjects, &authority) {
                    let input = game_society::institutions::ConditionInput {
                        observer: office.holder,
                        subject: actor,
                        subject_local: self.social_area(actor).ok()
                            == Some(institution.charter.jurisdiction),
                        own_hunger: self.state.physical.as_ref().unwrap().actors[&office.holder]
                            .vitals
                            .hunger,
                        known_stocks: &self.state.decisions.as_ref().unwrap().minds[&office.holder]
                            .knowledge,
                    };
                    let Some(intent) = self.state.institutions.as_ref().unwrap().rule_intent(
                        &institution,
                        index,
                        &input,
                        &authority,
                        true,
                        self.state.time,
                    ) else {
                        continue;
                    };
                    match &intent {
                        game_society::institutions::RuleIntent::Access { .. } => {
                            if self
                                .state
                                .institutions
                                .as_mut()
                                .unwrap()
                                .apply_access_intent(&intent)?
                            {
                                self.emit(EventKind::InstitutionRuleApplied {
                                    institution: id,
                                    rule: index as u16,
                                    actor,
                                    agreement: None,
                                });
                            }
                        }
                        game_society::institutions::RuleIntent::Duty { term, .. } => {
                            let agreement = self.institutional_agreement(id, term.clone())?;
                            self.state
                                .institutions
                                .as_mut()
                                .unwrap()
                                .register_duty(&intent, agreement)?;
                            self.emit(EventKind::InstitutionRuleApplied {
                                institution: id,
                                rule: index as u16,
                                actor,
                                agreement: Some(agreement),
                            });
                        }
                    }
                }
            }
            self.publish_institution(id);
        }
        Ok(())
    }

    fn record_social_case(
        &mut self,
        mut case: SocialCaseView,
        concept: Option<ConceptId>,
        public: bool,
    ) -> Result<CaseId, Error> {
        self.reserve_social_ids(1)?;
        let id = CaseId(allocate(&mut self.state)?);
        case.id = id;
        let observers: BTreeSet<_> = self
            .state
            .people
            .iter()
            .filter(|p| {
                p.location == Location::AtArea(case.area)
                    && self.independent(p.id)
                    && self.state.physical.as_ref().unwrap().actors[&p.id]
                        .vitals
                        .alive
                    && (public || p.id == case.subject || p.id == case.counterpart)
            })
            .take(64)
            .map(|p| p.id)
            .chain([case.subject])
            .collect();
        let observers = observers
            .into_iter()
            .map(|observer| {
                (
                    observer,
                    self.state.decisions.as_ref().unwrap().minds[&observer]
                        .personality
                        .empathy,
                )
            })
            .collect::<BTreeMap<_, _>>();
        self.state
            .institutions
            .as_mut()
            .unwrap()
            .remember_case(case.clone(), concept, &observers);
        self.emit(EventKind::SocialCaseRecorded {
            case: id,
            actor: case.subject,
        });
        Ok(id)
    }

    fn case_need_band(&self, actor: PersonId) -> u8 {
        let hunger = self.state.physical.as_ref().unwrap().actors[&actor]
            .vitals
            .hunger;
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
    fn linked_rule_concept(&self, agreement: AgreementId) -> Option<ConceptId> {
        self.state
            .institutions
            .as_ref()
            .unwrap()
            .institutions
            .values()
            .find_map(|i| {
                i.applied.iter().find_map(|(concept, actors)| {
                    actors
                        .values()
                        .any(|id| *id == agreement)
                        .then_some(*concept)
                })
            })
    }
    fn sharing_concept(&self, actor: PersonId, source: InventoryId) -> Option<ConceptId> {
        let w = self.state.institutions.as_ref().unwrap();
        w.institutions.values().find_map(|i| i.concepts.iter().find(|id| matches!(w.concepts[id].effect, SocialEffect::Access { inventory, allowed: true } if inventory == source) && i.grants.get(id).is_some_and(|actors| actors.contains(&actor))).copied())
    }
    fn record_received_support(
        &mut self,
        actor: PersonId,
        source: InventoryId,
    ) -> Result<(), Error> {
        let w = self.state.institutions.as_ref().unwrap();
        let Some(case) = w
            .exclusions
            .get(&actor)
            .and_then(|stores| stores.get(&source))
            .and_then(|id| w.cases.get(id))
            .cloned()
        else {
            return Ok(());
        };
        let concept = self.sharing_concept(actor, source);
        self.record_social_case(
            SocialCaseView {
                id: CaseId(0),
                outcome: SocialOutcome::Shared,
                observed_at: self.state.time,
                ..case
            },
            concept,
            true,
        )?;
        self.state
            .institutions
            .as_mut()
            .unwrap()
            .exclusions
            .get_mut(&actor)
            .unwrap()
            .remove(&source);
        Ok(())
    }

    pub(crate) fn reconcile_institutions(&mut self) -> Result<(), Error> {
        let Some(w) = &self.state.institutions else {
            return Ok(());
        };
        let cursor = w.last_event;
        let events: Vec<_> = self
            .state
            .events
            .iter()
            .skip(cursor as usize)
            .cloned()
            .collect();
        let last = self.state.next_event - 1;
        // Memory keys are validated person IDs, including the dead. Only new
        // people can change coverage; ordinary actions need no population scan.
        if w.memories.len() != self.state.people.len() {
            for person in &self.state.people {
                self.state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .entry(person.id)
                    .or_default();
            }
        }
        let attributable: BTreeSet<_> = events
            .iter()
            .filter_map(|e| {
                if let EventKind::ObligationPerformed {
                    obligation, actor, ..
                } = e.kind
                {
                    match self.state.agreements.as_ref().unwrap().obligations[&obligation].term {
                        AgreementTerm::Labor { site, .. } => Some((actor, site)),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();
        for event in events {
            match event.kind {
                EventKind::ObligationPerformed {
                    obligation, actor, ..
                } => {
                    let o =
                        self.state.agreements.as_ref().unwrap().obligations[&obligation].clone();
                    let (area, good, inventory) = match o.term {
                        AgreementTerm::Labor { site, .. } => {
                            let site = &self.state.physical.as_ref().unwrap().sites[&site];
                            (
                                site.area,
                                self.state.inventories[&site.output].good,
                                Some(site.output),
                            )
                        }
                        AgreementTerm::Delivery { destination, .. } => {
                            let inventory = &self.state.inventories[&destination];
                            let Some(area) = self.inventory_area(inventory) else {
                                continue;
                            };
                            (area, inventory.good, Some(destination))
                        }
                        _ => continue,
                    };
                    self.record_social_case(
                        SocialCaseView {
                            id: CaseId(0),
                            kind: SocialCaseKind::Cooperation,
                            subject: actor,
                            counterpart: o.term.parties()[1],
                            area,
                            inventory,
                            good,
                            need_band: self.case_need_band(actor),
                            reciprocal: true,
                            outcome: SocialOutcome::Contributed,
                            observed_at: self.state.time,
                        },
                        self.linked_rule_concept(o.agreement),
                        true,
                    )?;
                }
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    ..
                } if !attributable.contains(&(actor, site)) => {
                    let site = &self.state.physical.as_ref().unwrap().sites[&site];
                    let counterpart = self
                        .state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .custodians
                        .get(&output)
                        .copied()
                        .unwrap_or(actor);
                    if counterpart == actor {
                        continue;
                    }
                    self.record_social_case(
                        SocialCaseView {
                            id: CaseId(0),
                            kind: SocialCaseKind::Cooperation,
                            subject: actor,
                            counterpart,
                            area: site.area,
                            inventory: Some(output),
                            good: self.state.inventories[&output].good,
                            need_band: self.case_need_band(actor),
                            reciprocal: true,
                            outcome: SocialOutcome::Contributed,
                            observed_at: self.state.time,
                        },
                        None,
                        true,
                    )?;
                }
                EventKind::ObligationDefaulted { obligation } => {
                    let o =
                        self.state.agreements.as_ref().unwrap().obligations[&obligation].clone();
                    let actor = o.term.parties()[0];
                    let counterpart = o.term.parties()[1];
                    let Some(area) =
                        self.state
                            .people
                            .get(self.person_index[&actor])
                            .and_then(|p| {
                                if let Location::AtArea(area) = p.location {
                                    Some(area)
                                } else {
                                    None
                                }
                            })
                    else {
                        continue;
                    };
                    let inventory = match o.term {
                        AgreementTerm::Labor { site, .. } => {
                            self.state.physical.as_ref().unwrap().sites[&site].output
                        }
                        AgreementTerm::Delivery { destination, .. } => destination,
                        _ => continue,
                    };
                    let concept = self.linked_rule_concept(o.agreement);
                    self.record_social_case(
                        SocialCaseView {
                            id: CaseId(0),
                            kind: SocialCaseKind::Default,
                            subject: actor,
                            counterpart,
                            area,
                            inventory: Some(inventory),
                            good: self.state.inventories[&inventory].good,
                            need_band: self.case_need_band(actor),
                            reciprocal: self.known_case_count(
                                actor,
                                actor,
                                SocialCaseKind::Cooperation,
                            ) > 0,
                            outcome: SocialOutcome::Defaulted,
                            observed_at: self.state.time,
                        },
                        concept,
                        concept.is_some(),
                    )?;
                }
                EventKind::Ate { actor, source, .. } => {
                    self.record_received_support(actor, source)?
                }
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    ..
                } if self.state.inventories[&destination].location
                    == InventoryLocation::Carried(actor)
                    && self.state.inventories[&source].good == Good::Food =>
                {
                    self.record_received_support(actor, source)?
                }
                _ => {}
            }
        }
        self.state.institutions.as_mut().unwrap().last_event = last;
        Ok(())
    }

    fn observe_exclusions(&mut self) -> Result<(), Error> {
        let mut requests = Vec::new();
        let w = self.state.institutions.as_ref().unwrap();
        for (actor, mind) in &self.state.decisions.as_ref().unwrap().minds {
            let Ok(area) = self.social_area(*actor) else {
                continue;
            };
            let band = self.case_need_band(*actor);
            if band == 0 {
                continue;
            }
            if mind.knowledge.values().any(|k| {
                k.area == area && k.good == Good::Food && k.accessible && k.quantity.0 >= 1_000
            }) {
                continue;
            }
            for stock in mind
                .knowledge
                .values()
                .filter(|k| {
                    k.area == area && k.good == Good::Food && !k.accessible && k.quantity.0 >= 1_000
                })
                .take(8)
            {
                let Some(owner) = self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&stock.inventory)
                    .copied()
                else {
                    continue;
                };
                if w.exclusions
                    .get(actor)
                    .and_then(|stores| stores.get(&stock.inventory))
                    .and_then(|id| w.cases.get(id))
                    .is_some_and(|c| c.need_band == band)
                {
                    continue;
                }
                requests.push(SocialCaseView {
                    id: CaseId(0),
                    kind: SocialCaseKind::Exclusion,
                    subject: *actor,
                    counterpart: owner,
                    area,
                    inventory: Some(stock.inventory),
                    good: stock.good,
                    need_band: band,
                    reciprocal: self.known_case_count(*actor, *actor, SocialCaseKind::Cooperation)
                        > 0,
                    outcome: SocialOutcome::Unresolved,
                    observed_at: self.state.time,
                });
            }
        }
        for request in requests {
            let actor = request.subject;
            let inventory = request.inventory.unwrap();
            let id = self.record_social_case(request, None, false)?;
            self.state
                .institutions
                .as_mut()
                .unwrap()
                .exclusions
                .entry(actor)
                .or_default()
                .insert(inventory, id);
        }
        Ok(())
    }

    fn learned_rule_score(&self, actor: PersonId, rule: &InstitutionalRule) -> i64 {
        let w = self.state.institutions.as_ref().unwrap();
        let Some(memory) = w.memories.get(&actor) else {
            return 0;
        };
        memory
            .values
            .iter()
            .rev()
            .take(32)
            .map(|(id, value)| {
                let old = &w.concepts[id];
                let same = match (&old.effect, &rule.effect) {
                    (
                        SocialEffect::Access {
                            inventory: a,
                            allowed: aa,
                        },
                        SocialEffect::Access {
                            inventory: b,
                            allowed: ba,
                        },
                    ) => a == b && aa == ba,
                    (SocialEffect::Labor { site: a, .. }, SocialEffect::Labor { site: b, .. }) => {
                        a == b
                    }
                    (
                        SocialEffect::Delivery {
                            source: a,
                            destination: ad,
                            ..
                        },
                        SocialEffect::Delivery {
                            source: b,
                            destination: bd,
                            ..
                        },
                    ) => a == b && ad == bd,
                    _ => false,
                };
                if same { i64::from(value.weight) } else { 0 }
            })
            .sum::<i64>()
            .clamp(-4_000, 4_000)
    }
    fn rule_vote_score(
        &self,
        actor: PersonId,
        charter: &InstitutionCharter,
        rule: &InstitutionalRule,
    ) -> i64 {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let recipient = rule.subjects.members
            || rule.subjects.named.contains(&actor)
            || rule.subjects.offices.iter().any(|slot| {
                charter
                    .offices
                    .iter()
                    .any(|o| o.slot == *slot && o.holder == actor)
            });
        let mut score = i64::from(mind.personality.empathy) - 500;
        match rule.effect {
            SocialEffect::Access { inventory, allowed } => {
                if recipient
                    && mind
                        .knowledge
                        .get(&inventory)
                        .is_some_and(|k| k.quantity.0 >= 1_000)
                {
                    score += if allowed { 1_200 } else { -1_200 };
                }
                if self
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&inventory)
                    == Some(&actor)
                    && allowed
                {
                    score -= 300;
                }
            }
            SocialEffect::Labor {
                millis,
                ref beneficiary,
                ..
            } => {
                if recipient {
                    score -= (millis / 120).min(100_000) as i64;
                }
                if beneficiary == &Beneficiary::Person(actor)
                    || matches!(beneficiary, Beneficiary::Office(slot) if charter.offices.iter().any(|o| o.slot == *slot && o.holder == actor))
                {
                    score += 700;
                }
                if mind.dream == decisions::Aspiration::SupportOthers {
                    score += 300;
                }
            }
            SocialEffect::Delivery {
                quantity,
                ref beneficiary,
                ..
            } => {
                if recipient {
                    score -= quantity.0.min(100_000) as i64;
                }
                if beneficiary == &Beneficiary::Person(actor)
                    || matches!(beneficiary, Beneficiary::Office(slot) if charter.offices.iter().any(|o| o.slot == *slot && o.holder == actor))
                {
                    score += quantity.0.min(100_000) as i64;
                }
            }
        }
        score + self.learned_rule_score(actor, rule)
    }
    fn charter_vote_score(&self, actor: PersonId, charter: &InstitutionCharter) -> i64 {
        let mut score: i64 = charter
            .rules
            .iter()
            .map(|rule| self.rule_vote_score(actor, charter, rule))
            .sum();
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        for office in &charter.offices {
            score += i64::from(
                mind.relationships
                    .get(&office.holder)
                    .map_or(0, |r| r.trust),
            );
        }
        for contribution in &charter.contributions {
            if contribution.parties()[0] == actor {
                score -= match contribution {
                    AgreementTerm::Labor { millis, .. } => (millis / 120).min(100_000) as i64,
                    _ => contribution.amount().min(100_000) as i64,
                };
            } else if contribution.parties()[1] == actor {
                score += contribution.amount().min(100_000) as i64;
            }
        }
        score
    }
    fn change_vote_score(
        &self,
        actor: PersonId,
        i: &Institution,
        change: &InstitutionChange,
    ) -> i64 {
        match change {
            InstitutionChange::RatifyRegional { .. }
            | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                self.realm_vote_score(actor, change)
            }
            InstitutionChange::EnforcementPolicy { .. } | InstitutionChange::Adjudicate { .. } => {
                self.enforcement_vote_score(actor, i, change)
            }
            InstitutionChange::AddRule(rule) => self.rule_vote_score(actor, &i.charter, rule),
            InstitutionChange::ReplaceRule { index, rule } => {
                self.rule_vote_score(actor, &i.charter, rule)
                    - self.rule_vote_score(actor, &i.charter, &i.charter.rules[*index as usize])
            }
            InstitutionChange::RemoveRule { index } => {
                -self.rule_vote_score(actor, &i.charter, &i.charter.rules[*index as usize])
            }
            InstitutionChange::Appoint { holder, .. } => i64::from(
                self.state.decisions.as_ref().unwrap().minds[&actor]
                    .relationships
                    .get(holder)
                    .map_or(0, |r| r.trust),
            ),
            InstitutionChange::Procedure(_) | InstitutionChange::Invite(_) => 0,
            InstitutionChange::Dissolve => -self.charter_vote_score(actor, &i.charter),
        }
    }

    fn precedent_score(&self, actor: PersonId, case: &SocialCaseView) -> i64 {
        self.state
            .institutions
            .as_ref()
            .unwrap()
            .precedent_score(actor, case)
    }
    fn hear_known_cases(&mut self) -> Result<(), Error> {
        let w = self.state.institutions.as_ref().unwrap();
        let mut choices = Vec::new();
        for i in w
            .institutions
            .values()
            .filter(|i| i.status == InstitutionStatus::Active)
        {
            for office in i.charter.offices.iter().filter(|o| {
                o.holder != self.state.ruler
                    && o.powers.contains(&OfficePower::HearAccessCases)
                    && self.office_effective(i, o)
            }) {
                for id in w.memories[&office.holder].cases.iter().rev().take(64) {
                    let c = &w.cases[id];
                    if c.kind != SocialCaseKind::Exclusion
                        || c.outcome != SocialOutcome::Unresolved
                        || c.area != i.charter.jurisdiction
                        || !i.members.contains(&c.subject)
                        || w.heard.contains_key(id)
                        || c.inventory
                            .is_none_or(|inv| !office.inventories.contains(&inv))
                        || self.social_area(office.holder).ok() != Some(c.area)
                    {
                        continue;
                    }
                    let score = i64::from(c.need_band) * 100
                        + i64::from(
                            self.state.decisions.as_ref().unwrap().minds[&office.holder]
                                .personality
                                .empathy,
                        )
                        - 800
                        + self.precedent_score(office.holder, c);
                    choices.push((
                        office.holder,
                        InstitutionAction::HearCase {
                            institution: i.id,
                            office: office.slot,
                            case: *id,
                            share: score >= 0,
                        },
                    ));
                }
            }
        }
        for (actor, choice) in choices {
            let _ = self.institution_command(actor, &choice);
        }
        Ok(())
    }

    pub(crate) fn review_institutions(&mut self) -> Result<(), Error> {
        if self.state.institutions.is_none() {
            return Ok(());
        }
        self.reconcile_institutions()?;
        self.observe_exclusions()?;
        let w = self.state.institutions.as_ref().unwrap();
        let mut replies = Vec::new();
        for i in w
            .institutions
            .values()
            .filter(|i| i.status == InstitutionStatus::Proposed)
        {
            for actor in i.charter.founders.iter().filter(|id| {
                **id != self.state.ruler && !i.members.contains(id) && !i.declined.contains(id)
            }) {
                if self.social_area(*actor).ok() != Some(i.charter.jurisdiction) {
                    continue;
                }
                let action = if self.charter_vote_score(*actor, &i.charter) >= 0 {
                    InstitutionAction::Join { institution: i.id }
                } else {
                    InstitutionAction::Decline { institution: i.id }
                };
                replies.push((*actor, action));
            }
        }
        for p in w
            .proposals
            .values()
            .filter(|p| p.status == RuleProposalStatus::Proposed)
        {
            let i = &w.institutions[&p.institution];
            for actor in self
                .subjects(i, &i.charter.procedure.voters)
                .intersection(&i.members)
                .filter(|id| **id != self.state.ruler && !p.votes.contains_key(id))
            {
                if self.social_area(*actor).ok() != Some(i.charter.jurisdiction) {
                    continue;
                }
                replies.push((
                    *actor,
                    InstitutionAction::Vote {
                        proposal: p.id,
                        support: self.change_vote_score(*actor, i, &p.change)
                            - if self.social_area(p.proposer).ok() == self.social_area(*actor).ok()
                            {
                                self.invocation_social_penalty(p.proposer)
                            } else {
                                0
                            }
                            >= 0,
                    },
                ));
            }
        }
        for (actor, action) in replies {
            let _ = self.institution_command(actor, &action);
        }
        self.construct_social_arrangements()?;
        self.apply_institution_rules()?;
        self.perform_institution_duties();
        self.hear_known_cases()?;
        Ok(())
    }

    fn perform_institution_duties(&mut self) {
        let w = self.state.institutions.as_ref().unwrap();
        let agreements: BTreeSet<_> = w
            .institutions
            .values()
            .filter(|i| i.status == InstitutionStatus::Active)
            .flat_map(|i| {
                i.applied
                    .values()
                    .flat_map(|actors| actors.values().copied())
            })
            .collect();
        let mut intentions = BTreeMap::new();
        for agreement in agreements {
            let a = &self.state.agreements.as_ref().unwrap().agreements[&agreement];
            for id in &a.obligations {
                let o = &self.state.agreements.as_ref().unwrap().obligations[id];
                let AgreementTerm::Labor {
                    debtor,
                    site,
                    millis,
                    ..
                } = o.term
                else {
                    continue;
                };
                if debtor == self.state.ruler
                    || o.performed >= millis
                    || intentions.contains_key(&debtor)
                {
                    continue;
                }
                let Ok(area) = self.social_area(debtor) else {
                    continue;
                };
                let mind = &self.state.decisions.as_ref().unwrap().minds[&debtor];
                let body = &self.state.physical.as_ref().unwrap().actors[&debtor];
                let rules = &self.state.physical.as_ref().unwrap().rules;
                if !matches!(body.activity, Activity::Idle | Activity::Rest)
                    || body.vitals.fatigue >= rules.fatigue_limit
                    || body
                        .vitals
                        .hunger
                        .saturating_add(millis - o.performed)
                        .saturating_add(rules.eating_time.0)
                        >= rules.hunger_harm
                    || (mind.personality.discipline < 500 && mind.personality.empathy < 500)
                    || !mind
                        .sites
                        .get(&site)
                        .is_some_and(|s| s.area == area && s.access)
                {
                    continue;
                }
                intentions.insert(debtor, Command::Work { site });
            }
        }
        for (actor, command) in intentions {
            let _ = self.debug_actor_command(actor, command);
        }
    }

    fn construct_social_arrangements(&mut self) -> Result<(), Error> {
        let actors: Vec<_> = self
            .state
            .people
            .iter()
            .filter(|p| p.id != self.state.ruler)
            .map(|p| p.id)
            .collect();
        for actor in actors {
            let Ok(area) = self.social_area(actor) else {
                continue;
            };
            let w = self.state.institutions.as_ref().unwrap();
            if w.memories[&actor].search_after > self.state.time {
                continue;
            }
            let existing: Vec<_> = w
                .institutions
                .values()
                .filter(|i| i.charter.jurisdiction == area && i.status == InstitutionStatus::Active)
                .cloned()
                .collect();
            let mut acted = false;
            for i in &existing {
                if !i.members.contains(&actor)
                    && i.charter.open_membership
                    && self.charter_vote_score(actor, &i.charter) >= 0
                {
                    let _ = self
                        .institution_command(actor, &InstitutionAction::Join { institution: i.id });
                    acted = true;
                    break;
                }
                if i.members.contains(&actor)
                    && self
                        .subjects(i, &i.charter.procedure.proposers)
                        .contains(&actor)
                    && self.known_case_count(actor, actor, SocialCaseKind::Default) >= 2
                    && let Some((index, old)) =
                        i.charter.rules.iter().enumerate().find(|(_, rule)| {
                            matches!(rule.effect, SocialEffect::Access { allowed: true, .. })
                                && !expression_has_defaults(&rule.when)
                        })
                {
                    let mut rule = old.clone();
                    rule.when = RuleExpression::All(vec![
                        old.when.clone(),
                        RuleExpression::Not(Box::new(RuleExpression::Fact(
                            SocialCondition::KnownDefaultsAtLeast(1),
                        ))),
                    ]);
                    if self.validate_rule(&i.charter, &rule).is_ok() {
                        let _ = self.institution_command(
                            actor,
                            &InstitutionAction::Propose {
                                institution: i.id,
                                change: InstitutionChange::ReplaceRule {
                                    index: index as u16,
                                    rule,
                                },
                            },
                        );
                        acted = true;
                        break;
                    }
                }
            }
            if acted {
                self.state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .get_mut(&actor)
                    .unwrap()
                    .search_after = SimTime(self.state.time.0.saturating_add(300_000));
                continue;
            }
            let w = self.state.institutions.as_ref().unwrap();
            let Some(case) = w.memories[&actor]
                .cases
                .iter()
                .rev()
                .filter_map(|id| w.cases.get(id))
                .find(|c| {
                    c.kind == SocialCaseKind::Exclusion
                        && c.subject == actor
                        && c.outcome == SocialOutcome::Unresolved
                        && c.area == area
                })
                .cloned()
            else {
                continue;
            };
            let inventory = case.inventory.unwrap();
            if w.institutions.values().any(|i| i.charter.jurisdiction == area && matches!(i.status, InstitutionStatus::Active | InstitutionStatus::Proposed) && i.charter.rules.iter().any(|r| matches!(r.effect, SocialEffect::Access { inventory: target, .. } if target == inventory))) { continue; }
            let owner = case.counterpart;
            if owner == self.state.ruler || self.social_area(owner).ok() != Some(area) {
                continue;
            }
            let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
            let Some(site) = mind
                .sites
                .values()
                .find(|s| s.area == area && s.access)
                .map(|s| s.site)
            else {
                continue;
            };
            let mut founders = BTreeSet::from([actor, owner]);
            founders.extend(
                self.state
                    .people
                    .iter()
                    .filter(|p| {
                        p.role == Role::Citizen
                            && p.location == Location::AtArea(area)
                            && p.id != self.state.ruler
                            && self.social_area(p.id).is_ok()
                    })
                    .map(|p| p.id)
                    .filter(|id| *id != actor && *id != owner)
                    .take(6),
            );
            let hunger = RuleExpression::Fact(SocialCondition::HungerAtLeast(120_000));
            let stock = RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
                inventory,
                quantity: Quantity(2_000),
            });
            let cooperation = RuleExpression::Fact(SocialCondition::KnownCooperationAtLeast(1));
            let eligible = if mind.personality.empathy >= 800 {
                RuleExpression::Any(vec![hunger.clone(), cooperation])
            } else {
                cooperation
            };
            let mut share_conditions = vec![
                RuleExpression::Fact(SocialCondition::Member),
                stock.clone(),
                eligible,
            ];
            if self.known_case_count(actor, actor, SocialCaseKind::Default) > 0 {
                share_conditions.push(RuleExpression::Not(Box::new(RuleExpression::Fact(
                    SocialCondition::KnownDefaultsAtLeast(1),
                ))));
            }
            let charter = InstitutionCharter {
                purpose: format!("Access and contribution at store {}", inventory.0),
                jurisdiction: area,
                founders: founders.clone(),
                open_membership: true,
                procedure: DecisionProcedure {
                    proposers: Subjects::members(),
                    voters: Subjects::members(),
                    minimum_support: (founders.len() / 2 + 1) as u16,
                },
                offices: vec![OfficeMandate {
                    slot: 0,
                    holder: owner,
                    inventories: BTreeSet::from([inventory]),
                    sites: BTreeSet::from([site]),
                    powers: BTreeSet::from([OfficePower::Access, OfficePower::Labor]),
                    minimum_support: 2,
                }],
                rules: vec![
                    InstitutionalRule {
                        subjects: Subjects::members(),
                        when: RuleExpression::All(vec![
                            RuleExpression::Fact(SocialCondition::Member),
                            hunger,
                            stock,
                            RuleExpression::Fact(SocialCondition::Excluded { inventory }),
                        ]),
                        effect: SocialEffect::Labor {
                            site,
                            millis: 60_000,
                            beneficiary: Beneficiary::Person(owner),
                            due_after: SimDuration(180_000),
                        },
                        authority: 0,
                    },
                    InstitutionalRule {
                        subjects: Subjects::members(),
                        when: RuleExpression::All(share_conditions),
                        effect: SocialEffect::Access {
                            inventory,
                            allowed: true,
                        },
                        authority: 0,
                    },
                ],
                contributions: Vec::new(),
                evidence: BTreeSet::from([case.id]),
            };
            if self.charter_vote_score(actor, &charter) >= 0 {
                self.institution_command(
                    actor,
                    &InstitutionAction::Found {
                        charter,
                        origin: None,
                    },
                )?;
                self.state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .get_mut(&actor)
                    .unwrap()
                    .search_after = SimTime(self.state.time.0.saturating_add(300_000));
            }
        }
        Ok(())
    }

    pub(crate) fn available_institution_commands(&self, actor: PersonId) -> Vec<Command> {
        let Ok(area) = self.social_area(actor) else {
            return Vec::new();
        };
        let Some(w) = &self.state.institutions else {
            return Vec::new();
        };
        let Some(memory) = w.memories.get(&actor) else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for id in memory.institutions.keys().rev().take(64) {
            let i = &w.institutions[id];
            if i.charter.jurisdiction != area {
                continue;
            }
            if i.status == InstitutionStatus::Proposed
                && i.charter.founders.contains(&actor)
                && !i.members.contains(&actor)
            {
                actions.push(InstitutionAction::Join { institution: *id });
                actions.push(InstitutionAction::Decline { institution: *id });
            }
            if i.status == InstitutionStatus::Active {
                if i.members.contains(&actor) {
                    actions.push(InstitutionAction::Leave { institution: *id });
                    for office in &i.charter.offices {
                        actions.push(InstitutionAction::SupportOffice {
                            institution: *id,
                            office: office.slot,
                            support: !i
                                .office_support
                                .get(&office.slot)
                                .is_some_and(|ids| ids.contains(&actor)),
                        });
                    }
                } else if i.charter.open_membership || i.invited.contains(&actor) {
                    actions.push(InstitutionAction::Join { institution: *id });
                }
            }
        }
        for p in w
            .proposals
            .values()
            .rev()
            .filter(|p| p.status == RuleProposalStatus::Proposed)
            .take(32)
        {
            let i = &w.institutions[&p.institution];
            if i.charter.jurisdiction == area
                && i.members.contains(&actor)
                && self
                    .subjects(i, &i.charter.procedure.voters)
                    .contains(&actor)
                && !p.votes.contains_key(&actor)
            {
                actions.push(InstitutionAction::Vote {
                    proposal: p.id,
                    support: true,
                });
                actions.push(InstitutionAction::Vote {
                    proposal: p.id,
                    support: false,
                });
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Institution { action })
            .collect()
    }
}

fn expression_has_defaults(expression: &RuleExpression) -> bool {
    match expression {
        RuleExpression::Fact(SocialCondition::KnownDefaultsAtLeast(_)) => true,
        RuleExpression::All(items) | RuleExpression::Any(items) => {
            items.iter().any(expression_has_defaults)
        }
        RuleExpression::Not(e) => expression_has_defaults(e),
        _ => false,
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.institutions else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid institution state".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let areas: BTreeSet<_> = s.areas.iter().map(|a| a.id).collect();
    let physical = s.physical.as_ref().ok_or_else(fail)?;
    let agreements = s.agreements.as_ref().ok_or_else(fail)?;
    if s.decisions.is_none()
        || w.last_event >= s.next_event
        || w.memories.keys().any(|id| !people.contains(id))
    {
        return Err(fail());
    }
    fn expression_valid(
        e: &RuleExpression,
        s: &CanonicalState,
        depth: usize,
        nodes: &mut usize,
    ) -> bool {
        *nodes += 1;
        if depth > 4 || *nodes > 24 {
            return false;
        }
        match e {
            RuleExpression::All(items) | RuleExpression::Any(items) => {
                !items.is_empty()
                    && items.len() <= 8
                    && items
                        .iter()
                        .all(|e| expression_valid(e, s, depth + 1, nodes))
            }
            RuleExpression::Not(e) => expression_valid(e, s, depth + 1, nodes),
            RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
                inventory,
                quantity,
            }) => s.inventories.contains_key(inventory) && quantity.0 > 0,
            RuleExpression::Fact(SocialCondition::Excluded { inventory }) => {
                s.inventories.contains_key(inventory)
            }
            RuleExpression::Fact(SocialCondition::HungerAtLeast(n)) => *n > 0,
            RuleExpression::Fact(
                SocialCondition::KnownCooperationAtLeast(n)
                | SocialCondition::KnownDefaultsAtLeast(n),
            ) => (1..=64).contains(n),
            _ => true,
        }
    }
    let subjects_valid = |subjects: &Subjects, offices: &[OfficeMandate]| {
        (subjects.members || !subjects.named.is_empty() || !subjects.offices.is_empty())
            && subjects.named.len() <= 64
            && subjects.named.is_subset(&people)
            && subjects.offices.len() <= 8
            && subjects
                .offices
                .iter()
                .all(|slot| offices.iter().any(|o| o.slot == *slot))
    };
    let rule_references = |rule: &InstitutionalRule| {
        if rule.subjects.named.len() > 64
            || !rule.subjects.named.is_subset(&people)
            || !expression_valid(&rule.when, s, 0, &mut 0)
        {
            return false;
        }
        let beneficiary = |b: &Beneficiary| match b {
            Beneficiary::Person(id) => people.contains(id),
            Beneficiary::Office(_) => true,
        };
        match &rule.effect {
            SocialEffect::Access { inventory, .. } => s.inventories.contains_key(inventory),
            SocialEffect::Labor {
                site,
                millis,
                beneficiary: b,
                due_after,
            } => {
                physical.sites.contains_key(site)
                    && *millis > 0
                    && due_after.0 >= *millis
                    && beneficiary(b)
            }
            SocialEffect::Delivery {
                source,
                destination,
                quantity,
                beneficiary: b,
                due_after,
            } => {
                source != destination
                    && quantity.0 > 0
                    && due_after.0 >= 120_000
                    && s.inventories
                        .get(source)
                        .zip(s.inventories.get(destination))
                        .is_some_and(|(a, b)| a.good == b.good && quantity.0 <= b.capacity.0)
                    && beneficiary(b)
            }
        }
    };
    for (id, rule) in &w.concepts {
        if id.0 >= s.next_id || !rule_references(rule) {
            return Err(fail());
        }
    }
    for (id, i) in &w.institutions {
        let c = &i.charter;
        if *id != i.id
            || id.0 >= s.next_id
            || i.origin
                .is_some_and(|id| !w.institutions.contains_key(&id) || id == i.id)
            || c.purpose.is_empty()
            || c.purpose.len() > 160
            || !areas.contains(&c.jurisdiction)
            || !(2..=8).contains(&c.founders.len())
            || !c.founders.is_subset(&people)
            || i.members.len() > 64
            || !i.members.is_subset(&people)
            || !i.declined.is_subset(&c.founders)
            || !i.invited.is_subset(&people)
            || c.offices.is_empty()
            || c.offices.len() > 8
            || c.rules.len() > 16
            || i.concepts.len() != c.rules.len()
            || c.evidence.len() > 8
            || c.evidence.iter().any(|id| !w.cases.contains_key(id))
            || !subjects_valid(&c.procedure.proposers, &c.offices)
            || !subjects_valid(&c.procedure.voters, &c.offices)
            || !(1..=64).contains(&c.procedure.minimum_support)
        {
            return Err(fail());
        }
        let mut slots = BTreeSet::new();
        for office in &c.offices {
            if !slots.insert(office.slot)
                || !people.contains(&office.holder)
                || office.minimum_support == 0
                || usize::from(office.minimum_support) > c.founders.len()
                || office.inventories.len() > 16
                || office.sites.len() > 16
                || office.powers.is_empty()
                || office
                    .inventories
                    .iter()
                    .any(|id| !s.inventories.contains_key(id))
                || office
                    .sites
                    .iter()
                    .any(|id| !physical.sites.contains_key(id))
            {
                return Err(fail());
            }
        }
        for (rule, concept) in c.rules.iter().zip(&i.concepts) {
            let office = c
                .offices
                .iter()
                .find(|o| o.slot == rule.authority)
                .ok_or_else(fail)?;
            if w.concepts.get(concept) != Some(rule) || !subjects_valid(&rule.subjects, &c.offices)
            {
                return Err(fail());
            }
            let valid = match &rule.effect {
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
            if !valid {
                return Err(fail());
            }
        }
        if i.office_support
            .iter()
            .any(|(slot, ids)| !slots.contains(slot) || !ids.is_subset(&i.members))
            || i.grants.iter().any(|(concept, actors)| {
                !i.concepts.contains(concept)
                    || !actors.is_subset(&people)
                    || !matches!(
                        w.concepts[concept].effect,
                        SocialEffect::Access { allowed: true, .. }
                    )
            })
            || i.applied.iter().any(|(concept, actors)| {
                !w.concepts.contains_key(concept)
                    || actors.iter().any(|(actor, id)| {
                        !people.contains(actor) || !agreements.agreements.contains_key(id)
                    })
            })
            || i.funding
                .iter()
                .any(|id| !agreements.agreements.contains_key(id))
            || c.contributions.len() > 16
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
            if agreements.agreements[id].terms != vec![term.clone()] {
                return Err(fail());
            }
        }
    }
    for (id, p) in &w.proposals {
        let i = w.institutions.get(&p.institution).ok_or_else(fail)?;
        if *id != p.id
            || id.0 >= s.next_id
            || !people.contains(&p.proposer)
            || p.base_revision > i.revision
            || (p.status == RuleProposalStatus::Proposed && p.base_revision != i.revision)
            || p.votes.keys().any(|id| !people.contains(id))
        {
            return Err(fail());
        }
        match &p.change {
            InstitutionChange::AddRule(rule) | InstitutionChange::ReplaceRule { rule, .. } => {
                if !rule_references(rule) {
                    return Err(fail());
                }
            }
            InstitutionChange::Appoint { holder, .. } | InstitutionChange::Invite(holder)
                if !people.contains(holder) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    for (id, case) in &w.cases {
        if *id != case.id
            || id.0 >= s.next_id
            || !people.contains(&case.subject)
            || !people.contains(&case.counterpart)
            || !areas.contains(&case.area)
            || case
                .inventory
                .is_some_and(|id| !s.inventories.contains_key(&id))
            || case.need_band > 3
            || case.observed_at > s.time
        {
            return Err(fail());
        }
    }
    if w.exclusions.iter().any(|(actor, stores)| {
        !people.contains(actor)
            || stores.iter().any(|(inventory, id)| {
                w.cases.get(id).is_none_or(|c| {
                    c.subject != *actor
                        || c.inventory != Some(*inventory)
                        || c.kind != SocialCaseKind::Exclusion
                })
            })
    }) || w.heard.keys().any(|id| !w.cases.contains_key(id))
    {
        return Err(fail());
    }
    for memory in w.memories.values() {
        if memory.cases.iter().any(|id| !w.cases.contains_key(id))
            || memory.values.iter().any(|(id, value)| {
                !w.concepts.contains_key(id)
                    || !(-2_000..=2_000).contains(&value.weight)
                    || value.experiences == 0
            })
            || memory.institutions.iter().any(|(id, view)| {
                !w.institutions.contains_key(id)
                    || *id != view.id
                    || view.observed_at > s.time
                    || !view.members.is_subset(&people)
                    || !view.charter.founders.is_subset(&people)
                    || view
                        .proposals
                        .iter()
                        .any(|p| !w.proposals.contains_key(&p.id))
            })
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[cfg(test)]
mod closing_resource_tests {
    use super::*;
    fn many_defaults() -> Simulation {
        let mut sim = Simulation::institution_fixture(61, Execution::default()).unwrap();
        for person in sim.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.state.ruler;
        let creditor = sim.state.people[3].id;
        let area = sim.state.areas[0].id;
        let source = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Timber && stock.location == InventoryLocation::Area(area)
            })
            .unwrap()
            .id;
        let destination = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10000),
                BTreeSet::from([creditor]),
            )
            .unwrap();
        let parties = BTreeSet::from([debtor, creditor]);
        for _ in 0..1000 {
            let agreement = AgreementId(allocate(&mut sim.state).unwrap());
            let obligation = ObligationId(allocate(&mut sim.state).unwrap());
            let term = AgreementTerm::Delivery {
                debtor,
                creditor,
                source,
                destination,
                quantity: Quantity(1),
                deadline: SimTime(1),
            };
            let world = sim.state.agreements.as_mut().unwrap();
            world.agreements.insert(
                agreement,
                game_agreements::Agreement {
                    id: agreement,
                    proposer: debtor,
                    parties: parties.clone(),
                    accepted: parties.clone(),
                    terms: vec![term.clone()],
                    procedure: None,
                    obligations: vec![obligation],
                    status: AgreementStatus::Active,
                    proposed_at: SimTime(0),
                    revision: 0,
                },
            );
            world.obligations.insert(
                obligation,
                game_agreements::Obligation {
                    id: obligation,
                    agreement,
                    term,
                    performed: 0,
                    accrued: 0,
                    share_remainder: 0,
                    share_scale: 0,
                    defaulted: false,
                    cargo: BTreeMap::new(),
                },
            );
        }
        sim.state.next_id = u64::MAX - 500;
        // Loading is the full canonical validator, not an unchecked test setup.
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap()
    }
    #[test]
    fn closing_defaults_must_preflight_every_social_case_before_publishing() {
        let mut sim = many_defaults();
        assert!(
            sim.state
                .institutions
                .as_ref()
                .unwrap()
                .institutions
                .is_empty()
        );
        assert_eq!(sim.state.people.len(), 15);
        let before = sim.state.clone();
        assert_eq!(sim.advance(SimDuration(1)), Err(Error::Overflow));
        assert!(
            sim.state == before,
            "overflow must precede clock, defaults, social cases and counters"
        );
    }
    #[test]
    fn pending_unreconciled_default_receipts_are_also_preflighted_from_a_valid_save() {
        let mut sim = many_defaults();
        sim.state.next_id = 10000;
        sim.state.time = SimTime(1);
        sim.reconcile_agreements().unwrap();
        assert_eq!(
            sim.state
                .events
                .iter()
                .filter(|event| matches!(event.kind, EventKind::ObligationDefaulted { .. }))
                .count(),
            1000
        );
        assert!(sim.state.institutions.as_ref().unwrap().cases.is_empty());
        sim.state.next_id = u64::MAX - 500;
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.state.clone();
        assert_eq!(sim.advance(SimDuration(1)), Err(Error::Overflow));
        assert!(
            sim.state == before,
            "unreconciled receipts must not publish partial cases"
        );
    }
    #[test]
    fn sufficient_closing_case_headroom_commits_the_whole_valid_ledger_without_unused_review_reserves()
     {
        let mut sim = many_defaults();
        sim.state.next_id = u64::MAX - 1000;
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(sim.state.next_id, u64::MAX);
        assert_eq!(sim.state.institutions.as_ref().unwrap().cases.len(), 1000);
        assert!(
            sim.state
                .agreements
                .as_ref()
                .unwrap()
                .obligations
                .values()
                .all(|obligation| obligation.defaulted)
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}

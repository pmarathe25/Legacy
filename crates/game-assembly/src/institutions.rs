//! Institutions are supported arrangements of people and scoped authority.
//! Their rules create permissions and recognized duties, never physical payment.
use crate::*;
use game_society::institutions::{
    CommandEffect, CommandFacts, CommandIds, ExternalChangePermit, ReferenceFacts,
    ReferenceRequest, StockReference,
};
pub use game_society::institutions::{Institution, RuleProposal, SocialMemory, World};

struct InstitutionalVotingKnowledge<'a> {
    stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    custodians: &'a BTreeMap<InventoryId, PersonId>,
    relationships: &'a BTreeMap<PersonId, decisions::Relationship>,
}
impl game_society::voting::VotingKnowledge for InstitutionalVotingKnowledge<'_> {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        self.custodians.get(&id).copied()
    }
    fn trust(&self, id: PersonId) -> i32 {
        self.relationships.get(&id).map_or(0, |row| row.trust)
    }
}

impl Simulation {
    pub fn institution_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::agreement_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "institution-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let owner = sim.host.state.people[3].id;
        let area = sim.host.state.areas[0].id;
        let stocks: Vec<_> = game_cargo::inventories(&sim.host.storage)
            .filter(|(_, row)| {
                row.location == InventoryLocation::Area(area) && row.good == Good::Food
            })
            .map(|(id, _)| id)
            .collect();
        for id in stocks {
            game_cargo::inventory_mut(&mut sim.host.storage, id)
                .unwrap()
                .access = BTreeSet::from([owner]);
            sim.host
                .state
                .agreements
                .as_mut()
                .unwrap()
                .custodians
                .insert(id, owner);
        }
        sim.debug_set_personality(
            owner,
            decisions::Personality {
                empathy: 900,
                ..decisions::Personality::industrious()
            },
            decisions::Aspiration::SupportOthers,
        )?;
        sim.host.state.institutions = Some(World {
            institutions: BTreeMap::new(),
            proposals: BTreeMap::new(),
            concepts: BTreeMap::new(),
            cases: BTreeMap::new(),
            memories: sim
                .host
                .state
                .people
                .iter()
                .map(|p| (p.id, SocialMemory::default()))
                .collect(),
            last_event: sim.host.next_event() - 1,
            exclusions: BTreeMap::new(),
            heard: BTreeMap::new(),
        });
        Ok(sim)
    }

    fn social_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let p = self
            .person_index
            .get(&actor)
            .map(|i| &self.host.state.people[*i])
            .ok_or(Error::Unavailable)?;
        if !self.independent(actor)
            || self.combat_restricts(actor)
            || !self.host.state.physical.as_ref().is_some_and(|_| {
                game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|body| body.vitals.alive)
            })
        {
            return Err(Error::Unavailable);
        }
        match *game_spatial::location(&self.host.storage, game_ecs::StableId((p.id).0))
            .ok_or(Error::Unavailable)?
        {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }

    pub fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView> {
        self.host
            .state
            .institutions
            .as_ref()
            .map_or_else(Vec::new, |world| world.institution_views(actor))
    }
    pub fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView> {
        self.host
            .state
            .institutions
            .as_ref()
            .map_or_else(Vec::new, |world| world.social_case_views(actor))
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
            .host
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
        let w = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        self.preflight_institutions()?;
        if !matches!(
            action,
            InstitutionAction::HearCase { .. } | InstitutionAction::TellCase { .. }
        ) {
            return self.execute_institution_transition(actor, area, action);
        }
        let authority = game_society::institutions::AuthorityFacts {
            alive: w
                .institutions
                .values()
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let recipient_area = if let InstitutionAction::TellCase { recipient, .. } = action {
            self.social_area(*recipient).ok()
        } else {
            None
        };
        let refusal = w.case_command_record(action, self.time());
        let roster = self.social_observer_facts();
        let index = game_society::perception::ObservationIndex::new(&roster)?;
        let observers = refusal
            .as_ref()
            .map(|case| index.case_observers(case, false))
            .transpose()?
            .unwrap_or_default();
        if refusal.is_some() {
            self.reserve_social_ids(1)?;
        }
        let refusal_id = refusal.map(|_| CaseId(self.host.next_id()));
        let plan = w.prepare_case_command(
            action,
            game_society::case_commands::CaseCommandFacts {
                actor,
                area,
                now: self.time(),
                recipient_area,
                authority: &authority,
                refusal: refusal_id,
                refusal_observers: &observers,
            },
        )?;
        let effects = plan.commit(self.host.state.institutions.as_mut().unwrap())?;
        if let Some((case, subject)) = effects.recorded {
            self.host.allocate_ids(1)?;
            self.emit(EventKind::SocialCaseRecorded {
                case,
                actor: subject,
            });
        }
        if let Some(institution) = effects.publish {
            self.publish_institution(institution);
        }
        Ok(())
    }

    fn reserve_social_ids(&self, count: u64) -> Result<(), Error> {
        self.host
            .next_id()
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        self.host
            .next_event()
            .checked_add(count.checked_add(4).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn preflight_institutions(&self) -> Result<(), Error> {
        self.preflight_institutions_at(self.host.clock().time)
    }
    pub(crate) fn preflight_institutions_at(&self, at: SimTime) -> Result<(), Error> {
        let Some(w) = &self.host.state.institutions else {
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
        let Some(world) = &self.host.state.institutions else {
            return Ok(Default::default());
        };
        use game_society::institutions::CaseReceipt;
        let mut receipts = Vec::new();
        for event in self.host.events().iter().skip(world.last_event as usize) {
            let receipt = match event.event {
                EventKind::ObligationPerformed {
                    obligation, actor, ..
                } => match self.host.state.agreements.as_ref().unwrap().obligations[&obligation]
                    .term
                {
                    AgreementTerm::Labor { site, .. } => Some(CaseReceipt::Labor { actor, site }),
                    AgreementTerm::Delivery { destination, .. }
                        if self
                            .inventory_area(
                                game_cargo::inventory(&self.host.storage, destination)
                                    .expect("inventory exists"),
                            )
                            .is_some() =>
                    {
                        Some(CaseReceipt::Ledger)
                    }
                    _ => None,
                },
                EventKind::ObligationDefaulted { obligation } => {
                    let term =
                        &self.host.state.agreements.as_ref().unwrap().obligations[&obligation].term;
                    (matches!(
                        term,
                        AgreementTerm::Labor { .. } | AgreementTerm::Delivery { .. }
                    ) && matches!(
                        *game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId(
                                (self.host.state.people[self.person_index[&term.parties()[0]]].id)
                                    .0
                            )
                        )
                        .expect("location exists"),
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
                    .host
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
                } if game_cargo::inventory(&self.host.storage, source)
                    .expect("inventory exists")
                    .good
                    == Good::Food
                    && game_cargo::inventory(&self.host.storage, destination)
                        .expect("inventory exists")
                        .location
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
                    self.host
                        .state
                        .physical
                        .as_ref()
                        .and_then(|_| game_physiology::body(&self.host.storage, *id))
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
        let Some(world) = &self.host.state.institutions else {
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

    fn social_observer_facts(&self) -> Vec<game_society::perception::ObserverFacts> {
        self.host
            .state
            .people
            .iter()
            .map(|person| {
                let area = match game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId(person.id.0),
                ) {
                    Some(Location::AtArea(area)) => Some(*area),
                    _ => None,
                };
                game_society::perception::ObserverFacts {
                    actor: person.id,
                    area,
                    alive: game_physiology::body(&self.host.storage, person.id)
                        .is_some_and(|body| body.vitals.alive),
                    independent: self.independent(person.id),
                    conversation_allowed: !self.combat_restricts(person.id),
                    empathy: game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        person.id,
                    )
                    .expect("mind exists")
                    .personality
                    .empathy,
                }
            })
            .collect()
    }
    fn publish_institution(&mut self, id: InstitutionId) {
        let roster = self.social_observer_facts();
        let index = game_society::perception::ObservationIndex::new(&roster)
            .expect("unique observer roster");
        let world = self.host.state.institutions.as_ref().unwrap();
        let authority = self.institution_authority_facts(&world.institutions[&id]);
        let publication = world
            .prepare_publication(id, &authority, self.time(), &index)
            .expect("known institution");
        publication
            .commit(self.host.state.institutions.as_mut().unwrap())
            .expect("immediate publication guard");
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
                    game_cargo::inventory(&self.host.storage, *(id)).map(|stock| {
                        (
                            *id,
                            StockReference {
                                area: self.inventory_area(stock),
                                custodian: self
                                    .host
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
                    self.host
                        .state
                        .physical
                        .as_ref()
                        .and_then(|_| game_workshop::site(&self.host.storage, *id))
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
    fn execute_institution_transition(
        &mut self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let world = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let request = world.command_references(actor, action)?;
        let references = self.institution_reference_facts(&request);
        let authority = game_society::institutions::AuthorityFacts {
            alive: request
                .people
                .iter()
                .filter(|id| {
                    self.host
                        .state
                        .physical
                        .as_ref()
                        .and_then(|_| game_physiology::body(&self.host.storage, **id))
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
                now: self.host.clock().time,
                references: &references,
                authority: &authority,
                external_permit: permit.as_ref(),
            },
            CommandIds {
                first: self.host.next_id(),
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
                .checked_sub(self.host.next_id())
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
                self.host
                    .state
                    .agreements
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .ledger
                    .prepare_institutional_duties(&duties, self.host.clock().time)?,
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
        plan.verify(self.host.state.institutions.as_ref().unwrap())?;
        let effects = plan.effects.clone();
        let next_id = plan.next_id;
        if let Some(funding) = &funding {
            funding.check(&self.host.state.agreements.as_ref().unwrap().ledger)?;
        }
        plan.commit(self.host.state.institutions.as_mut().unwrap())?;
        if let Some(funding) = funding {
            funding.commit(&mut self.host.state.agreements.as_mut().unwrap().ledger)?;
        }
        self.host.allocate_ids(
            (next_id)
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?,
        )?;
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
        let agreement = AgreementId(self.host.next_id());
        let obligation = ObligationId(self.host.next_id().checked_add(1).ok_or(Error::Overflow)?);
        let next_id = self.host.next_id().checked_add(2).ok_or(Error::Overflow)?;
        let plan = self
            .host
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
                self.host.clock().time,
            )?;
        plan.commit(&mut self.host.state.agreements.as_mut().unwrap().ledger)?;
        self.host.allocate_ids(
            (next_id)
                .checked_sub(self.host.next_id())
                .ok_or(Error::Overflow)?,
        )?;
        self.emit(EventKind::AgreementActivated { agreement });
        Ok(agreement)
    }

    fn apply_institution_rules(&mut self) -> Result<(), Error> {
        let world = self.host.state.institutions.as_ref().unwrap();
        let holders = world
            .institutions
            .values()
            .filter(|institution| institution.status == InstitutionStatus::Active)
            .flat_map(|institution| {
                institution
                    .charter
                    .offices
                    .iter()
                    .map(|office| office.holder)
            })
            .collect::<BTreeSet<_>>();
        let speakers = holders
            .iter()
            .map(|actor| game_society::deliberation::RuleSpeaker {
                actor: *actor,
                area: self.social_area(*actor).ok(),
                hunger: game_physiology::body(&self.host.storage, *actor)
                    .expect("body exists")
                    .vitals
                    .hunger,
                known_stocks: &game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    *actor,
                )
                .expect("mind exists")
                .knowledge,
            })
            .collect::<Vec<_>>();
        let locations = self
            .host
            .state
            .people
            .iter()
            .filter_map(|person| {
                self.social_area(person.id)
                    .ok()
                    .map(|area| (person.id, area))
            })
            .collect();
        let authority = game_society::institutions::AuthorityFacts {
            alive: world
                .institutions
                .values()
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let applications =
            world.rule_applications(&authority, &speakers, &locations, self.time())?;
        for application in applications {
            for intent in application.intents {
                match &intent {
                    game_society::institutions::RuleIntent::Access {
                        institution,
                        index,
                        actor,
                        ..
                    } => {
                        if self
                            .host
                            .state
                            .institutions
                            .as_mut()
                            .unwrap()
                            .apply_access_intent(&intent)?
                        {
                            self.emit(EventKind::InstitutionRuleApplied {
                                institution: *institution,
                                rule: *index,
                                actor: *actor,
                                agreement: None,
                            });
                        }
                    }
                    game_society::institutions::RuleIntent::Duty {
                        institution,
                        index,
                        actor,
                        term,
                        ..
                    } => {
                        let agreement = self.institutional_agreement(*institution, term.clone())?;
                        self.host
                            .state
                            .institutions
                            .as_mut()
                            .unwrap()
                            .register_duty(&intent, agreement)?;
                        self.emit(EventKind::InstitutionRuleApplied {
                            institution: *institution,
                            rule: *index,
                            actor: *actor,
                            agreement: Some(agreement),
                        });
                    }
                }
            }
            self.publish_institution(application.institution);
        }
        Ok(())
    }
    fn case_subject_facts(&self, actor: PersonId) -> game_society::reactions::SubjectFacts {
        game_society::reactions::SubjectFacts {
            actor,
            area: match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            },
            hunger: game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .hunger,
        }
    }
    fn commit_social_reaction(
        &mut self,
        reaction: game_society::reactions::CaseReaction,
        index: &game_society::perception::ObservationIndex<'_>,
    ) -> Result<CaseId, Error> {
        self.reserve_social_ids(1)?;
        let observers = index.case_observers(&reaction.case, reaction.public)?;
        let actor = reaction.case.subject;
        let id = CaseId(self.host.allocate_id()?.0);
        self.host
            .state
            .institutions
            .as_mut()
            .unwrap()
            .commit_case_reaction(reaction, id, &observers)?;
        self.emit(EventKind::SocialCaseRecorded { case: id, actor });
        Ok(id)
    }
    fn social_receipt(&self, event: &EventKind) -> Option<game_society::reactions::SocialReceipt> {
        use game_society::reactions::{CaseAsset, CooperationTarget, SocialReceipt};
        let asset = |inventory: InventoryId, area: Option<AreaId>| {
            let stock =
                game_cargo::inventory(&self.host.storage, inventory).expect("receipt stock exists");
            CaseAsset {
                area,
                inventory,
                good: stock.good,
            }
        };
        let target = |term: &AgreementTerm| match *term {
            AgreementTerm::Labor { site, .. } => {
                let site =
                    game_workshop::site(&self.host.storage, site).expect("receipt site exists");
                Some(CooperationTarget::Labor {
                    site: site.id,
                    asset: asset(site.output, Some(site.area)),
                })
            }
            AgreementTerm::Delivery { destination, .. } => {
                let stock = game_cargo::inventory(&self.host.storage, destination)
                    .expect("receipt stock exists");
                Some(CooperationTarget::Delivery {
                    asset: asset(destination, self.inventory_area(stock)),
                })
            }
            _ => None,
        };
        match *event {
            EventKind::ObligationPerformed {
                obligation, actor, ..
            } => {
                let obligation =
                    &self.host.state.agreements.as_ref().unwrap().obligations[&obligation];
                Some(SocialReceipt::Performed {
                    actor,
                    counterpart: obligation.term.parties()[1],
                    agreement: obligation.agreement,
                    target: target(&obligation.term)?,
                })
            }
            EventKind::ObligationDefaulted { obligation } => {
                let obligation =
                    &self.host.state.agreements.as_ref().unwrap().obligations[&obligation];
                let asset = match target(&obligation.term)? {
                    CooperationTarget::Labor { asset, .. }
                    | CooperationTarget::Delivery { asset } => asset,
                };
                Some(SocialReceipt::Defaulted {
                    actor: obligation.term.parties()[0],
                    counterpart: obligation.term.parties()[1],
                    agreement: obligation.agreement,
                    asset,
                })
            }
            EventKind::Produced {
                actor,
                site,
                output,
                ..
            } => {
                let site =
                    game_workshop::site(&self.host.storage, site).expect("receipt site exists");
                Some(SocialReceipt::Produced {
                    actor,
                    site: site.id,
                    asset: asset(output, Some(site.area)),
                    custodian: self
                        .host
                        .state
                        .agreements
                        .as_ref()
                        .unwrap()
                        .custodians
                        .get(&output)
                        .copied(),
                })
            }
            EventKind::Ate { actor, source, .. } => Some(SocialReceipt::Ate { actor, source }),
            EventKind::Transferred {
                actor,
                source,
                destination,
                ..
            } => {
                let stock = game_cargo::inventory(&self.host.storage, source)
                    .expect("receipt stock exists");
                let destination = game_cargo::inventory(&self.host.storage, destination)
                    .expect("receipt stock exists");
                Some(SocialReceipt::Transferred {
                    actor,
                    source,
                    good: stock.good,
                    destination_holder: match destination.location {
                        InventoryLocation::Carried(actor) => Some(actor),
                        _ => None,
                    },
                })
            }
            _ => None,
        }
    }
    pub(crate) fn reconcile_institutions(&mut self) -> Result<(), Error> {
        let Some(world) = &self.host.state.institutions else {
            return Ok(());
        };
        let cursor = world.last_event;
        let last = self.host.next_event() - 1;
        if world.memories.len() != self.host.state.people.len() {
            for person in &self.host.state.people {
                self.host
                    .state
                    .institutions
                    .as_mut()
                    .unwrap()
                    .memories
                    .entry(person.id)
                    .or_default();
            }
        }
        let receipts = self
            .host
            .events()
            .iter()
            .skip(cursor as usize)
            .filter_map(|event| self.social_receipt(&event.event))
            .collect::<Vec<_>>();
        let receipt_index = game_society::reactions::ReceiptIndex::new(&receipts);
        let roster = self.social_observer_facts();
        let witnesses = game_society::perception::ObservationIndex::new(&roster)?;
        for receipt in receipts {
            use game_society::reactions::SocialReceipt;
            let actor = match receipt {
                SocialReceipt::Performed { actor, .. }
                | SocialReceipt::Produced { actor, .. }
                | SocialReceipt::Defaulted { actor, .. }
                | SocialReceipt::SupportReceived { actor, .. }
                | SocialReceipt::Ate { actor, .. }
                | SocialReceipt::Transferred { actor, .. } => actor,
            };
            let subject = self.case_subject_facts(actor);
            if let Some(reaction) = self
                .host
                .state
                .institutions
                .as_ref()
                .unwrap()
                .receipt_reaction(&receipt, subject, self.time(), &receipt_index)
            {
                self.commit_social_reaction(reaction, &witnesses)?;
            }
        }
        self.host.state.institutions.as_mut().unwrap().last_event = last;
        Ok(())
    }
    fn observe_exclusions(&mut self) -> Result<(), Error> {
        let world = self.host.state.institutions.as_ref().unwrap();
        let mut reactions = Vec::new();
        for (actor, mind) in
            game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
        {
            let Ok(area) = self.social_area(actor) else {
                continue;
            };
            let mut subject = self.case_subject_facts(actor);
            subject.area = Some(area);
            reactions.extend(world.exclusion_reactions(
                subject,
                &mind.knowledge,
                &self.host.state.agreements.as_ref().unwrap().custodians,
                self.time(),
            ));
        }
        let roster = self.social_observer_facts();
        let witnesses = game_society::perception::ObservationIndex::new(&roster)?;
        for reaction in reactions {
            self.commit_social_reaction(reaction, &witnesses)?;
        }
        Ok(())
    }
    fn institutional_vote_knowledge(&self, actor: PersonId) -> InstitutionalVotingKnowledge<'_> {
        let mind = game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            .expect("institutional actor has a mind");
        InstitutionalVotingKnowledge {
            stocks: &mind.knowledge,
            custodians: &self
                .host
                .state
                .agreements
                .as_ref()
                .expect("agreements are installed")
                .custodians,
            relationships: &mind.relationships,
        }
    }
    fn institutional_vote_facts<'a>(
        &self,
        actor: PersonId,
        knowledge: &'a dyn game_society::voting::VotingKnowledge,
    ) -> game_society::voting::VoterFacts<'a> {
        let mind = game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            .expect("institutional actor has a mind");
        game_society::voting::VoterFacts {
            actor,
            local_area: self.social_area(actor).ok(),
            autonomous: actor != self.host.state.ruler,
            empathy: mind.personality.empathy,
            supports_others: mind.dream == decisions::Aspiration::SupportOthers,
            knowledge,
        }
    }
    fn rule_vote_score(
        &self,
        actor: PersonId,
        charter: &InstitutionCharter,
        rule: &InstitutionalRule,
    ) -> i64 {
        let knowledge = self.institutional_vote_knowledge(actor);
        self.host
            .state
            .institutions
            .as_ref()
            .unwrap()
            .rule_vote_score(
                &self.institutional_vote_facts(actor, &knowledge),
                charter,
                rule,
            )
    }
    fn charter_vote_score(&self, actor: PersonId, charter: &InstitutionCharter) -> i64 {
        let knowledge = self.institutional_vote_knowledge(actor);
        self.host
            .state
            .institutions
            .as_ref()
            .unwrap()
            .charter_vote_score(&self.institutional_vote_facts(actor, &knowledge), charter)
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
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .unwrap()
                    .relationships
                    .get(holder)
                    .map_or(0, |r| r.trust),
            ),
            InstitutionChange::Procedure(_) | InstitutionChange::Invite(_) => 0,
            InstitutionChange::Dissolve => -self.charter_vote_score(actor, &i.charter),
        }
    }

    fn hear_known_cases(&mut self) -> Result<(), Error> {
        let world = self.host.state.institutions.as_ref().unwrap();
        let holders = world
            .institutions
            .values()
            .flat_map(|institution| {
                institution
                    .charter
                    .offices
                    .iter()
                    .map(|office| office.holder)
            })
            .collect::<BTreeSet<_>>();
        let actors = holders
            .into_iter()
            .map(|actor| game_society::deliberation::HearingActor {
                actor,
                area: self.social_area(actor).ok(),
                autonomous: actor != self.host.state.ruler,
                empathy: game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    actor,
                )
                .expect("mind exists")
                .personality
                .empathy,
            })
            .collect::<Vec<_>>();
        let authority = game_society::institutions::AuthorityFacts {
            alive: world
                .institutions
                .values()
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let choices = world.hearing_intentions(&authority, &actors)?;
        for (actor, choice) in choices {
            let _ = self.institution_command(actor, &choice);
        }
        Ok(())
    }
    pub(crate) fn review_institutions(&mut self) -> Result<(), Error> {
        if self.host.state.institutions.is_none() {
            return Ok(());
        }
        self.reconcile_institutions()?;
        self.observe_exclusions()?;
        let w = self.host.state.institutions.as_ref().unwrap();
        let founders = w
            .institutions
            .values()
            .filter(|row| row.status == InstitutionStatus::Proposed)
            .flat_map(|row| row.charter.founders.iter().copied())
            .collect::<BTreeSet<_>>();
        let knowledge = founders
            .into_iter()
            .map(|actor| (actor, self.institutional_vote_knowledge(actor)))
            .collect::<Vec<_>>();
        let facts = knowledge
            .iter()
            .map(|(actor, knowledge)| self.institutional_vote_facts(*actor, knowledge))
            .collect::<Vec<_>>();
        let mut replies = w.formation_replies(&facts)?;
        for p in w
            .proposals
            .values()
            .filter(|p| p.status == RuleProposalStatus::Proposed)
        {
            let i = &w.institutions[&p.institution];
            for actor in self
                .subjects(i, &i.charter.procedure.voters)
                .intersection(&i.members)
                .filter(|id| **id != self.host.state.ruler && !p.votes.contains_key(id))
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
        use game_society::deliberation::{DutyWorker, LivelihoodSite};
        let world = self.host.state.institutions.as_ref().unwrap();
        let people = world
            .duty_workers(&self.host.state.agreements.as_ref().unwrap().ledger)
            .expect("known duty ledger");
        let site_sets = people
            .iter()
            .map(|actor| {
                let sites = game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    *actor,
                )
                .expect("mind exists")
                .sites
                .values()
                .map(|site| LivelihoodSite {
                    site: site.site,
                    area: site.area,
                    accessible: site.access,
                })
                .collect::<Vec<_>>();
                (*actor, sites)
            })
            .collect::<Vec<_>>();
        let rules = &self.host.state.physical.as_ref().unwrap().rules;
        let workers = site_sets
            .iter()
            .map(|(actor, sites)| {
                let mind = game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    *actor,
                )
                .expect("mind exists");
                let body = game_physiology::body(&self.host.storage, *actor).expect("body exists");
                DutyWorker {
                    actor: *actor,
                    autonomous: *actor != self.host.state.ruler,
                    area: self.social_area(*actor).ok(),
                    idle: matches!(
                        self.host.storage.activity(*actor),
                        Some(Activity::Idle | Activity::Rest)
                    ),
                    work_budget: game_physiology::sustainable_work(
                        &body.vitals,
                        rules.fatigue_limit,
                        rules.hunger_harm,
                        rules.eating_time,
                    ),
                    discipline: mind.personality.discipline,
                    empathy: mind.personality.empathy,
                    sites,
                }
            })
            .collect::<Vec<_>>();
        let intentions = world
            .duty_work_intentions(
                &self.host.state.agreements.as_ref().unwrap().ledger,
                &workers,
            )
            .expect("known duty ledger");
        for intent in intentions {
            let _ = self.debug_actor_command(intent.actor, Command::Work { site: intent.site });
        }
    }

    fn construct_social_arrangements(&mut self) -> Result<(), Error> {
        use game_society::deliberation::{
            DiscoveryFacts, FailureHandling, LivelihoodSite, Recruit,
        };
        let roster = self
            .host
            .state
            .people
            .iter()
            .map(|person| Recruit {
                actor: person.id,
                area: self.social_area(person.id).ok(),
                autonomous: person.id != self.host.state.ruler,
                citizen: person.role == Role::Citizen,
            })
            .collect::<Vec<_>>();
        let mut by_area = BTreeMap::<AreaId, Vec<Recruit>>::new();
        for person in &roster {
            if let Some(area) = person.area {
                by_area.entry(area).or_default().push(*person);
            }
        }
        for person in &roster {
            let actor = person.actor;
            let Some(area) = person.area else {
                continue;
            };
            let knowledge = self.institutional_vote_knowledge(actor);
            let voter = self.institutional_vote_facts(actor, &knowledge);
            let sites =
                game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                    .expect("mind exists")
                    .sites
                    .values()
                    .map(|site| LivelihoodSite {
                        site: site.site,
                        area: site.area,
                        accessible: site.access,
                    })
                    .collect::<Vec<_>>();
            let recruits = by_area.get(&area).map_or(&[][..], Vec::as_slice);
            let world = self.host.state.institutions.as_ref().unwrap();
            let request = world.discovery_references(actor, area, &sites, recruits);
            let references = self.institution_reference_facts(&request);
            let authority = game_society::institutions::AuthorityFacts {
                alive: request
                    .people
                    .iter()
                    .copied()
                    .filter(|id| {
                        game_physiology::body(&self.host.storage, *id)
                            .is_some_and(|body| body.vitals.alive)
                    })
                    .collect(),
            };
            let decision = world.discovery_decision(DiscoveryFacts {
                voter: &voter,
                now: self.time(),
                authority: &authority,
                references: &references,
                sites: &sites,
                recruits,
            });
            let Some(decision) = decision else {
                continue;
            };
            let result = self.institution_command(actor, &decision.action);
            let accepted = result.is_ok();
            if decision.failure == FailureHandling::Propagate {
                result?;
            }
            self.host
                .state
                .institutions
                .as_mut()
                .unwrap()
                .record_discovery_attempt(actor, &decision, accepted)?;
        }
        Ok(())
    }
    pub(crate) fn available_institution_commands(&self, actor: PersonId) -> Vec<Command> {
        let Ok(area) = self.social_area(actor) else {
            return Vec::new();
        };
        let Some(world) = &self.host.state.institutions else {
            return Vec::new();
        };
        let authority = game_society::institutions::AuthorityFacts {
            alive: world
                .institutions
                .values()
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        world
            .institution_opportunities(actor, area, &authority)
            .into_iter()
            .map(|action| Command::Institution { action })
            .collect()
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.state.institutions else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid institution state".into());
    let people: BTreeSet<_> = s.state.people.iter().map(|p| p.id).collect();
    let areas: BTreeSet<_> = s.state.areas.iter().map(|a| a.id).collect();
    s.state.physical.as_ref().ok_or_else(fail)?;
    let agreements = s.state.agreements.as_ref().ok_or_else(fail)?;
    if s.state.decisions.is_none()
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
            }) => game_cargo::inventory(s.components, *(inventory)).is_some() && quantity.0 > 0,
            RuleExpression::Fact(SocialCondition::Excluded { inventory }) => {
                game_cargo::inventory(s.components, *(inventory)).is_some()
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
            SocialEffect::Access { inventory, .. } => {
                game_cargo::inventory(s.components, *(inventory)).is_some()
            }
            SocialEffect::Labor {
                site,
                millis,
                beneficiary: b,
                due_after,
            } => {
                game_workshop::site(s.components, *site).is_some()
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
                    && game_cargo::inventory(s.components, *(source))
                        .zip(game_cargo::inventory(s.components, *(destination)))
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
                    .any(|id| !game_cargo::inventory(s.components, *(id)).is_some())
                || office
                    .sites
                    .iter()
                    .any(|id| game_workshop::site(s.components, *id).is_none())
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
                .is_some_and(|id| !game_cargo::inventory(s.components, id).is_some())
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
        for person in sim.host.state.people.clone() {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.host.state.ruler;
        let creditor = sim.host.state.people[3].id;
        let area = sim.host.state.areas[0].id;
        let source = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
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
            let agreement = AgreementId(sim.host.allocate_id().unwrap().0);
            let obligation = ObligationId(sim.host.allocate_id().unwrap().0);
            let term = AgreementTerm::Delivery {
                debtor,
                creditor,
                source,
                destination,
                quantity: Quantity(1),
                deadline: SimTime(1),
            };
            let world = sim.host.state.agreements.as_mut().unwrap();
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
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(u64::MAX - 500);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        // Loading is the full canonical validator, not an unchecked test setup.
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap()
    }
    #[test]
    fn closing_defaults_must_preflight_every_social_case_before_publishing() {
        let mut sim = many_defaults();
        assert!(
            sim.host
                .state
                .institutions
                .as_ref()
                .unwrap()
                .institutions
                .is_empty()
        );
        assert_eq!(sim.host.state.people.len(), 15);
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.advance(SimDuration(1)), Err(Error::Overflow));
        assert!(
            sim.canonical_snapshot().unwrap() == before,
            "overflow must precede clock, defaults, social cases and counters"
        );
    }
    #[test]
    fn pending_unreconciled_default_receipts_are_also_preflighted_from_a_valid_save() {
        let mut sim = many_defaults();
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(10000);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        sim.host.publish_clock(game_sim::Clock {
            time: SimTime(1),
            ..sim.host.clock()
        });
        sim.reconcile_agreements().unwrap();
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventKind::ObligationDefaulted { .. }))
                .count(),
            1000
        );
        assert!(
            sim.host
                .state
                .institutions
                .as_ref()
                .unwrap()
                .cases
                .is_empty()
        );
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(u64::MAX - 500);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.advance(SimDuration(1)), Err(Error::Overflow));
        assert!(
            sim.canonical_snapshot().unwrap() == before,
            "unreconciled receipts must not publish partial cases"
        );
    }
    #[test]
    fn sufficient_closing_case_headroom_commits_the_whole_valid_ledger_without_unused_review_reserves()
     {
        let mut sim = many_defaults();
        {
            let mut authored: serde_json::Value =
                serde_json::from_slice(&sim.save().unwrap()).unwrap();
            authored["state"]["next_id"] = serde_json::json!(u64::MAX - 1000);
            sim = Simulation::load(
                &serde_json::to_vec(&authored).unwrap(),
                sim.execution.clone(),
            )
            .unwrap();
        }
        let mut sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(sim.host.next_id(), u64::MAX);
        assert_eq!(
            sim.host.state.institutions.as_ref().unwrap().cases.len(),
            1000
        );
        assert!(
            sim.host
                .state
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

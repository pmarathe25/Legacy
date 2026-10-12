//! Institutions are supported arrangements of people and scoped authority.
//! Their rules create permissions and recognized duties, never physical payment.
use crate::*;
use game_society::institutions::{
    CommandEffect, CommandFacts, CommandIds, ExternalChangePermit, ReferenceFacts,
    ReferenceRequest, StockReference,
};
pub use game_society::institutions::{Institution, RuleProposal, Runtime};

struct InstitutionalVotingKnowledge<'a> {
    stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    store: &'a game_ecs::Store,
    relationships: &'a BTreeMap<PersonId, decisions::Relationship>,
}
impl game_society::voting::VotingKnowledge for InstitutionalVotingKnowledge<'_> {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn custodian(&self, id: InventoryId) -> Option<PersonId> {
        game_property::inventory_custodian(self.store, id)
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
            game_property::declare_custody(&mut sim.host.storage, id, owner)
                .expect("declared custody");
        }
        sim.debug_set_personality(
            owner,
            decisions::Personality {
                empathy: 900,
                ..decisions::Personality::industrious()
            },
            decisions::Aspiration::SupportOthers,
        )?;
        sim.host.state.institutions = Some(Runtime {
            last_event: sim.host.next_event() - 1,
        });
        for person in &sim.host.state.people {
            game_society::insert_knowledge(
                &mut sim.host.storage,
                person.id,
                game_society::Knowledge::default(),
            )?;
        }
        Ok(sim)
    }

    pub(crate) fn social_area(&self, actor: PersonId) -> Result<AreaId, Error> {
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
            .map_or_else(Vec::new, |world| {
                world.institution_views(&self.host.storage, actor)
            })
    }
    pub fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView> {
        self.host
            .state
            .institutions
            .as_ref()
            .map_or_else(Vec::new, |world| {
                world.social_case_views(&self.host.storage, actor)
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
        self.host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let m =
            game_society::knowledge_mut(&mut self.host.storage, actor).ok_or(Error::Unavailable)?;
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
            alive: game_society::institutions(&self.host.storage)
                .map(|(_, row)| row)
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let recipient_area = if let InstitutionAction::TellCase { recipient, .. } = action {
            self.social_area(*recipient).ok()
        } else {
            None
        };
        let refusal = w.case_command_record(&self.host.storage, action, self.time());
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
            &self.host.storage,
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
        let effects = plan.commit(
            self.host.state.institutions.as_ref().unwrap(),
            &mut self.host.storage,
        )?;
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
        let Some(runtime) = &self.host.state.institutions else {
            return Ok(());
        };
        runtime.preflight_rule_deadlines(&self.host.storage, at)
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
                } => match game_agreements::obligation(&self.host.storage, obligation)
                    .unwrap()
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
                    let term = &game_agreements::obligation(&self.host.storage, obligation)
                        .unwrap()
                        .term;
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
                } if game_property::inventory_custodian(&self.host.storage, output)
                    .is_some_and(|owner| owner != actor) =>
                {
                    Some(CaseReceipt::Production { actor, site })
                }
                EventKind::Ate { actor, source, .. }
                    if game_society::exclusions(&self.host.storage, actor)
                        .is_some_and(|stores| stores.stores.contains_key(&source)) =>
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
                    && game_society::exclusions(&self.host.storage, actor)
                        .is_some_and(|stores| stores.stores.contains_key(&source)) =>
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
        world.closing_resource_demand(
            &self.host.storage,
            population,
            review,
            ledger,
            pending,
            direct,
        )
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
            alive: game_society::institutions(&self.host.storage)
                .map(|(_, row)| row)
                .flat_map(|i| self.institution_authority_facts(i).alive)
                .collect(),
        };
        world.allows_access(&self.host.storage, actor, inventory, &authority)
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
    fn social_observer_facts_for(
        &self,
        people: impl IntoIterator<Item = PersonId>,
    ) -> Vec<game_society::perception::ObserverFacts> {
        people
            .into_iter()
            .map(|actor| game_society::perception::ObserverFacts {
                actor,
                area: match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
                {
                    Some(Location::AtArea(area)) => Some(*area),
                    _ => None,
                },
                alive: game_physiology::body(&self.host.storage, actor)
                    .is_some_and(|b| b.vitals.alive),
                independent: self.independent(actor),
                conversation_allowed: !self.combat_restricts(actor),
                empathy: game_characters::decisions::mind::<Command>(&self.host.storage, actor)
                    .expect("known institutional person")
                    .personality
                    .empathy,
            })
            .collect()
    }
    pub(crate) fn prepare_institution_publications(
        &self,
        plan: &game_society::institutions::PreparedCommand,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
    ) -> Result<Vec<game_society::projected_publication::PreparedCommandPublication>, Error> {
        let mut people = BTreeSet::new();
        let mut authority_people = BTreeSet::new();
        for effect in &plan.effects {
            if let CommandEffect::Publish { institution } = effect {
                let i = plan.publication_institution(&self.host.storage, *institution)?;
                people.extend(
                    plan.publication_people(
                        &self.host.storage,
                        *institution,
                        positional
                            .get(&i.charter.jurisdiction)
                            .into_iter()
                            .flatten()
                            .copied(),
                    )?,
                );
                authority_people
                    .extend(plan.publication_authority_people(&self.host.storage, *institution)?);
            }
        }
        let roster = self.social_observer_facts_for(people);
        let index = game_society::perception::ObservationIndex::new(&roster)?;
        let authority = game_society::institutions::AuthorityFacts {
            alive: authority_people
                .into_iter()
                .filter(|id| {
                    game_physiology::body(&self.host.storage, *id).is_some_and(|b| b.vitals.alive)
                })
                .collect(),
        };
        plan.prepare_publications(
            self.host
                .state
                .institutions
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &self.host.storage,
            &authority,
            self.time(),
            &index,
        )
    }
    pub(crate) fn publish_institution(&mut self, id: InstitutionId) {
        let roster = self.social_observer_facts();
        let index = game_society::perception::ObservationIndex::new(&roster)
            .expect("unique observer roster");
        let world = self.host.state.institutions.as_ref().unwrap();
        let authority = self.institution_authority_facts(
            game_society::institution(&self.host.storage, id).expect("validated Society reference"),
        );
        let publication = world
            .prepare_publication(&self.host.storage, id, &authority, self.time(), &index)
            .expect("known institution");
        publication
            .commit(
                self.host.state.institutions.as_ref().unwrap(),
                &mut self.host.storage,
            )
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
                                custodian: self.host.state.agreements.as_ref().and_then(|_| {
                                    game_property::inventory_custodian(&self.host.storage, *id)
                                }),
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
    pub(crate) fn prepare_institution_transition(
        &self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
    ) -> Result<game_society::institutions::PreparedCommand, Error> {
        let world = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let request = world.command_references(&self.host.storage, actor, action)?;
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
            } => Some((
                &game_society::institution(&self.host.storage, institution)
                    .expect("validated Society reference"),
                change,
                true,
            )),
            InstitutionAction::Vote { proposal, .. } => {
                let proposal = &game_society::proposal(&self.host.storage, proposal)
                    .expect("validated Society reference");
                Some((
                    &game_society::institution(&self.host.storage, proposal.institution)
                        .expect("validated Society reference"),
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
        world.prepare_command(
            &self.host.storage,
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
        )
    }
    fn execute_institution_transition(
        &mut self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let positional = self.realm_positional_membership()?;
        self.execute_institution_transition_with_positions(actor, area, action, &positional)
    }
    pub(crate) fn execute_institution_transition_with_positions(
        &mut self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
    ) -> Result<(), Error> {
        let plan = self.prepare_institution_transition(actor, area, action)?;
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
                game_agreements::Ledger::new(&self.host.storage)
                    .prepare_institutional_duties(&duties, self.host.clock().time)?,
            )
        };
        let mut regional_plans = Vec::new();
        let mut regional_events = Vec::new();
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
                        if let Some((change, event)) =
                            self.prepare_realm_outcome(*institution, *proposal, *outcome)?
                        {
                            regional_plans.push(change);
                            regional_events.push((*institution, *proposal, event));
                        }
                        self.ensure_event_capacity(1)?;
                    }
                    _ => self.preflight_enforcement_adoption(*institution, *proposal)?,
                }
            }
        }
        plan.verify(&self.host.storage)?;
        let effects = plan.effects.clone();
        let next_id = plan.next_id;
        if let Some(funding) = &funding {
            funding.check(&game_agreements::Ledger::new(&self.host.storage))?;
        }
        let observe = effects
            .iter()
            .any(|e| matches!(e, CommandEffect::ObserveRegional));
        let publications = self.prepare_institution_publications(&plan, positional)?;
        let regional = self.prepare_realm_institution_reaction(
            &publications,
            regional_plans,
            observe,
            positional,
        )?;
        let mut batch = game_ecs::Batch::new();
        let publication_guard = plan.stage_with_publications(
            self.host.state.institutions.as_ref().unwrap(),
            &self.host.storage,
            publications,
            &mut batch,
        )?;
        let regional_guard = regional
            .map(|p| p.stage_into(&self.host.storage, &publication_guard, &mut batch))
            .transpose()?;
        if let Some(funding) = funding {
            funding.stage(&mut batch);
        }
        publication_guard.validate(&batch)?;
        if let Some(guard) = regional_guard {
            guard.validate(&batch)?;
        }
        self.host.storage.apply_batch(batch)?;
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
                    outcome: _,
                    change,
                } => match change {
                    InstitutionChange::RatifyRegional { .. }
                    | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                        if let Some((_, _, event)) = regional_events
                            .iter()
                            .find(|(i, p, _)| *i == institution && *p == proposal)
                        {
                            self.publish_realm_event(event.clone());
                        }
                    }
                    _ => self.adopt_enforcement_change(institution, proposal)?,
                },
                CommandEffect::Publish { .. } => {}
                CommandEffect::ObserveRegional => {
                    let through = self.host.next_event() - 1;
                    if let Some(world) = self.host.state.realm.as_mut() {
                        world.last_event = through;
                    }
                }
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
        let plan = game_agreements::Ledger::new(&self.host.storage).prepare_institutional_duties(
            &[game_agreements::InstitutionDuty {
                institution,
                agreement,
                obligation,
                term,
            }],
            self.host.clock().time,
        )?;
        plan.commit(&mut self.host.storage)?;
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
        let holders = game_society::institutions(&self.host.storage)
            .map(|(_, row)| row)
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
                known_stocks: &game_characters::decisions::stocks(&self.host.storage, *actor)
                    .expect("mind exists")
                    .known,
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
            alive: game_society::institutions(&self.host.storage)
                .map(|(_, row)| row)
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let applications = world.rule_applications(
            &self.host.storage,
            &authority,
            &speakers,
            &locations,
            self.time(),
        )?;
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
                            .apply_access_intent(&mut self.host.storage, &intent)?
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
                            .register_duty(&mut self.host.storage, &intent, agreement)?;
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
            .commit_case_reaction(&mut self.host.storage, reaction, id, &observers)?;
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
                    &game_agreements::obligation(&self.host.storage, obligation).unwrap();
                Some(SocialReceipt::Performed {
                    actor,
                    counterpart: obligation.term.parties()[1],
                    agreement: obligation.agreement,
                    target: target(&obligation.term)?,
                })
            }
            EventKind::ObligationDefaulted { obligation } => {
                let obligation =
                    &game_agreements::obligation(&self.host.storage, obligation).unwrap();
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
                    custodian: game_property::inventory_custodian(&self.host.storage, output),
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
        if game_society::knowledges(&self.host.storage).count() != self.host.state.people.len() {
            for person in &self.host.state.people {
                if game_society::knowledge(&self.host.storage, person.id).is_none() {
                    game_society::insert_knowledge(
                        &mut self.host.storage,
                        person.id,
                        game_society::Knowledge::default(),
                    )?;
                }
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
                .receipt_reaction(
                    &self.host.storage,
                    &receipt,
                    subject,
                    self.time(),
                    &receipt_index,
                )
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
        for (actor, _) in game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
        {
            let Ok(area) = self.social_area(actor) else {
                continue;
            };
            let mut subject = self.case_subject_facts(actor);
            subject.area = Some(area);
            reactions.extend(
                world.exclusion_reactions(
                    &self.host.storage,
                    subject,
                    &game_characters::decisions::stocks(&self.host.storage, actor)
                        .expect("stock knowledge exists")
                        .known,
                    self.time(),
                ),
            );
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
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .expect("stock knowledge exists");
        InstitutionalVotingKnowledge {
            stocks: &stocks.known,
            store: &self.host.storage,
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
    fn hear_known_cases(&mut self) -> Result<(), Error> {
        let world = self.host.state.institutions.as_ref().unwrap();
        let holders = game_society::institutions(&self.host.storage)
            .map(|(_, row)| row)
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
            alive: game_society::institutions(&self.host.storage)
                .map(|(_, row)| row)
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        let choices = world.hearing_intentions(&self.host.storage, &authority, &actors)?;
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
        let founders = game_society::institutions(&self.host.storage)
            .map(|(_, row)| row)
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
        let mut replies = w.formation_replies(&self.host.storage, &facts)?;
        let voters = self
            .host
            .state
            .people
            .iter()
            .map(|person| (person.id, self.institutional_vote_knowledge(person.id)))
            .collect::<Vec<_>>();
        let voter_facts = voters
            .iter()
            .map(|(actor, knowledge)| self.institutional_vote_facts(*actor, knowledge))
            .collect::<Vec<_>>();
        let authority = game_society::institutions::AuthorityFacts {
            alive: game_society::institutions(&self.host.storage)
                .flat_map(|(_, i)| self.institution_authority_facts(i).alive)
                .collect(),
        };
        let requests = w.proposal_vote_requests(&self.host.storage, &authority, &voter_facts)?;
        let proposers = game_society::proposals(&self.host.storage)
            .map(|(_, p)| p.proposer)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|actor| game_society::proposal_voting::ProposerFacts {
                actor,
                area: self.social_area(actor).ok(),
                social_penalty: self.invocation_social_penalty(actor),
            })
            .collect::<Vec<_>>();
        let mut foreign = Vec::new();
        for (proposal, actor) in requests {
            let p = game_society::proposal(&self.host.storage, proposal).expect("proposal exists");
            let i = game_society::institution(&self.host.storage, p.institution)
                .expect("institution exists");
            let score = match &p.change {
                InstitutionChange::RatifyRegional { .. }
                | InstitutionChange::AuthorizeRegionalPassage { .. } => {
                    Some(self.realm_vote_score(actor, &p.change))
                }
                InstitutionChange::EnforcementPolicy { .. }
                | InstitutionChange::Adjudicate { .. } => {
                    Some(self.enforcement_vote_score(actor, i, &p.change))
                }
                _ => None,
            };
            if let Some(score) = score {
                foreign.push(game_society::proposal_voting::ForeignVoteJudgment {
                    proposal,
                    actor,
                    score,
                });
            }
        }
        replies.extend(w.proposal_replies(
            &self.host.storage,
            &authority,
            &voter_facts,
            &proposers,
            &foreign,
        )?);
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
            .duty_workers(&self.host.storage)
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
            .duty_work_intentions(&self.host.storage, &workers)
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
            let request =
                world.discovery_references(&self.host.storage, actor, area, &sites, recruits);
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
            let decision = world.discovery_decision(
                &self.host.storage,
                DiscoveryFacts {
                    voter: &voter,
                    now: self.time(),
                    authority: &authority,
                    references: &references,
                    sites: &sites,
                    recruits,
                },
            );
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
                .record_discovery_attempt(&mut self.host.storage, actor, &decision, accepted)?;
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
            alive: game_society::institutions(&self.host.storage)
                .map(|(_, row)| row)
                .flat_map(|institution| self.institution_authority_facts(institution).alive)
                .collect(),
        };
        world
            .institution_opportunities(&self.host.storage, actor, area, &authority)
            .into_iter()
            .map(|action| Command::Institution { action })
            .collect()
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &s.state.institutions else {
        if game_society::institutions(s.components).next().is_some()
            || game_society::proposals(s.components).next().is_some()
            || game_society::concepts(s.components).next().is_some()
            || game_society::cases(s.components).next().is_some()
            || game_society::knowledges(s.components).next().is_some()
            || game_society::request_rows(s.components).next().is_some()
            || game_society::exclusion_rows(s.components).next().is_some()
            || game_society::hearings(s.components).next().is_some()
        {
            return Err(Error::InvalidSave(
                "society components require their runtime".into(),
            ));
        }
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid institution state".into());
    if s.state.physical.is_none() || s.state.decisions.is_none() {
        return Err(fail());
    }
    s.state.agreements.as_ref().ok_or_else(fail)?;
    let request = runtime.validation_references(s.components);
    let people = s.state.people.iter().map(|p| p.id).collect::<BTreeSet<_>>();
    let areas = s.state.areas.iter().map(|a| a.id).collect::<BTreeSet<_>>();
    let references = ReferenceFacts {
        people: request.people.intersection(&people).copied().collect(),
        eligible_locations: BTreeMap::new(),
        stocks: request
            .stocks
            .iter()
            .filter_map(|id| {
                game_cargo::inventory(s.components, *id).map(|row| {
                    (
                        *id,
                        StockReference {
                            area: None,
                            custodian: None,
                            good: row.good,
                            capacity: row.capacity,
                        },
                    )
                })
            })
            .collect(),
        sites: request
            .sites
            .iter()
            .filter_map(|id| game_workshop::site(s.components, *id).map(|row| (*id, row.area)))
            .collect(),
    };
    runtime.validate(
        s.components,
        &game_society::save_validation::ValidationFacts {
            at: s.time,
            next_id: s.next_id,
            next_event: s.next_event,
            areas: &areas,
            references: &references,
        },
    )
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
            game_agreements::insert_agreement(
                &mut sim.host.storage,
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
            )
            .unwrap();
            game_agreements::insert_obligation(
                &mut sim.host.storage,
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
            )
            .unwrap();
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
            game_society::institutions(&sim.host.storage)
                .next()
                .is_none()
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
        assert!(game_society::cases(&sim.host.storage).next().is_none());
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
        assert_eq!(game_society::cases(&sim.host.storage).count(), 1000);
        assert!(
            game_agreements::obligations(&sim.host.storage)
                .map(|(_, row)| row)
                .all(|obligation| obligation.defaulted)
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}

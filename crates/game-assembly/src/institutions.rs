//! Institutions are supported arrangements of people and scoped authority.
//! Their rules create permissions and recognized duties, never physical payment.
use crate::*;
use game_society::institutions::{
    CommandEffect, CommandFacts, CommandIds, ExternalChangePermit, ReferenceFacts,
    ReferenceRequest, StockReference,
};
pub use game_society::institutions::{Institution, RuleProposal, Runtime};

fn bind_institution_enforcement_event(event: game_society::enforcement::Event) -> EventKind {
    use game_society::enforcement::Event;
    match event {
        Event::EnforcementPolicyChanged {
            institution,
            office,
        } => EventKind::EnforcementPolicyChanged {
            institution,
            office,
        },
        Event::EnforcementDecided {
            case,
            judgment,
            proposer,
        } => EventKind::EnforcementDecided {
            case,
            judgment,
            proposer,
        },
        Event::EnforcementFiling {
            case,
            actor,
            recipient,
        } => EventKind::EnforcementFiling {
            case,
            actor,
            recipient,
        },
        Event::EnforcementResponded {
            case,
            actor,
            comply,
        } => EventKind::EnforcementResponded {
            case,
            actor,
            comply,
        },
        Event::EnforcementFiled {
            case,
            actor,
            institution,
        } => EventKind::EnforcementFiled {
            case,
            actor,
            institution,
        },
        Event::EnforcementReported {
            case,
            sender,
            recipient,
        } => EventKind::EnforcementReported {
            case,
            sender,
            recipient,
        },
        Event::EnforcementDispatched {
            case,
            actor,
            mission,
        } => EventKind::EnforcementDispatched {
            case,
            actor,
            mission,
        },
        Event::EnforcementOutcome { case, status } => {
            EventKind::EnforcementOutcome { case, status }
        }
    }
}
fn bind_institution_realm_event(event: game_realm::RealmEvent) -> EventKind {
    use game_realm::RealmEvent;
    match event {
        RealmEvent::Proposed {
            commitment,
            actor,
            revision,
        } => EventKind::RealmProposed {
            commitment,
            actor,
            revision,
        },
        RealmEvent::Ratified {
            commitment,
            revision,
            institution,
            proposal,
            adopted,
        } => EventKind::RealmRatified {
            commitment,
            revision,
            institution,
            proposal,
            adopted,
        },
        RealmEvent::WorkRequested { key, actor } => EventKind::RealmWorkRequested { key, actor },
        RealmEvent::Reported {
            commitment,
            sender,
            recipient,
        } => EventKind::RealmReported {
            commitment,
            sender,
            recipient,
        },
        RealmEvent::Interrupted { actor } => EventKind::Interrupted { actor },
    }
}

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
        let owner = sim.person_at(3).id;
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
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            game_society::insert_knowledge(
                &mut sim.host.storage,
                person_id,
                game_society::Knowledge::default(),
            )?;
        }
        Ok(sim)
    }

    pub(crate) fn social_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        game_society::review::social_area(game_society::review::SocialAreaFacts {
            area: match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            },
            independent: self.independent(actor),
            combat_restricted: self.combat_restricts(actor),
            alive: self
                .host
                .state
                .physical
                .as_ref()
                .and_then(|_| game_physiology::body(&self.host.storage, actor))
                .is_some_and(|body| body.vitals.alive),
        })
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
            .ok_or(Error::Unavailable)?
            .clear_learning(&mut self.host.storage, actor, values, precedents)
    }

    /// The public command and its pending institutional receipts share one admission.
    pub(crate) fn routed_institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        let area = self.social_area(actor)?;
        self.preflight_institutions()?;
        let world = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let references = world.command_references(&self.host.storage, actor, action)?;
        let positional = self.realm_positional_membership()?;
        let mut extra = references
            .people
            .iter()
            .map(|id| game_ecs::StableId(id.0))
            .collect::<BTreeSet<_>>();
        extra.extend(references.stocks.iter().map(|id| game_ecs::StableId(id.0)));
        extra.extend(references.sites.iter().map(|id| game_ecs::StableId(id.0)));
        let mut affected = BTreeSet::new();
        let case_action = matches!(
            action,
            InstitutionAction::HearCase { .. } | InstitutionAction::TellCase { .. }
        );
        let mut enforcement_observation = false;
        let publications = if case_action {
            if let InstitutionAction::HearCase {
                institution, case, ..
            } = action
            {
                affected.insert(*institution);
                extra.insert(game_ecs::StableId(case.0));
                let current = game_society::institution(&self.host.storage, *institution)
                    .ok_or(Error::Unavailable)?;
                let roster = self.social_observer_facts();
                let index = game_society::perception::ObservationIndex::new(&roster)?;
                let authority = self.institution_authority_facts(current);
                let refusal = world.case_command_record(&self.host.storage, action, self.time());
                let observers = refusal
                    .as_ref()
                    .map(|case| index.case_observers(case, false))
                    .transpose()?
                    .unwrap_or_default();
                extra.extend(observers.keys().map(|id| game_ecs::StableId(id.0)));
                let case_plan = world.prepare_case_command(
                    &self.host.storage,
                    action,
                    game_society::case_commands::CaseCommandFacts {
                        actor,
                        area,
                        now: self.time(),
                        recipient_area: None,
                        authority: &authority,
                        refusal: refusal.map(|_| CaseId(self.host.next_id())),
                        refusal_observers: &observers,
                    },
                )?;
                let publications = case_plan.prepare_publications(
                    world,
                    &self.host.storage,
                    &authority,
                    self.time(),
                    &index,
                )?;
                extra.extend(
                    publications
                        .iter()
                        .flat_map(|p| p.recipients())
                        .map(|id| game_ecs::StableId(id.0)),
                );
            } else if let InstitutionAction::TellCase { case, .. } = action {
                extra.insert(game_ecs::StableId(case.0));
            }
            None
        } else {
            let plan = self.prepare_institution_transition(actor, area, action)?;
            enforcement_observation = plan.effects.iter().any(|effect| {
                matches!(
                    effect,
                    CommandEffect::ExternalOutcome {
                        outcome: RuleProposalStatus::Adopted,
                        change: InstitutionChange::EnforcementPolicy { .. }
                            | InstitutionChange::Adjudicate { .. },
                        ..
                    }
                )
            });
            let publications = self.prepare_institution_publications(&plan, &positional)?;
            for publication in &publications {
                affected.insert(publication.institution());
            }
            extra.extend(
                publications
                    .iter()
                    .flat_map(|p| p.recipients())
                    .map(|id| game_ecs::StableId(id.0)),
            );
            for id in &affected {
                extra.extend(
                    plan.publication_authority_people(&self.host.storage, *id)?
                        .into_iter()
                        .map(|id| game_ecs::StableId(id.0)),
                );
            }
            Some(publications)
        };
        match action {
            InstitutionAction::Join { institution }
            | InstitutionAction::Decline { institution }
            | InstitutionAction::Leave { institution }
            | InstitutionAction::SupportOffice { institution, .. }
            | InstitutionAction::Propose { institution, .. }
            | InstitutionAction::HearCase { institution, .. } => {
                affected.insert(*institution);
            }
            InstitutionAction::Vote { proposal, .. } => {
                affected.insert(
                    game_society::proposal(&self.host.storage, *proposal)
                        .ok_or(Error::Unavailable)?
                        .institution,
                );
                extra.insert(game_ecs::StableId(proposal.0));
            }
            _ => {}
        }
        extra.extend(affected.iter().map(|id| game_ecs::StableId(id.0)));
        extra.extend(
            game_realm::commitments(&self.host.storage)
                .flat_map(|(_, commitment)| &commitment.revisions)
                .flat_map(|revision| revision.pending.values())
                .map(|row| game_ecs::StableId(row.actor.0)),
        );
        let origin = game_ecs::Batch::new();
        let mut context = self.prepare_physical_maintenance_context(&origin, &[])?;
        context.include_institution_source(self, &affected);
        if enforcement_observation {
            context.include_enforcement_observation();
        }
        let contacts = self.physical_maintenance_contacts(&origin)?;
        let followups = self.prepare_source_followups_with_context(
            None,
            &origin,
            0,
            0,
            extra,
            &context,
            |preview| {
                if case_action {
                    preview.institution_command(actor, action)
                } else {
                    let people = context.enforcement_people(preview);
                    let assaults = context.enforcement_assaults(preview);
                    preview.execute_institution_transition_prepared(
                        actor,
                        area,
                        action,
                        &positional,
                        publications,
                        Some(&people),
                        Some(&contacts),
                        Some(&assaults),
                    )
                }
            },
            |preview| {
                let mut batch = game_ecs::Batch::new();
                let receipt = preview.prepare_physical_institution_maintenance_with_context(
                    &context,
                    &mut batch,
                    &[],
                    0,
                )?;
                receipt.validate(preview, &batch)?;
                let mut transaction =
                    game_world::Transaction::new(preview.host.transaction_opening(), batch);
                transaction.ids = receipt.ids;
                transaction.events = receipt.events.clone();
                transaction.guards = receipt.guards();
                preview.host.commit_transaction(transaction)?;
                receipt.finish(preview);
                preview.host.bump_revision(1)
            },
        )?;
        let (transaction, finish) = followups.into_transaction(self)?;
        self.host.commit_transaction(transaction)?;
        finish.finish(self);
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
        let opening = self.host.transaction_opening();
        let now = self.time();
        let publications =
            plan.prepare_publications(w, &self.host.storage, &authority, now, &index)?;
        let positional = self.realm_positional_membership()?;
        let regional =
            self.prepare_realm_institution_reaction(&publications, Vec::new(), false, &positional)?;
        let mut batch = game_ecs::Batch::new();
        let publication =
            plan.stage_with_publications(w, &self.host.storage, now, publications, &mut batch)?;
        let regional = regional
            .map(|plan| {
                plan.stage_into(
                    &self.host.storage,
                    publication.publication_guard(),
                    &mut batch,
                )
            })
            .transpose()?;
        publication.validate(w, &self.host.storage, now, &batch)?;
        if let Some(regional) = &regional {
            regional.validate(&batch)?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        if let Some((case, actor)) = publication.effects.recorded {
            transaction.ids = 1;
            transaction.events = vec![EventKind::SocialCaseRecorded { case, actor }];
        }
        transaction.guards.push(publication.row_guard());
        self.host.commit_transaction(transaction)?;
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
        let receipts = self
            .host
            .events_since(world.last_event)
            .iter()
            .filter_map(|event| self.social_receipt(&event.event).transpose())
            .map(|receipt| {
                let receipt = receipt?;
                let actor = game_society::review::receipt_actor(&receipt);
                Ok((receipt, self.case_subject_facts(actor)))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let pending =
            game_society::institutions::pending_case_receipt_demand(&self.host.storage, &receipts)?;
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
        let needed = game_society::review::authority_people(i);
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
        self.people()
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
        self.execute_institution_transition_prepared(
            actor, area, action, positional, None, None, None, None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn execute_institution_transition_prepared(
        &mut self,
        actor: PersonId,
        area: AreaId,
        action: &InstitutionAction,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
        publications: Option<Vec<game_society::projected_publication::PreparedCommandPublication>>,
        enforcement_people: Option<&[game_society::enforcement::PersonFacts]>,
        agreement_contacts: Option<&game_agreements::ContactSet>,
        enforcement_assaults: Option<&[game_society::enforcement::WitnessedAssault]>,
    ) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
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
                && matches!(
                    change,
                    InstitutionChange::RatifyRegional { .. }
                        | InstitutionChange::AuthorizeRegionalPassage { .. }
                )
            {
                if let Some((change, event)) =
                    self.prepare_realm_outcome(*institution, *proposal, *outcome)?
                {
                    regional_plans.push(change);
                    regional_events.push((*institution, *proposal, event));
                }
                self.ensure_event_capacity(1)?;
            }
        }
        plan.verify(&self.host.storage)?;
        let effects = plan.effects.clone();
        let mut next_id = plan.next_id;
        let opening_plan = plan.clone();
        if let Some(funding) = &funding {
            funding.check(&game_agreements::Ledger::new(&self.host.storage))?;
        }
        let observe = effects
            .iter()
            .any(|e| matches!(e, CommandEffect::ObserveRegional));
        let publications = publications.map_or_else(
            || self.prepare_institution_publications(&plan, positional),
            Ok,
        )?;
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
        let funding_guard = funding
            .as_ref()
            .map(|funding| funding.stage_into(&self.host.storage, &mut batch))
            .transpose()?;
        let mut enforcement_publications = BTreeMap::new();
        for effect in &effects {
            if let CommandEffect::ExternalOutcome {
                proposal,
                outcome: RuleProposalStatus::Adopted,
                change,
                ..
            } = effect
                && matches!(
                    change,
                    InstitutionChange::EnforcementPolicy { .. }
                        | InstitutionChange::Adjudicate { .. }
                )
            {
                let proposal_row = batch
                    .projected::<RuleProposal>(&self.host.storage, game_ecs::StableId(proposal.0))?
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                let count = Self::enforcement_change_ids(change);
                let judgment = (count > 0).then_some(JudgmentId(next_id));
                next_id = next_id.checked_add(count).ok_or(Error::Overflow)?;
                let receiver = game_society::enforcement::prepare_adoption(
                    &self.host.storage,
                    self.time(),
                    &proposal_row,
                    judgment,
                )?;
                let receiver = receiver.stage(&self.host.storage, &mut batch)?;
                enforcement_publications.insert(*proposal, receiver);
            }
        }
        let enforcement_observation = if enforcement_publications.is_empty() {
            None
        } else {
            let receipts = enforcement_publications.values().collect::<Vec<_>>();
            Some(self.prepare_institution_enforcement_observation(
                &mut batch,
                &receipts,
                enforcement_people,
                enforcement_assaults,
            )?)
        };
        opening_plan.verify(&self.host.storage)?;
        publication_guard.validate(&batch)?;
        if let Some(guard) = regional_guard {
            guard.validate(&batch)?;
        }
        if let Some(guard) = &funding_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        for receiver in enforcement_publications.values() {
            receiver.validate(&self.host.storage, &batch)?;
        }
        if let Some(observation) = &enforcement_observation {
            observation.validate(
                self.host.state.enforcement.as_ref(),
                &self.host.storage,
                &batch,
            )?;
        }
        let mut events = Vec::new();
        let mut enforcement_changed = false;
        for effect in effects {
            let event = match effect {
                CommandEffect::Funding { agreement, .. } => {
                    Some(EventKind::AgreementActivated { agreement })
                }
                CommandEffect::Proposed {
                    institution,
                    proposer,
                } => Some(EventKind::InstitutionProposed {
                    institution,
                    proposer,
                }),
                CommandEffect::Founded { institution } => {
                    Some(EventKind::InstitutionFounded { institution })
                }
                CommandEffect::Left { institution, actor } => {
                    Some(EventKind::InstitutionLeft { institution, actor })
                }
                CommandEffect::OfficeSupportChanged {
                    institution,
                    office,
                    actor,
                    support,
                } => Some(EventKind::OfficeSupportChanged {
                    institution,
                    office,
                    actor,
                    support,
                }),
                CommandEffect::Changed {
                    institution,
                    proposal,
                } => Some(EventKind::InstitutionChanged {
                    institution,
                    proposal,
                }),
                CommandEffect::ExternalOutcome {
                    institution,
                    proposal,
                    change,
                    ..
                } => {
                    if matches!(
                        change,
                        InstitutionChange::RatifyRegional { .. }
                            | InstitutionChange::AuthorizeRegionalPassage { .. }
                    ) {
                        regional_events
                            .iter()
                            .find(|(i, p, _)| *i == institution && *p == proposal)
                            .map(|(_, _, event)| bind_institution_realm_event(event.clone()))
                    } else {
                        let receipt = enforcement_publications
                            .remove(&proposal)
                            .ok_or(Error::Unavailable)?;
                        enforcement_changed = true;
                        receipt.finish().map(bind_institution_enforcement_event)
                    }
                }
                CommandEffect::Publish { .. } | CommandEffect::ObserveRegional => None,
            };
            if let Some(event) = event {
                events.push(event);
            }
        }
        let agreement_receivers = funding
            .as_ref()
            .map(|funding| match agreement_contacts {
                Some(contacts) => self.stage_agreement_receivers_with_contacts(
                    actor, funding, &events, &mut batch, contacts,
                ),
                None => self.stage_agreement_receivers(actor, funding, &events, &mut batch),
            })
            .transpose()?;
        publication_guard.validate(&batch)?;
        if let Some(guard) = &funding_guard {
            guard.validate(&self.host.storage, &batch)?;
        }
        if let Some(receivers) = &agreement_receivers {
            receivers.validate(self, &batch)?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = next_id
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        transaction.events = events;
        if let Some(guard) = funding_guard {
            transaction.guards.push(guard);
        }
        if let Some(receivers) = &agreement_receivers {
            transaction.guards.extend(receivers.guards.iter().cloned());
        }
        self.host.commit_transaction(transaction)?;
        if let Some(receivers) = agreement_receivers {
            receivers.finish(self);
        }
        let through = self.host.next_event() - 1;
        if observe && let Some(world) = self.host.state.realm.as_mut() {
            world.last_event = through;
        }
        if enforcement_changed {
            self.host.state.enforcement = Some(
                enforcement_observation
                    .expect("prepared enforcement observation")
                    .finish(through),
            );
        }
        Ok(())
    }

    fn prepare_institution_enforcement_observation(
        &self,
        batch: &mut game_ecs::Batch,
        receipts: &[&game_society::enforcement::AdoptionPublication],
        supplied: Option<&[game_society::enforcement::PersonFacts]>,
        supplied_assaults: Option<&[game_society::enforcement::WitnessedAssault]>,
    ) -> Result<game_society::enforcement::ObservationPublication, Error> {
        use game_society::enforcement::{AdoptionObservationFacts, PersonFacts};
        let mut reads = game_ecs::ReadSet::new();
        reads.membership::<game_characters::Person>(&self.host.storage, |_, _| true);
        let mut people = Vec::new();
        if let Some(supplied) = supplied {
            people.extend_from_slice(supplied);
        } else {
            for person in self.people() {
                let id = game_ecs::StableId(person.id.0);
                reads.read::<game_characters::Person>(&self.host.storage, id);
                let body = reads.read::<game_physiology::Body>(&self.host.storage, id);
                let location = reads.read::<Location>(&self.host.storage, id);
                if let (Some(body), Some(location)) = (body, location) {
                    people.push(PersonFacts {
                        id: person.id,
                        inventory: person.inventory,
                        location: location.clone(),
                        vitals: body.vitals.clone(),
                        independent: self.independent(person.id),
                        combat_restricted: self.combat_restricts(person.id),
                    });
                }
            }
        }
        let encounters = self
            .host
            .events()
            .iter()
            .filter_map(|event| {
                if let EventKind::Struck { encounter, .. } = event.event {
                    Some(encounter)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        for encounter in encounters {
            reads.read::<game_conflict::Encounter>(
                &self.host.storage,
                game_ecs::StableId(encounter.0),
            );
        }
        let assaults = supplied_assaults.map_or_else(
            || self.enforcement_assault_facts(),
            |assaults| assaults.to_vec(),
        );
        game_society::enforcement::stage_adoption_observation(
            &self.host.storage,
            batch,
            receipts,
            AdoptionObservationFacts {
                now: self.time(),
                people: &people,
                assaults: &assaults,
                last_event: self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .map(|world| world.last_event),
            },
            reads,
        )
    }

    fn commit_institution_rule_application(
        &mut self,
        application: &game_society::deliberation::RuleApplications,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
    ) -> Result<(), Error> {
        use game_society::rule_publication::RuleApplicationEffect;
        let opening = self.host.transaction_opening();
        let now = self.time();
        let runtime = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let plan = runtime.prepare_rule_application(
            &self.host.storage,
            application,
            opening.next_id,
            now,
        )?;
        let effects = plan.effects.clone();
        let next_id = plan.next_id;
        let agreements = plan.agreement_transition().cloned();
        let people = plan.publication_people(
            positional
                .get(&plan.institution().charter.jurisdiction)
                .into_iter()
                .flatten()
                .copied(),
        );
        let roster = self.social_observer_facts_for(people);
        let index = game_society::perception::ObservationIndex::new(&roster)?;
        let authority = self.institution_authority_facts(plan.institution());
        let publications =
            plan.prepare_publications(runtime, &self.host.storage, &authority, now, &index)?;
        let regional =
            self.prepare_realm_institution_reaction(&publications, Vec::new(), false, positional)?;
        let mut batch = game_ecs::Batch::new();
        let publication = plan.stage_with_publications(
            runtime,
            &self.host.storage,
            now,
            publications,
            &mut batch,
        )?;
        let regional = regional
            .map(|plan| {
                plan.stage_into(
                    &self.host.storage,
                    publication.publication_guard(),
                    &mut batch,
                )
            })
            .transpose()?;
        publication.validate(runtime, &self.host.storage, now, &batch)?;
        if let Some(regional) = &regional {
            regional.validate(&batch)?;
        }
        let mut events = Vec::new();
        for effect in effects {
            match effect {
                RuleApplicationEffect::Access {
                    institution,
                    index,
                    actor,
                } => events.push(EventKind::InstitutionRuleApplied {
                    institution,
                    rule: index,
                    actor,
                    agreement: None,
                }),
                RuleApplicationEffect::Duty {
                    institution,
                    index,
                    actor,
                    agreement,
                } => {
                    events.push(EventKind::AgreementActivated { agreement });
                    events.push(EventKind::InstitutionRuleApplied {
                        institution,
                        rule: index,
                        actor,
                        agreement: Some(agreement),
                    });
                }
            }
        }
        let agreement_receivers = agreements
            .as_ref()
            .map(|agreements| {
                let actor = events
                    .iter()
                    .find_map(|event| match event {
                        EventKind::InstitutionRuleApplied {
                            actor,
                            agreement: Some(_),
                            ..
                        } => Some(*actor),
                        _ => None,
                    })
                    .ok_or(Error::Unavailable)?;
                self.stage_agreement_receivers(actor, agreements, &events, &mut batch)
            })
            .transpose()?;
        publication.validate(runtime, &self.host.storage, now, &batch)?;
        if let Some(receivers) = &agreement_receivers {
            receivers.validate(self, &batch)?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = next_id
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        transaction.events = events;
        transaction.guards = publication.row_guards();
        if let Some(receivers) = &agreement_receivers {
            transaction.guards.extend(receivers.guards.iter().cloned());
        }
        self.host.commit_transaction(transaction)?;
        if let Some(receivers) = agreement_receivers {
            receivers.finish(self);
        }
        Ok(())
    }

    fn apply_institution_rules(&mut self) -> Result<(), Error> {
        let world = self.host.state.institutions.as_ref().unwrap();
        let holders = game_society::review::office_holders(&self.host.storage, true);
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
            .people()
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
        let positional = self.realm_positional_membership()?;
        for application in applications {
            self.commit_institution_rule_application(&application, &positional)?;
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
        let opening = self.host.transaction_opening();
        let id = CaseId(opening.next_id);
        let observers = index.case_observers(&reaction.case, reaction.public)?;
        let runtime = self
            .host
            .state
            .institutions
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let plan = runtime.prepare_case_reaction(&self.host.storage, reaction, id, &observers)?;
        let mut batch = game_ecs::Batch::new();
        let publication = plan.stage_into(runtime, &self.host.storage, self.time(), &mut batch)?;
        publication.validate(runtime, &self.host.storage, self.time(), &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = 1;
        transaction.events = vec![EventKind::SocialCaseRecorded {
            case: publication.case,
            actor: publication.actor,
        }];
        transaction.guards.push(publication.row_guard());
        self.host.commit_transaction(transaction)?;
        Ok(id)
    }
    fn social_receipt(
        &self,
        event: &EventKind,
    ) -> Result<Option<game_society::reactions::SocialReceipt>, Error> {
        self.social_receipt_projected(&game_ecs::Batch::new(), event)
    }
    pub(crate) fn reconcile_institutions(&mut self) -> Result<(), Error> {
        let Some(world) = &self.host.state.institutions else {
            return Ok(());
        };
        let cursor = world.last_event;
        let last = self.host.next_event() - 1;
        let actors = self.person_ids.clone();
        game_society::review::initialize_knowledge(&mut self.host.storage, actors)?;
        let receipts = self
            .host
            .events_since(cursor)
            .iter()
            .filter_map(|event| self.social_receipt(&event.event).transpose())
            .collect::<Result<Vec<_>, Error>>()?;
        let receipt_index = game_society::reactions::ReceiptIndex::new(&receipts);
        let roster = self.social_observer_facts();
        let witnesses = game_society::perception::ObservationIndex::new(&roster)?;
        for receipt in receipts {
            let actor = game_society::review::receipt_actor(&receipt);
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
        let facts = game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
            .map(|(actor, _)| game_society::review::ExclusionFacts {
                subject: self.case_subject_facts(actor),
                available_area: self.social_area(actor).ok(),
                known_stocks: &game_characters::decisions::stocks(&self.host.storage, actor)
                    .expect("stock knowledge exists")
                    .known,
            })
            .collect::<Vec<_>>();
        let reactions = world.observed_exclusions(&self.host.storage, &facts, self.time());
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
        let holders = game_society::review::office_holders(&self.host.storage, false);
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
        let founders = game_society::review::formation_voters(&self.host.storage);
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
            .people()
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
        let proposers = game_society::review::proposal_proposers(&self.host.storage)
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
            .people()
            .map(|person| Recruit {
                actor: person.id,
                area: self.social_area(person.id).ok(),
                autonomous: person.id != self.host.state.ruler,
                citizen: person.role == Role::Citizen,
            })
            .collect::<Vec<_>>();
        let index = game_society::review::DiscoveryIndex::new(&roster);
        for (actor, area) in index.actors() {
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
            let recruits = index.recruits(area);
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
    let people = s.people().map(|p| p.id).collect::<BTreeSet<_>>();
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
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            sim.debug_actor_command(person_id, Command::Cancel).unwrap();
            sim.debug_set_personality(
                person_id,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )
            .unwrap();
        }
        let debtor = sim.host.state.ruler;
        let creditor = sim.person_at(3).id;
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
        assert_eq!(sim.people().len(), 15);
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

#[cfg(test)]
mod agreement_receiver_tests {
    use super::*;

    fn strict_receiver_schema() -> Result<game_ecs::Schema, Error> {
        strict_receiver_schema_with_case_rejection(false)
    }
    fn strict_receiver_schema_with_case_rejection(
        reject_cases: bool,
    ) -> Result<game_ecs::Schema, Error> {
        let mut schema = game_ecs::Schema::new();
        game_cargo::register_components(&mut schema)?;
        game_characters::register_people(&mut schema)?;
        game_agriculture::register_components(&mut schema)?;
        game_physiology::register_components(&mut schema)?;
        game_workshop::register_components(&mut schema)?;
        game_skills::register_components(&mut schema)?;
        game_armies::register_components(&mut schema)?;
        game_conflict::register_components(&mut schema)?;
        game_conflict::force::register_components(&mut schema)?;
        game_lineage::register_components(&mut schema)?;
        game_health::register_components(&mut schema)?;
        game_lifecycle::register_components(&mut schema)?;
        game_property::register_components(&mut schema)?;
        game_projects::register_components(&mut schema)?;
        game_invocation::register_components(&mut schema)?;
        game_missions::register_components(&mut schema)?;
        schema.register::<game_agreements::Agreement>(
            "agreements/agreement",
            game_agreements::Agreement::validate,
        )?;
        schema.register::<game_agreements::Obligation>(
            "agreements/obligation",
            game_agreements::Obligation::validate,
        )?;
        schema.register::<game_agreements::Amendment>(
            "agreements/amendment",
            game_agreements::Amendment::validate,
        )?;
        schema.register::<game_agreements::Knowledge>("agreements/knowledge", |knowledge| {
            knowledge.validate()?;
            if !knowledge.agreements.is_empty() {
                return Err(Error::InvalidConfiguration(
                    "strict institution Agreement receiver".into(),
                ));
            }
            Ok(())
        })?;
        schema.register::<game_agreements::Requests>("agreements/requests", |_| Ok(()))?;
        schema.register::<game_agreements::PendingWork>("agreements/pending_work", |work| {
            if work.millis == 0 {
                return Err(Error::InvalidConfiguration("zero agreement work".into()));
            }
            Ok(())
        })?;
        schema.register::<game_agreements::PendingReport>(
            "agreements/report",
            game_agreements::PendingReport::validate,
        )?;
        game_realm::register_components(&mut schema)?;
        game_invocation::register_memory_components::<game_api::Command>(&mut schema)?;
        game_transport::register_components(&mut schema)?;
        if reject_cases {
            schema.register::<Institution>("society/institution", Institution::validate_shape)?;
            schema.register::<RuleProposal>("society/proposal", |_| Ok(()))?;
            schema.register::<game_society::Concept>("society/concept", |row| {
                game_society::validate_rule_shape(&row.rule)
            })?;
            schema.register::<SocialCaseView>("society/case", SocialCaseView::validate_shape)?;
            schema.register::<game_society::Knowledge>("society/knowledge", |row| {
                row.validate_shape()?;
                if !row.cases.is_empty() {
                    return Err(Error::InvalidConfiguration(
                        "strict pending case receiver".into(),
                    ));
                }
                Ok(())
            })?;
            schema.register::<game_society::Requests>("society/requests", |_| Ok(()))?;
            schema.register::<game_society::Exclusions>("society/exclusions", |_| Ok(()))?;
            schema.register::<game_society::Hearing>("society/hearing", |_| Ok(()))?;
        } else {
            game_society::register_components(&mut schema)?;
        }

        game_society::enforcement::register_components(&mut schema)?;
        game_characters::decisions::register_minds::<game_api::Command>(&mut schema)?;
        game_characters::social::register_components(&mut schema)?;
        game_genetics::register_components(&mut schema)?;
        game_physiology::traits::register_components(&mut schema)?;
        game_characters::temperament::register_components(&mut schema)?;
        game_spatial::register_components(&mut schema)?;
        game_environment::register_components(&mut schema)?;
        game_ecology::register_components(&mut schema)?;
        game_land::register_components(&mut schema)?;
        schema.register::<game_api::Activity>("coordination/activity", |_| Ok(()))?;

        Ok(schema)
    }
    fn fixture() -> (Simulation, PersonId, PersonId, AreaId, SiteId, InventoryId) {
        let sim = Simulation::institution_fixture(17, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let recipient = sim.person_at(3).id;
        let area = sim.host.state.areas[0].id;
        let site = game_workshop::sites(&sim.host.storage)
            .find(|(_, site)| site.area == area)
            .unwrap()
            .0;
        let source = game_cargo::inventories(&sim.host.storage)
            .find(|(_, stock)| {
                stock.location == InventoryLocation::Area(area) && stock.good == Good::Food
            })
            .unwrap()
            .0;
        assert!(
            game_agreements::knowledges(&sim.host.storage)
                .all(|(_, knowledge)| knowledge.agreements.is_empty())
        );
        (sim, actor, recipient, area, site, source)
    }
    fn charter(
        actor: PersonId,
        recipient: PersonId,
        area: AreaId,
        site: SiteId,
        source: InventoryId,
        rules: Vec<InstitutionalRule>,
        contributions: Vec<AgreementTerm>,
    ) -> InstitutionCharter {
        InstitutionCharter {
            purpose: "Receiver atomicity".into(),
            jurisdiction: area,
            founders: BTreeSet::from([actor, recipient]),
            open_membership: false,
            procedure: DecisionProcedure {
                proposers: Subjects::members(),
                voters: Subjects::members(),
                minimum_support: 2,
            },
            offices: vec![OfficeMandate {
                slot: 0,
                holder: actor,
                inventories: BTreeSet::from([source]),
                sites: BTreeSet::from([site]),
                powers: BTreeSet::from([OfficePower::Labor, OfficePower::Delivery]),
                minimum_support: 2,
            }],
            rules,
            contributions,
            evidence: BTreeSet::new(),
        }
    }
    fn reject_new_knowledge(sim: &mut Simulation) {
        let restored = game_ecs::Store::restore(
            strict_receiver_schema().unwrap(),
            sim.host.storage.snapshot().unwrap(),
        )
        .unwrap();
        sim.host.storage = restored.into();
    }
    #[test]
    fn successful_funding_receivers_know_the_new_duty_and_track_the_full_origin_cursor() {
        let (mut sim, actor, recipient, area, site, source) = fixture();
        let destination = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([actor, recipient]),
            )
            .unwrap();
        let action = InstitutionAction::Found {
            charter: charter(
                actor,
                recipient,
                area,
                site,
                source,
                Vec::new(),
                vec![AgreementTerm::Delivery {
                    debtor: actor,
                    creditor: recipient,
                    source,
                    destination,
                    quantity: Quantity(1_000),
                    deadline: SimTime(600_000),
                }],
            ),
            origin: None,
        };
        sim.execute_institution_transition(actor, area, &action)
            .unwrap();
        let institution = game_society::institutions(&sim.host.storage)
            .next()
            .unwrap()
            .0;
        let action = InstitutionAction::Join { institution };
        let plan = sim
            .prepare_institution_transition(recipient, area, &action)
            .unwrap();
        assert!(
            plan.effects
                .iter()
                .any(|effect| matches!(effect, CommandEffect::Funding { .. }))
        );
        let opening_event = sim.host.next_event();
        sim.execute_institution_transition(recipient, area, &action)
            .unwrap();
        let agreement = game_agreements::agreements(&sim.host.storage)
            .next()
            .unwrap()
            .0;
        for person in [actor, recipient] {
            assert_eq!(
                game_agreements::known_agreement(&sim.host.storage, person, agreement)
                    .unwrap()
                    .status,
                AgreementStatus::Active
            );
        }
        assert_eq!(
            sim.host
                .state
                .agreements
                .as_ref()
                .unwrap()
                .last_observation_event,
            sim.host.next_event() - 1
        );
        let actual = sim
            .host
            .events()
            .iter()
            .filter(|event| event.sequence >= opening_event)
            .map(|event| &event.event)
            .collect::<Vec<_>>();
        assert!(
            actual
                .iter()
                .any(|event| matches!(event, EventKind::AgreementActivated { .. }))
        );
        assert!(
            actual.len() > 1,
            "observer cursor includes the joint institution events"
        );
    }
    #[test]
    fn funding_receiver_rejection_preserves_native_origin_events_ids_and_all_cursors() {
        let (mut sim, actor, recipient, area, site, source) = fixture();
        let destination = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([actor, recipient]),
            )
            .unwrap();
        let action = InstitutionAction::Found {
            charter: charter(
                actor,
                recipient,
                area,
                site,
                source,
                Vec::new(),
                vec![AgreementTerm::Delivery {
                    debtor: actor,
                    creditor: recipient,
                    source,
                    destination,
                    quantity: Quantity(1_000),
                    deadline: SimTime(600_000),
                }],
            ),
            origin: None,
        };
        sim.execute_institution_transition(actor, area, &action)
            .unwrap();
        let institution = game_society::institutions(&sim.host.storage)
            .next()
            .unwrap()
            .0;
        let action = InstitutionAction::Join { institution };
        let plan = sim
            .prepare_institution_transition(recipient, area, &action)
            .unwrap();
        assert!(
            plan.effects
                .iter()
                .any(|effect| matches!(effect, CommandEffect::Funding { .. }))
        );
        reject_new_knowledge(&mut sim);
        let opening = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.execute_institution_transition(recipient, area, &action),
            Err(Error::InvalidConfiguration(
                "strict institution Agreement receiver".into()
            ))
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), opening);
    }
    #[test]
    fn rule_duty_receiver_rejection_preserves_institution_agreements_cases_and_cursors() {
        let (mut sim, actor, recipient, area, site, source) = fixture();
        let rule = InstitutionalRule {
            subjects: Subjects::named(actor),
            when: RuleExpression::Fact(SocialCondition::Local),
            effect: SocialEffect::Labor {
                site,
                millis: 60_000,
                beneficiary: Beneficiary::Person(recipient),
                due_after: SimDuration(600_000),
            },
            authority: 0,
        };
        sim.execute_institution_transition(
            actor,
            area,
            &InstitutionAction::Found {
                charter: charter(actor, recipient, area, site, source, vec![rule], Vec::new()),
                origin: None,
            },
        )
        .unwrap();
        let institution = game_society::institutions(&sim.host.storage)
            .next()
            .unwrap()
            .0;
        sim.execute_institution_transition(
            recipient,
            area,
            &InstitutionAction::Join { institution },
        )
        .unwrap();
        let concept = game_society::institution(&sim.host.storage, institution)
            .unwrap()
            .concepts[0];
        let application = game_society::deliberation::RuleApplications {
            institution,
            intents: vec![game_society::institutions::RuleIntent::Duty {
                institution,
                concept,
                index: 0,
                actor,
                term: AgreementTerm::Labor {
                    debtor: actor,
                    creditor: recipient,
                    site,
                    millis: 60_000,
                    deadline: SimTime(600_000),
                },
            }],
        };
        let plan = sim
            .host
            .state
            .institutions
            .as_ref()
            .unwrap()
            .prepare_rule_application(
                &sim.host.storage,
                &application,
                sim.host.next_id(),
                sim.time(),
            )
            .unwrap();
        assert!(plan.agreement_transition().is_some());
        reject_new_knowledge(&mut sim);
        let opening = sim.canonical_snapshot().unwrap();
        let positional = sim.realm_positional_membership().unwrap();
        assert_eq!(
            sim.commit_institution_rule_application(&application, &positional),
            Err(Error::InvalidConfiguration(
                "strict institution Agreement receiver".into()
            ))
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), opening);
    }
    #[test]
    fn routed_institution_receiver_failure_rolls_back_foundation_and_pending_receipts() {
        let (mut sim, actor, recipient, area, site, source) = fixture();
        let case = CaseId(sim.host.allocate_id().unwrap().0);
        let now = sim.time();
        game_society::insert_case(
            &mut sim.host.storage,
            case,
            SocialCaseView {
                id: case,
                kind: SocialCaseKind::Exclusion,
                subject: actor,
                counterpart: recipient,
                area,
                inventory: Some(source),
                good: Good::Food,
                need_band: 0,
                reciprocal: false,
                outcome: SocialOutcome::Unresolved,
                observed_at: now,
            },
        )
        .unwrap();
        game_society::insert_exclusions(
            &mut sim.host.storage,
            actor,
            game_society::Exclusions {
                stores: BTreeMap::from([(source, case)]),
            },
        )
        .unwrap();
        sim.host
            .append_many(vec![EventKind::Ate {
                actor,
                source,
                quantity: Quantity(1),
            }])
            .unwrap();
        let action = InstitutionAction::Found {
            charter: charter(actor, recipient, area, site, source, vec![], vec![]),
            origin: None,
        };
        let strict = game_ecs::Store::restore(
            strict_receiver_schema_with_case_rejection(true).unwrap(),
            sim.host.storage.snapshot().unwrap(),
        )
        .unwrap();
        sim.host.storage = strict.into();
        let before = sim.host.snapshot().unwrap();
        let before_clock = sim.host.clock();
        let before_cursors = (
            sim.host.state.institutions.as_ref().unwrap().last_event,
            sim.host
                .state
                .agreements
                .as_ref()
                .unwrap()
                .last_observation_event,
        );
        assert_eq!(
            sim.routed_institution_command(actor, &action),
            Err(Error::InvalidConfiguration(
                "strict pending case receiver".into()
            ))
        );
        assert_eq!(sim.host.snapshot().unwrap(), before);
        assert_eq!(sim.host.clock(), before_clock);
        assert_eq!(
            (
                sim.host.state.institutions.as_ref().unwrap().last_event,
                sim.host
                    .state
                    .agreements
                    .as_ref()
                    .unwrap()
                    .last_observation_event
            ),
            before_cursors
        );
        assert!(
            game_society::institutions(&sim.host.storage)
                .next()
                .is_none()
        );
        assert_eq!(
            game_society::exclusions(&sim.host.storage, actor)
                .unwrap()
                .stores
                .get(&source),
            Some(&case)
        );
    }
    #[test]
    fn routed_institution_preserves_source_then_receiving_event_order_and_single_revision() {
        let (mut sim, actor, recipient, area, site, source) = fixture();
        let case = CaseId(sim.host.allocate_id().unwrap().0);
        let now = sim.time();
        game_society::insert_case(
            &mut sim.host.storage,
            case,
            SocialCaseView {
                id: case,
                kind: SocialCaseKind::Exclusion,
                subject: actor,
                counterpart: recipient,
                area,
                inventory: Some(source),
                good: Good::Food,
                need_band: 0,
                reciprocal: false,
                outcome: SocialOutcome::Unresolved,
                observed_at: now,
            },
        )
        .unwrap();
        game_society::insert_exclusions(
            &mut sim.host.storage,
            actor,
            game_society::Exclusions {
                stores: BTreeMap::from([(source, case)]),
            },
        )
        .unwrap();
        sim.host
            .append_many(vec![EventKind::Ate {
                actor,
                source,
                quantity: Quantity(1),
            }])
            .unwrap();
        let before = sim.host.transaction_opening();
        sim.routed_institution_command(
            actor,
            &InstitutionAction::Found {
                charter: charter(actor, recipient, area, site, source, vec![], vec![]),
                origin: None,
            },
        )
        .unwrap();
        let events = sim.host.events_since(before.next_event - 1);
        assert!(matches!(
            events.first().unwrap().event,
            EventKind::InstitutionProposed { .. }
        ));
        assert!(matches!(
            events.last().unwrap().event,
            EventKind::SocialCaseRecorded { .. }
        ));
        assert_eq!(sim.host.clock().revision, before.clock.revision + 1);
        assert_eq!(
            sim.host.state.institutions.as_ref().unwrap().last_event,
            events.last().unwrap().sequence - 1
        );
        assert!(
            !game_society::exclusions(&sim.host.storage, actor)
                .unwrap()
                .stores
                .contains_key(&source)
        );
    }
}

//! Prepared physical maintenance sequenced with its originating publication.
use crate::*;
use game_ecs::{Batch, StableId};
use game_society::reactions::{
    CaseAsset, CooperationTarget, ReceiptObservation, ReceiptObserverFacts,
    ReceiptReactionsFinalization, SocialReceipt, SubjectFacts,
};

/// Compact source facts remain complete even when the NPC preview copies only a local scene.
#[derive(Clone)]
pub(crate) struct PhysicalMaintenanceContext {
    opening: game_world::TransactionOpening,
    people: Vec<PersonId>,
    facts: InstitutionalReceiptFacts,
    contacts: game_agreements::ContactSet,
    enforcement_people: Vec<game_society::enforcement::PersonFacts>,
    enforcement_assaults: Vec<game_society::enforcement::WitnessedAssault>,
    enforcement_cases: BTreeSet<EnforcementCaseId>,
    enforcement_policies: BTreeSet<InstitutionId>,
    enforcement_memory: BTreeSet<PersonId>,
    enforcement_memory_candidates: BTreeSet<PersonId>,
    agreement_inputs: game_agreements::MaintenanceInputs,
    institutions: BTreeSet<InstitutionId>,
    institution_proposals: BTreeSet<RuleProposalId>,
    realm_commitments: BTreeSet<RealmCommitmentId>,
    realm_knowledge: BTreeSet<PersonId>,
    realm_missions: BTreeSet<MissionId>,
    concepts: BTreeSet<ConceptId>,
    cases: BTreeSet<CaseId>,
    stocks: BTreeSet<InventoryId>,
    sites: BTreeSet<SiteId>,
    carrier_locations: BTreeSet<StableId>,
    missing_knowledge: Vec<PersonId>,
    native_actors: BTreeSet<PersonId>,
    pending_cursor: u64,
}
impl PhysicalMaintenanceContext {
    pub(crate) fn pending_cursor(&self) -> u64 {
        self.pending_cursor
    }
    pub(crate) fn validate_source(&self, source: &Simulation) -> Result<(), Error> {
        let missing = self
            .people
            .iter()
            .copied()
            .filter(|actor| game_society::knowledge(&source.host.storage, *actor).is_none())
            .collect::<Vec<_>>();
        if source.host.transaction_opening() != self.opening
            || source.person_ids != self.people
            || source.institution_receipt_facts(&Batch::new())? != self.facts
            || source.physical_maintenance_contacts(&Batch::new())?.0 != self.contacts.0
            || source.enforcement_observer_people() != self.enforcement_people
            || source.enforcement_assault_facts() != self.enforcement_assaults
            || missing != self.missing_knowledge
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn source_roster_guard(
        &self,
        source: &Simulation,
        batch: &Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.validate_source(source)?;
        let mut reads = game_ecs::ReadSet::new();
        reads.membership::<game_characters::people::Person>(&source.host.storage, |_, _| true);
        reads.bind(&source.host.storage, batch, &game_ecs::ReadFootprint::new())
    }
    fn projected_facts(
        &self,
        scene: &Simulation,
        batch: &Batch,
    ) -> Result<InstitutionalReceiptFacts, Error> {
        let mut facts = self.facts.clone();
        let overrides = scene.institution_receipt_facts(batch)?;
        facts.subjects.extend(overrides.subjects);
        let overrides = overrides
            .observers
            .into_iter()
            .map(|row| (row.actor, row))
            .collect::<BTreeMap<_, _>>();
        for row in &mut facts.observers {
            if let Some(after) = overrides.get(&row.actor) {
                *row = *after;
            }
        }
        Ok(facts)
    }
    pub(crate) fn include_institution_source(
        &mut self,
        source: &Simulation,
        institutions: &BTreeSet<InstitutionId>,
    ) {
        self.institution_proposals = game_society::proposals(&source.host.storage)
            .filter(|(_, row)| institutions.contains(&row.institution))
            .map(|(id, _)| id)
            .collect();
        self.realm_commitments = game_realm::commitments(&source.host.storage)
            .map(|(id, _)| id)
            .collect();
        // Pending Realm work is nested in these commitments, not a separate column.
        self.realm_knowledge = game_realm::knowledges(&source.host.storage)
            .map(|(id, _)| id)
            .collect();
        self.realm_missions = game_missions::missions(&source.host.storage)
            .map(|(id, _)| id)
            .collect();
    }
    pub(crate) fn include_enforcement_observation(&mut self) {
        self.enforcement_memory
            .clone_from(&self.enforcement_memory_candidates);
    }
    pub(crate) fn enforcement_people(
        &self,
        scene: &Simulation,
    ) -> Vec<game_society::enforcement::PersonFacts> {
        let mut people = self
            .enforcement_people
            .iter()
            .cloned()
            .map(|row| (row.id, row))
            .collect::<BTreeMap<_, _>>();
        for row in scene.enforcement_observer_people() {
            people.insert(row.id, row);
        }
        people.into_values().collect()
    }
    pub(crate) fn enforcement_assaults(
        &self,
        scene: &Simulation,
    ) -> Vec<game_society::enforcement::WitnessedAssault> {
        let mut assaults = self.enforcement_assaults.clone();
        assaults.extend(scene.enforcement_assault_facts());
        assaults
    }
    pub(crate) fn observe_enforcement(&self, scene: &mut Simulation) -> Result<(), Error> {
        if scene.host.state.enforcement.is_none() {
            return Ok(());
        }
        if self.enforcement_memory != self.enforcement_memory_candidates {
            return Err(Error::InvalidConfiguration(
                "undeclared enforcement observer scope".into(),
            ));
        }
        let people = self.enforcement_people(scene);
        let assaults = self.enforcement_assaults(scene);
        let mut batch = Batch::new();
        let receipt = scene.prepare_enforcement_observation_with_people(
            &mut batch,
            &people,
            &assaults,
            game_ecs::ReadSet::new(),
            game_ecs::ReadFootprint::new(),
        )?;
        receipt.validate(
            scene.host.state.enforcement.as_ref(),
            &scene.host.storage,
            &batch,
        )?;
        let through = scene.host.next_event() - 1;
        let mut transaction = game_world::Transaction::new(scene.host.transaction_opening(), batch);
        transaction.guards.push(receipt.row_guard());
        scene.host.commit_transaction(transaction)?;
        scene.host.state.enforcement = Some(receipt.finish(through));
        Ok(())
    }
    /// Preview construction imports only native receiving/link columns. Remote
    /// people, bodies, genomes, and activities remain in the source factual roster.
    pub(crate) fn import_native_inputs(
        &self,
        source: &Simulation,
        projection: &mut game_ecs::Store,
        full_scene_ids: &BTreeSet<StableId>,
    ) -> Result<(), Error> {
        self.validate_source(source)?;
        fn copy<C: game_ecs::Component>(
            source: &game_ecs::Store,
            target: &mut game_ecs::Store,
            ids: impl IntoIterator<Item = StableId>,
            full_scene_ids: &BTreeSet<StableId>,
        ) -> Result<(), Error> {
            target.import_rows::<C>(
                source,
                ids.into_iter().filter(|id| !full_scene_ids.contains(id)),
            )
        }
        let store = &source.host.storage;
        copy::<game_society::enforcement::Case>(
            store,
            projection,
            self.enforcement_cases.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::enforcement::Policies>(
            store,
            projection,
            self.enforcement_policies.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::enforcement::Memory>(
            store,
            projection,
            self.enforcement_memory.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::institutions::Institution>(
            store,
            projection,
            self.institutions.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::institutions::RuleProposal>(
            store,
            projection,
            self.institution_proposals.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_realm::Commitment>(
            store,
            projection,
            self.realm_commitments.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_realm::PersonalKnowledge>(
            store,
            projection,
            self.realm_knowledge.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_missions::Mission>(
            store,
            projection,
            self.realm_missions.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::Concept>(
            store,
            projection,
            self.concepts.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_society::institutions::SocialCaseView>(
            store,
            projection,
            self.cases.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_cargo::Inventory>(
            store,
            projection,
            self.stocks.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_property::InventoryCustody>(
            store,
            projection,
            self.stocks.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        let production_sites = self
            .sites
            .iter()
            .copied()
            .chain(
                self.agreement_inputs
                    .output_sources
                    .keys()
                    .map(|(site, _)| *site),
            )
            .collect::<BTreeSet<_>>();
        copy::<game_workshop::Site>(
            store,
            projection,
            production_sites.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agriculture::Field>(
            store,
            projection,
            production_sites.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        let mut output_projects = game_ecs::ReadFootprint::new();
        for (site, good) in self.agreement_inputs.output_sources.keys() {
            output_projects
                .extend(game_projects::prepare_site_output_read(store, *site, *good).reads());
        }
        copy::<game_projects::Project>(
            store,
            projection,
            game_projects::projects(store)
                .map(|(id, _)| StableId(id.0))
                .filter(|id| output_projects.contains::<game_projects::Project>(*id)),
            full_scene_ids,
        )?;
        copy::<Location>(
            store,
            projection,
            self.carrier_locations.iter().copied(),
            full_scene_ids,
        )?;
        let knowledge = self
            .native_actors
            .iter()
            .chain(&self.missing_knowledge)
            .map(|id| StableId(id.0))
            .collect::<BTreeSet<_>>();
        copy::<game_society::Knowledge>(store, projection, knowledge, full_scene_ids)?;
        copy::<game_society::Exclusions>(
            store,
            projection,
            self.native_actors.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agreements::Agreement>(
            store,
            projection,
            self.agreement_inputs
                .agreements
                .iter()
                .map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agreements::Obligation>(
            store,
            projection,
            self.agreement_inputs
                .obligations
                .iter()
                .map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agreements::PendingWork>(
            store,
            projection,
            self.agreement_inputs
                .pending_work
                .iter()
                .map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agreements::Amendment>(
            store,
            projection,
            self.agreement_inputs
                .amendments
                .iter()
                .map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_agreements::Knowledge>(
            store,
            projection,
            self.agreement_inputs
                .knowledge
                .iter()
                .map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        copy::<game_characters::decisions::Mind<Command>>(
            store,
            projection,
            self.agreement_inputs.people.iter().map(|id| StableId(id.0)),
            full_scene_ids,
        )?;
        Ok(())
    }
}

/// All fallible maintenance is prepared against the source's final batch.
/// Source Property effects are folded and staged by their owning source plan.
pub(crate) struct PhysicalMaintenance {
    agreements: crate::agreements::PhysicalAgreementMaintenance,
    institutions: PhysicalInstitutionMaintenance,
    pub(crate) ids: u64,
    pub(crate) events: Vec<EventKind>,
}
impl PhysicalMaintenance {
    pub(crate) fn validate(&self, sim: &Simulation, batch: &Batch) -> Result<(), Error> {
        self.agreements.validate(sim, batch)?;
        self.institutions.validate(sim, batch)
    }
    pub(crate) fn guards(&self) -> Vec<game_ecs::ReadGuard> {
        let mut guards = self.agreements.guards();
        guards.extend(self.institutions.guards());
        guards
    }
    pub(crate) fn finish(self, sim: &mut Simulation) {
        self.agreements.finish(sim);
        self.institutions.finish(sim);
    }
}
impl Simulation {
    pub(crate) fn prepare_physical_maintenance(
        &self,
        batch: &mut Batch,
        prefix: &[EventKind],
        source_ids: u64,
    ) -> Result<PhysicalMaintenance, Error> {
        let agreements = self.prepare_physical_agreement_maintenance(batch, prefix)?;
        let mut received = prefix.to_vec();
        received.extend_from_slice(agreements.events());
        let institutions =
            self.prepare_physical_institution_maintenance(batch, &received, source_ids)?;
        let mut events = agreements.events().to_vec();
        events.extend_from_slice(&institutions.events);
        Ok(PhysicalMaintenance {
            ids: institutions.ids,
            events,
            agreements,
            institutions,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstitutionalReceiptFacts {
    subjects: BTreeMap<PersonId, SubjectFacts>,
    observers: Vec<game_society::perception::ObserverFacts>,
}
impl ReceiptObserverFacts for InstitutionalReceiptFacts {
    fn subject(&self, actor: PersonId) -> Option<SubjectFacts> {
        self.subjects.get(&actor).copied()
    }
    fn subject_reads(&self, _: PersonId) -> game_ecs::ReadFootprint {
        // These inputs are the source owners' guarded final projection. The
        // maintenance adapter rebinds the factual view for final validation.
        game_ecs::ReadFootprint::new()
    }
    fn observe(
        &self,
        case: &game_society::institutions::SocialCaseView,
        public: bool,
    ) -> Result<ReceiptObservation, Error> {
        Ok(ReceiptObservation {
            observers: game_society::perception::ObservationIndex::new(&self.observers)?
                .case_observers(case, public)?,
            reads: game_ecs::ReadFootprint::new(),
        })
    }
    fn observe_batch(
        &self,
        _: &Batch,
        case: &game_society::institutions::SocialCaseView,
        public: bool,
    ) -> Result<ReceiptObservation, Error> {
        self.observe(case, public)
    }
}

pub(crate) struct PhysicalInstitutionMaintenance {
    publication: Option<ReceiptReactionsFinalization>,
    people: Vec<PersonId>,
    now: SimTime,
    prefix: Vec<EventKind>,
    receipts: Vec<(SocialReceipt, SubjectFacts)>,
    context: Option<PhysicalMaintenanceContext>,
    pub(crate) ids: u64,
    pub(crate) events: Vec<EventKind>,
}
impl PhysicalInstitutionMaintenance {
    pub(crate) fn validate(&self, sim: &Simulation, batch: &Batch) -> Result<(), Error> {
        if (self.context.is_none() && self.people != sim.person_ids) || self.now != sim.time() {
            return Err(Error::Unavailable);
        }
        if let Some(publication) = &self.publication {
            let runtime = sim
                .host
                .state
                .institutions
                .as_ref()
                .ok_or(Error::Unavailable)?;
            let facts = self.context.as_ref().map_or_else(
                || sim.institution_receipt_facts(batch),
                |context| context.projected_facts(sim, batch),
            )?;
            if sim.institution_receipts(batch, &self.prefix, runtime.last_event, &facts)?
                != self.receipts
            {
                return Err(Error::Unavailable);
            }
            publication.validate_batch(
                sim.host
                    .state
                    .institutions
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &sim.host.storage,
                self.now,
                &facts,
                batch,
            )?;
        } else if sim.host.state.institutions.is_some() {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub(crate) fn guards(&self) -> Vec<game_ecs::ReadGuard> {
        self.publication
            .as_ref()
            .map(|publication| vec![publication.row_guard()])
            .unwrap_or_default()
    }
    pub(crate) fn finish(self, sim: &mut Simulation) {
        if let Some(publication) = self.publication {
            publication.finish(sim.host.state.institutions.as_mut().unwrap());
        }
    }
}
impl Simulation {
    fn institution_receipt_facts(&self, batch: &Batch) -> Result<InstitutionalReceiptFacts, Error> {
        let mut subjects = BTreeMap::new();
        let mut observers = Vec::with_capacity(self.person_ids.len());
        for actor in &self.person_ids {
            let entity = StableId(actor.0);
            let location = batch.projected::<Location>(&self.host.storage, entity)?;
            let area = match location {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            };
            let body = batch
                .projected::<game_physiology::Body>(&self.host.storage, entity)?
                .ok_or(Error::Unavailable)?;
            let mind = batch
                .projected::<game_characters::decisions::Mind<Command>>(&self.host.storage, entity)?
                .ok_or(Error::Unavailable)?;
            let life = batch.projected::<game_lifecycle::Life>(&self.host.storage, entity)?;
            subjects.insert(
                *actor,
                SubjectFacts {
                    actor: *actor,
                    area,
                    hunger: body.vitals.hunger,
                },
            );
            observers.push(game_society::perception::ObserverFacts {
                actor: *actor,
                area,
                alive: body.vitals.alive,
                independent: game_lifecycle::independent(
                    life.map(|life| life.stage),
                    self.host.state.development.is_some(),
                ),
                // Case observation does not use conversation availability.
                conversation_allowed: true,
                empathy: mind.personality.empathy,
            });
        }
        Ok(InstitutionalReceiptFacts {
            subjects,
            observers,
        })
    }
    pub(crate) fn prepare_physical_maintenance_context(
        &self,
        origin: &Batch,
        prefix: &[EventKind],
    ) -> Result<PhysicalMaintenanceContext, Error> {
        let opening = self.host.transaction_opening();
        let facts = self.institution_receipt_facts(&Batch::new())?;
        let contacts = self.physical_maintenance_contacts(&Batch::new())?;
        let enforcement_people = self.enforcement_observer_people();
        let enforcement_assaults = self.enforcement_assault_facts();
        let enforcement_cases = game_society::enforcement::cases(&self.host.storage)
            .map(|(id, _)| id)
            .collect();
        let enforcement_policies = game_society::enforcement::policies(&self.host.storage)
            .map(|(id, _)| id)
            .chain(game_society::institutions(&self.host.storage).map(|(id, _)| id))
            .collect();
        let enforcement_memory_candidates = enforcement_people
            .iter()
            .map(|row| row.id)
            .chain(game_society::enforcement::memories(&self.host.storage).map(|(id, _)| id))
            .collect();
        let missing_knowledge = self
            .person_ids
            .iter()
            .copied()
            .filter(|actor| game_society::knowledge(&self.host.storage, *actor).is_none())
            .collect::<Vec<_>>();
        let pending_cursor = self
            .host
            .state
            .institutions
            .as_ref()
            .map_or(opening.next_event - 1, |r| r.last_event)
            .min(
                self.host
                    .state
                    .agreements
                    .as_ref()
                    .map_or(opening.next_event - 1, |r| {
                        r.last_event.min(r.last_observation_event)
                    }),
            )
            .min(
                self.host
                    .state
                    .property
                    .as_ref()
                    .map_or(opening.next_event - 1, |r| r.last_event),
            )
            .min(
                self.host
                    .state
                    .enforcement
                    .as_ref()
                    .map_or(opening.next_event - 1, |r| r.last_event),
            );
        let agreement_inputs = self.physical_maintenance_agreement_inputs(origin, prefix)?;
        let mut native_actors = agreement_inputs.people.clone();
        let mut areas = origin
            .insertions::<game_conflict::Encounter>()
            .map(|(_, row)| row.area)
            .collect::<BTreeSet<_>>();
        let institutions = game_society::institutions(&self.host.storage)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        let concepts = game_society::institutions(&self.host.storage)
            .flat_map(|(_, row)| row.concepts.iter().copied())
            .collect::<BTreeSet<_>>();
        let mut stocks = agreement_inputs.stocks.clone();
        let mut sites = BTreeSet::new();
        for (_, row) in game_agreements::obligations(&self.host.storage) {
            match row.term {
                AgreementTerm::Labor { site, .. } => {
                    sites.insert(site);
                    if let Some(site) = game_workshop::site(&self.host.storage, site) {
                        stocks.insert(site.output);
                    }
                }
                AgreementTerm::Delivery { destination, .. } => {
                    stocks.insert(destination);
                }
                _ => {}
            }
        }
        for event in self
            .host
            .events_since(pending_cursor)
            .iter()
            .map(|row| &row.event)
            .chain(prefix)
        {
            if let Some(receipt) = self.social_receipt_projected(origin, event)? {
                native_actors.extend(game_society::reactions::receipt_people(&receipt));
                if let Some(asset) = game_society::reactions::receipt_asset(&receipt) {
                    areas.extend(asset.area);
                    stocks.insert(asset.inventory);
                }
                match receipt {
                    SocialReceipt::SupportReceived { source, .. }
                    | SocialReceipt::Ate { source, .. }
                    | SocialReceipt::Transferred { source, .. } => {
                        stocks.insert(source);
                    }
                    SocialReceipt::Produced { site, .. } => {
                        sites.insert(site);
                    }
                    _ => {}
                }
            }
        }
        for actor in &native_actors {
            areas.extend(facts.subjects.get(actor).and_then(|row| row.area));
        }
        native_actors.extend(
            game_society::perception::ObservationIndex::new(&facts.observers)?
                .case_observer_frontier(&areas),
        );
        let history =
            game_society::reactions::receipt_knowledge_inputs(&self.host.storage, &native_actors);
        let cases = history.cases;
        stocks.extend(history.exclusion_stocks);
        let mut carrier_locations = BTreeSet::new();
        for inventory in &stocks {
            if let Some(row) = game_cargo::inventory(&self.host.storage, *inventory)
                && let InventoryLocation::Carried(actor) = row.location
            {
                game_spatial::resolved_anchor_with(StableId(actor.0), |id| {
                    carrier_locations.insert(id);
                    game_spatial::location(&self.host.storage, id)
                })?;
            }
        }
        Ok(PhysicalMaintenanceContext {
            opening,
            people: self.person_ids.clone(),
            facts,
            contacts,
            enforcement_people,
            enforcement_assaults,
            enforcement_cases,
            enforcement_policies,
            enforcement_memory: BTreeSet::new(),
            enforcement_memory_candidates,
            agreement_inputs,
            institutions,
            institution_proposals: BTreeSet::new(),
            realm_commitments: BTreeSet::new(),
            realm_knowledge: BTreeSet::new(),
            realm_missions: BTreeSet::new(),
            concepts,
            cases,
            stocks,
            sites,
            carrier_locations,
            missing_knowledge,
            native_actors,
            pending_cursor,
        })
    }
    pub(crate) fn prepare_physical_maintenance_with_context(
        &self,
        context: &PhysicalMaintenanceContext,
        batch: &mut Batch,
        prefix: &[EventKind],
        source_ids: u64,
    ) -> Result<PhysicalMaintenance, Error> {
        let agreements = self.prepare_physical_agreement_maintenance_with_context(
            batch,
            prefix,
            &context.contacts,
            &context.agreement_inputs,
        )?;
        let mut received = prefix.to_vec();
        received.extend_from_slice(agreements.events());
        let institutions = self.prepare_physical_institution_maintenance_with_context(
            context, batch, &received, source_ids,
        )?;
        let mut events = agreements.events().to_vec();
        events.extend_from_slice(&institutions.events);
        Ok(PhysicalMaintenance {
            ids: institutions.ids,
            events,
            agreements,
            institutions,
        })
    }
    pub(crate) fn prepare_physical_institution_maintenance_with_context(
        &self,
        context: &PhysicalMaintenanceContext,
        batch: &mut Batch,
        prefix: &[EventKind],
        source_ids: u64,
    ) -> Result<PhysicalInstitutionMaintenance, Error> {
        let now = self.time();
        let Some(runtime) = &self.host.state.institutions else {
            return Ok(PhysicalInstitutionMaintenance {
                publication: None,
                people: context.people.clone(),
                now,
                prefix: prefix.to_vec(),
                receipts: Vec::new(),
                context: Some(context.clone()),
                ids: 0,
                events: Vec::new(),
            });
        };
        let facts = context.projected_facts(self, batch)?;
        let receipts = self.institution_receipts(batch, prefix, runtime.last_event, &facts)?;
        let through = (self.host.next_event() - 1)
            .checked_add(u64::try_from(prefix.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let first = self
            .host
            .next_id()
            .checked_add(source_ids)
            .ok_or(Error::Overflow)?;
        let prepared = runtime.prepare_initialized_receipt_reactions(
            &self.host.storage,
            context.missing_knowledge.iter().copied(),
            &receipts,
            now,
            CaseId(first),
            through,
            &facts,
        )?;
        let ids = prepared.allocations();
        let events = prepared
            .cases()
            .map(|(case, actor)| EventKind::SocialCaseRecorded { case, actor })
            .collect();
        let publication = prepared.stage_into(runtime, &self.host.storage, now, &facts, batch)?;
        Ok(PhysicalInstitutionMaintenance {
            publication: Some(publication),
            people: context.people.clone(),
            now,
            prefix: prefix.to_vec(),
            receipts,
            context: Some(context.clone()),
            ids,
            events,
        })
    }
    pub(crate) fn prepare_physical_institution_maintenance(
        &self,
        batch: &mut Batch,
        prefix: &[EventKind],
        source_ids: u64,
    ) -> Result<PhysicalInstitutionMaintenance, Error> {
        let now = self.time();
        let people = self.person_ids.clone();
        let Some(runtime) = &self.host.state.institutions else {
            return Ok(PhysicalInstitutionMaintenance {
                publication: None,
                people,
                now,
                prefix: prefix.to_vec(),
                receipts: Vec::new(),
                context: None,
                ids: 0,
                events: Vec::new(),
            });
        };
        let facts = self.institution_receipt_facts(batch)?;
        let receipts = self.institution_receipts(batch, prefix, runtime.last_event, &facts)?;
        let through = (self.host.next_event() - 1)
            .checked_add(u64::try_from(prefix.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let first = self
            .host
            .next_id()
            .checked_add(source_ids)
            .ok_or(Error::Overflow)?;
        let prepared = runtime.prepare_initialized_receipt_reactions(
            &self.host.storage,
            people.iter().copied(),
            &receipts,
            now,
            CaseId(first),
            through,
            &facts,
        )?;
        let ids = prepared.allocations();
        let events = prepared
            .cases()
            .map(|(case, actor)| EventKind::SocialCaseRecorded { case, actor })
            .collect();
        let publication = prepared.stage_into(runtime, &self.host.storage, now, &facts, batch)?;
        Ok(PhysicalInstitutionMaintenance {
            publication: Some(publication),
            people,
            now,
            prefix: prefix.to_vec(),
            receipts,
            context: None,
            ids,
            events,
        })
    }
    fn institution_receipts(
        &self,
        batch: &Batch,
        prefix: &[EventKind],
        cursor: u64,
        facts: &InstitutionalReceiptFacts,
    ) -> Result<Vec<(SocialReceipt, SubjectFacts)>, Error> {
        self.host
            .events_since(cursor)
            .iter()
            .map(|event| &event.event)
            .chain(prefix)
            .filter_map(|event| self.social_receipt_projected(batch, event).transpose())
            .map(|receipt| {
                let receipt = receipt?;
                let actor = game_society::review::receipt_actor(&receipt);
                Ok((receipt, facts.subject(actor).ok_or(Error::Unavailable)?))
            })
            .collect()
    }
    pub(crate) fn social_receipt_projected(
        &self,
        batch: &Batch,
        event: &EventKind,
    ) -> Result<Option<SocialReceipt>, Error> {
        let stock = |id: InventoryId| {
            batch
                .projected::<game_cargo::Inventory>(&self.host.storage, StableId(id.0))?
                .ok_or(Error::Unavailable)
        };
        let stock_area =
            |inventory: &game_cargo::Inventory| -> Result<Option<AreaId>, Error> {
                match inventory.location {
                    InventoryLocation::Area(area) => Ok(Some(area)),
                    InventoryLocation::Carried(actor) => {
                        let mut failed = None;
                        let anchor = game_spatial::resolved_anchor_with(StableId(actor.0), |id| {
                            match batch.projected::<Location>(&self.host.storage, id) {
                                Ok(row) => row,
                                Err(error) => {
                                    failed = Some(error);
                                    None
                                }
                            }
                        });
                        if let Some(error) = failed {
                            return Err(error);
                        }
                        Ok(match anchor?.1 {
                            Location::AtArea(area) => Some(*area),
                            _ => None,
                        })
                    }
                }
            };
        let asset = |inventory: InventoryId, area: Option<AreaId>| -> Result<CaseAsset, Error> {
            Ok(CaseAsset {
                area,
                inventory,
                good: stock(inventory)?.good,
            })
        };
        let target = |term: &AgreementTerm| -> Result<Option<CooperationTarget>, Error> {
            Ok(match *term {
                AgreementTerm::Labor { site, .. } => {
                    let row = batch
                        .projected::<game_workshop::Site>(&self.host.storage, StableId(site.0))?
                        .ok_or(Error::Unavailable)?;
                    Some(CooperationTarget::Labor {
                        site,
                        asset: asset(row.output, Some(row.area))?,
                    })
                }
                AgreementTerm::Delivery { destination, .. } => Some(CooperationTarget::Delivery {
                    asset: asset(destination, stock_area(stock(destination)?)?)?,
                }),
                _ => None,
            })
        };
        Ok(match *event {
            EventKind::ObligationPerformed {
                obligation, actor, ..
            } => {
                let obligation = batch
                    .projected::<game_agreements::Obligation>(
                        &self.host.storage,
                        StableId(obligation.0),
                    )?
                    .ok_or(Error::Unavailable)?;
                target(&obligation.term)?.map(|target| SocialReceipt::Performed {
                    actor,
                    counterpart: obligation.term.parties()[1],
                    agreement: obligation.agreement,
                    target,
                })
            }
            EventKind::ObligationDefaulted { obligation } => {
                let obligation = batch
                    .projected::<game_agreements::Obligation>(
                        &self.host.storage,
                        StableId(obligation.0),
                    )?
                    .ok_or(Error::Unavailable)?;
                target(&obligation.term)?.map(|target| SocialReceipt::Defaulted {
                    actor: obligation.term.parties()[0],
                    counterpart: obligation.term.parties()[1],
                    agreement: obligation.agreement,
                    asset: match target {
                        CooperationTarget::Labor { asset, .. }
                        | CooperationTarget::Delivery { asset } => asset,
                    },
                })
            }
            EventKind::Produced {
                actor,
                site,
                output,
                ..
            } => {
                let row = batch
                    .projected::<game_workshop::Site>(&self.host.storage, StableId(site.0))?
                    .ok_or(Error::Unavailable)?;
                let custodian = batch
                    .projected::<game_property::InventoryCustody>(
                        &self.host.storage,
                        StableId(output.0),
                    )?
                    .map(|row| row.custodian);
                Some(SocialReceipt::Produced {
                    actor,
                    site,
                    asset: asset(output, Some(row.area))?,
                    custodian,
                })
            }
            EventKind::Ate { actor, source, .. } => Some(SocialReceipt::Ate { actor, source }),
            EventKind::Transferred {
                actor,
                source,
                destination,
                ..
            } => Some(SocialReceipt::Transferred {
                actor,
                source,
                good: stock(source)?.good,
                destination_holder: match stock(destination)?.location {
                    InventoryLocation::Carried(actor) => Some(actor),
                    _ => None,
                },
            }),
            _ => None,
        })
    }
}

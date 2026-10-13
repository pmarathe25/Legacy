//! Bounded factual scene projection; all NPC decisions use the existing owners.
use super::*;
use game_ecs::{Batch, ReadGuard, StableId};

pub(crate) struct PreparedCombatOpeningFollowups {
    encounter: Option<EncounterId>,
    transaction: game_world::Transaction<EventKind>,
    finish: CombatOpeningFinalization,
    inputs: AssemblyState,
    selected: BTreeSet<StableId>,
    context: Option<crate::maintenance::PhysicalMaintenanceContext>,
    agreement_context: Option<AgreementPreviewContext>,
}
#[derive(Clone)]
struct AgreementPreviewContext {
    contacts: game_agreements::ContactSet,
    inputs: game_agreements::MaintenanceInputs,
    pending_cursor: u64,
}
impl AgreementPreviewContext {
    fn validate(&self, source: &Simulation) -> Result<(), Error> {
        let inputs = source.physical_maintenance_agreement_inputs(&Batch::new(), &[])?;
        if source.physical_maintenance_contacts(&Batch::new())?.0 != self.contacts.0
            || (
                inputs.agreements,
                inputs.obligations,
                inputs.amendments,
                inputs.pending_work,
                inputs.knowledge,
                inputs.people,
                inputs.stocks,
                inputs.output_sources,
            ) != (
                self.inputs.agreements.clone(),
                self.inputs.obligations.clone(),
                self.inputs.amendments.clone(),
                self.inputs.pending_work.clone(),
                self.inputs.knowledge.clone(),
                self.inputs.people.clone(),
                self.inputs.stocks.clone(),
                self.inputs.output_sources.clone(),
            )
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    fn import(
        &self,
        source: &Simulation,
        projection: &mut game_ecs::Store,
        full: &BTreeSet<StableId>,
    ) -> Result<(), Error> {
        fn rows<C: game_ecs::Component>(
            source: &Simulation,
            target: &mut game_ecs::Store,
            ids: impl IntoIterator<Item = StableId>,
            full: &BTreeSet<StableId>,
        ) -> Result<(), Error> {
            target.import_rows::<C>(
                &source.host.storage,
                ids.into_iter().filter(|id| !full.contains(id)),
            )
        }
        rows::<game_agreements::Agreement>(
            source,
            projection,
            self.inputs.agreements.iter().map(|id| StableId(id.0)),
            full,
        )?;
        rows::<game_agreements::Obligation>(
            source,
            projection,
            self.inputs.obligations.iter().map(|id| StableId(id.0)),
            full,
        )?;
        rows::<game_agreements::Amendment>(
            source,
            projection,
            self.inputs.amendments.iter().map(|id| StableId(id.0)),
            full,
        )?;
        rows::<game_agreements::PendingWork>(
            source,
            projection,
            self.inputs.pending_work.iter().map(|id| StableId(id.0)),
            full,
        )?;
        let people: BTreeSet<_> = self
            .inputs
            .knowledge
            .iter()
            .chain(&self.inputs.people)
            .map(|id| StableId(id.0))
            .collect();
        rows::<game_agreements::Knowledge>(source, projection, people, full)?;
        rows::<game_characters::decisions::Mind<Command>>(
            source,
            projection,
            self.inputs.people.iter().map(|id| StableId(id.0)),
            full,
        )?;
        rows::<game_cargo::Inventory>(
            source,
            projection,
            self.inputs.stocks.iter().map(|id| StableId(id.0)),
            full,
        )?;
        rows::<game_property::InventoryCustody>(
            source,
            projection,
            self.inputs.stocks.iter().map(|id| StableId(id.0)),
            full,
        )?;
        // Reconciliation resolves the authored output binding through native
        // owner rows, including sites outside the active combat scene.
        let output_sites = self
            .inputs
            .output_sources
            .keys()
            .map(|(site, _)| StableId(site.0));
        rows::<game_workshop::Site>(source, projection, output_sites.clone(), full)?;
        rows::<game_agriculture::Field>(source, projection, output_sites, full)?;
        let mut project_outputs = game_ecs::ReadFootprint::new();
        for (site, good) in self.inputs.output_sources.keys() {
            project_outputs.extend(
                game_projects::prepare_site_output_read(&source.host.storage, *site, *good).reads(),
            );
        }
        rows::<game_projects::Project>(
            source,
            projection,
            game_projects::projects(&source.host.storage)
                .map(|(id, _)| StableId(id.0))
                .filter(|id| project_outputs.contains::<game_projects::Project>(*id)),
            full,
        )?;
        Ok(())
    }
    fn guard(&self, source: &Simulation, batch: &Batch) -> Result<ReadGuard, Error> {
        self.validate(source)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        macro_rules! column {
            ($ty:ty) => {{
                reads.membership::<$ty>(&source.host.storage, |_, _| true);
                for id in batch.edited_ids::<$ty>() {
                    admitted.protect::<$ty>(id);
                }
            }};
        }
        column!(game_agreements::Agreement);
        column!(game_agreements::Obligation);
        column!(game_agreements::Amendment);
        column!(game_agreements::PendingWork);
        column!(game_agreements::Knowledge);
        reads.bind(&source.host.storage, batch, &admitted)
    }
}
pub(crate) struct CombatOpeningFinalization {
    before: AssemblyState,
    after: AssemblyState,
    inventory_before: BTreeMap<InventoryLocation, Vec<InventoryId>>,
    inventory_after: BTreeMap<InventoryLocation, Vec<InventoryId>>,
    pregnancies_before: BTreeMap<PersonId, PregnancyId>,
    pregnancies_after: BTreeMap<PersonId, PregnancyId>,
    care_before: BTreeMap<PersonId, PersonId>,
    care_after: BTreeMap<PersonId, PersonId>,
    dependents_before: BTreeMap<AreaId, BTreeSet<PersonId>>,
    dependents_after: BTreeMap<AreaId, BTreeSet<PersonId>>,
}

fn due_selected(due: &Due, selected: &BTreeSet<StableId>, encounter: Option<EncounterId>) -> bool {
    match due {
        Due::Arrive(actor) | Due::Physical(actor) | Due::Age(actor) => {
            selected.contains(&StableId(actor.0))
        }
        Due::Birth(id) => selected.contains(&StableId(id.0)),
        Due::Spoil(id) => selected.contains(&StableId(id.0)),
        Due::CombatRound(id) => Some(*id) == encounter,
        Due::Transfers(_) | Due::Review => false,
    }
}
impl Simulation {
    fn combat_preview_inputs(
        &self,
        selected: &BTreeSet<StableId>,
        encounter: Option<EncounterId>,
    ) -> AssemblyState {
        let state = &self.host.state;
        AssemblyState {
            openings: None,
            workshops: state.workshops.clone(),
            armies: state.armies.clone(),
            realm: state.realm.clone(),
            transport: state.transport.clone(),
            genetics: state.genetics.clone(),
            physical_traits: state.physical_traits.clone(),
            temperament: state.temperament.clone(),
            heredity: state.heredity.clone(),
            content: BTreeMap::new(),
            scenario: state.scenario.clone(),
            ruler: state.ruler,
            areas: state.areas.clone(),
            physical: state.physical.clone(),
            decisions: state.decisions.clone(),
            development: state.development.clone(),
            combat: state.combat.clone(),
            environment: state.environment.clone(),
            agriculture: state.agriculture.clone(),
            agreements: state.agreements.clone(),
            lineage: state.lineage.clone(),
            property: state.property.clone(),
            institutions: state.institutions.clone(),
            skills: state.skills.clone(),
            projects: state.projects.clone(),
            invocation: state.invocation.clone(),
            force: state.force.clone(),
            missions: state.missions.clone(),
            ecology: state.ecology.clone(),
            health: state.health.clone(),
            social: state.social.clone(),
            enforcement: state.enforcement.clone(),
            due: state
                .due
                .iter()
                .filter_map(|(at, deadlines)| {
                    let deadlines: Vec<_> = deadlines
                        .iter()
                        .filter(|due| due_selected(due, selected, encounter))
                        .cloned()
                        .collect();
                    (!deadlines.is_empty()).then_some((*at, deadlines))
                })
                .collect(),
            accepted: BTreeMap::new(),
            next_command: state.next_command,
        }
    }

    /// Select actual referenced identities, never a second population snapshot.
    fn combat_preview_entities(
        &self,
        id: Option<EncounterId>,
        origin: &Batch,
        extra: BTreeSet<StableId>,
    ) -> Result<BTreeSet<StableId>, Error> {
        let encounter = id
            .map(|id| {
                origin
                    .projected::<Encounter>(&self.host.storage, StableId(id.0))?
                    .ok_or(Error::Unavailable)
            })
            .transpose()?;
        let mut potential_combatants: BTreeSet<_> = encounter
            .into_iter()
            .flat_map(|encounter| encounter.fighters.keys().copied())
            .collect();
        potential_combatants.extend(extra.iter().filter_map(|id| {
            game_characters::person(&self.host.storage, PersonId(id.0)).map(|person| person.id)
        }));
        let mut selected: BTreeSet<_> = origin.edited_entity_ids().collect();
        selected.extend(extra);
        selected.insert(StableId(self.host.state.ruler.0));
        if let Some(encounter) = encounter {
            selected.extend(encounter.fighters.keys().map(|actor| StableId(actor.0)));
            selected.extend([
                StableId(encounter.id.0),
                StableId(encounter.map.0),
                StableId(encounter.area.0),
            ]);
        }
        if let Some(lineage) = &self.host.state.lineage {
            selected.insert(lineage.history);
            if let Some(current) = lineage.current(&self.host.storage) {
                selected.insert(StableId(current.0));
            }
            selected
                .extend(game_lineage::guardians(&self.host.storage).map(|(id, _)| StableId(id.0)));
        }
        loop {
            let before = selected.len();
            for (id, group) in game_transport::groups(&self.host.storage) {
                if selected.contains(&StableId(group.anchor.0))
                    || selected.contains(&StableId(group.passenger.0))
                    || group
                        .bearers
                        .iter()
                        .any(|actor| selected.contains(&StableId(actor.0)))
                {
                    selected.insert(StableId(id.0));
                    selected.extend(group.bearers.iter().map(|actor| StableId(actor.0)));
                    selected.extend([StableId(group.passenger.0), StableId(group.anchor.0)]);
                }
            }
            for (id, lift) in game_transport::lifts(&self.host.storage) {
                if selected.contains(&StableId(lift.passenger.0))
                    || lift
                        .bearers
                        .iter()
                        .any(|actor| selected.contains(&StableId(actor.0)))
                {
                    selected.insert(StableId(id.0));
                    selected.insert(StableId(lift.passenger.0));
                    selected.extend(lift.bearers.iter().map(|actor| StableId(actor.0)));
                }
            }
            for (id, work) in game_conflict::force::escapes(&self.host.storage) {
                if selected.contains(&StableId(work.actor.0))
                    || selected.contains(&StableId(work.target.0))
                {
                    selected.extend([
                        StableId(id.0),
                        StableId(work.actor.0),
                        StableId(work.target.0),
                    ]);
                    selected.extend(work.tool.map(|id| StableId(id.0)));
                }
            }
            for (id, work) in game_conflict::force::restraints(&self.host.storage) {
                if selected.contains(&StableId(id.0)) || selected.contains(&StableId(work.target.0))
                {
                    selected.extend([
                        StableId(id.0),
                        StableId(work.target.0),
                        StableId(work.binding.0),
                    ]);
                }
            }
            for (id, work) in game_health::jobs(&self.host.storage) {
                if let game_health::Work::Treat { patient, .. } = work.work
                    && (selected.contains(&StableId(id.0))
                        || selected.contains(&StableId(patient.0)))
                {
                    selected.extend([StableId(id.0), StableId(patient.0)]);
                }
            }
            for entity in selected.clone() {
                let actor = PersonId(entity.0);
                if let Some(person) = game_characters::person(&self.host.storage, actor) {
                    selected.insert(StableId(person.inventory.0));
                    selected.extend(
                        self.inventory_index
                            .get(&InventoryLocation::Carried(actor))
                            .into_iter()
                            .flatten()
                            .map(|id| StableId(id.0)),
                    );
                    if let Some(id) = self.pregnancy_index.get(&actor) {
                        selected.insert(StableId(id.0));
                    }
                    if let Some(caregiver) = self.care_index.get(&actor) {
                        selected.insert(StableId(caregiver.0));
                    }
                    if let Some(mission) = potential_combatants
                        .contains(&actor)
                        .then(|| {
                            self.host.state.missions.as_ref().and_then(|_| {
                                self.mission_membership
                                    .membership(&self.host.storage, actor)
                            })
                        })
                        .flatten()
                    {
                        selected.insert(StableId(mission.0));
                    }
                }
                for pregnancy in game_lifecycle::active_pregnancies(&self.host.storage, actor) {
                    selected.extend([
                        StableId(pregnancy.id.0),
                        StableId(pregnancy.gestator.0),
                        StableId(pregnancy.partner.0),
                    ]);
                }
                if let Some(guardian) = game_lineage::guardian(&self.host.storage, actor) {
                    selected.extend(guardian.target.map(|target| StableId(target.0)));
                }
                if let Some(location) = game_spatial::location(&self.host.storage, entity) {
                    match *location {
                        Location::Carried { carrier } => {
                            selected.insert(StableId(carrier.0));
                        }
                        Location::AtArea(area) => {
                            selected.insert(StableId(area.0));
                        }
                        Location::InTransit { from, to, .. }
                        | Location::StoppedOnRoad { from, to, .. } => {
                            selected.extend([StableId(from.0), StableId(to.0)]);
                        }
                    }
                }
                if let Some(group) = game_transport::group(&self.host.storage, actor) {
                    selected.extend(group.bearers.iter().map(|actor| StableId(actor.0)));
                    selected.extend([StableId(group.passenger.0), StableId(group.anchor.0)]);
                }
                if let Some(session) = game_invocation::session(&self.host.storage, actor) {
                    selected.extend(
                        session
                            .active
                            .iter()
                            .flat_map(|active| &active.lives)
                            .map(|id| StableId(id.0)),
                    );
                    selected.extend(
                        session
                            .activation
                            .iter()
                            .flat_map(|activation| &activation.lives)
                            .map(|id| StableId(id.0)),
                    );
                }
                if let Some(owner) = &self.host.state.institutions {
                    selected.extend(
                        owner
                            .social_case_views(&self.host.storage, actor)
                            .into_iter()
                            .map(|case| StableId(case.id.0)),
                    );
                }
                if let Some(captive) = game_conflict::captivity(&self.host.storage, actor) {
                    selected.extend([
                        StableId(captive.captor.0),
                        StableId(captive.binding.0),
                        StableId(captive.map.0),
                    ]);
                }
                if let Some(interaction) =
                    game_characters::social::interaction(&self.host.storage, actor)
                {
                    selected.extend(
                        interaction
                            .actors()
                            .into_iter()
                            .map(|actor| StableId(actor.0)),
                    );
                }
                if let Some(pregnancy) =
                    game_lifecycle::pregnancy(&self.host.storage, PregnancyId(entity.0))
                {
                    selected.extend([
                        StableId(pregnancy.gestator.0),
                        StableId(pregnancy.partner.0),
                    ]);
                }
                if let Some(mission) =
                    game_missions::mission(&self.host.storage, MissionId(entity.0))
                {
                    selected.extend([
                        StableId(mission.sponsor.0),
                        StableId(mission.proposal.leader.0),
                        StableId(mission.proposal.home.0),
                    ]);
                    selected.extend(mission.members.keys().map(|actor| StableId(actor.0)));
                    selected.extend(
                        mission
                            .members
                            .values()
                            .map(|member| StableId(member.recruitment.0)),
                    );
                    for member in mission.members.values() {
                        selected.extend(member.cargo.keys().map(|id| StableId(id.0)));
                        selected.extend(member.supply_cargo.keys().map(|id| StableId(id.0)));
                        objective_ids(&member.directive, &mut selected);
                    }
                    objective_ids(&mission.proposal.objective, &mut selected);
                    for supply in &mission.proposal.supplies {
                        selected.extend([StableId(supply.carrier.0), StableId(supply.source.0)]);
                    }
                }
                if let Some(recruitment) =
                    game_missions::recruitment(&self.host.storage, RecruitmentId(entity.0))
                {
                    selected.extend([
                        StableId(recruitment.mission.0),
                        StableId(recruitment.person.0),
                    ]);
                    selected.extend(recruitment.agreement.map(|id| StableId(id.0)));
                }
                if let Some(agreement) =
                    game_agreements::agreement(&self.host.storage, AgreementId(entity.0))
                {
                    selected.extend(agreement.obligations.iter().map(|id| StableId(id.0)));
                    selected.extend(agreement.parties.iter().map(|id| StableId(id.0)));
                    for term in &agreement.terms {
                        agreement_term_ids(term, &mut selected);
                    }
                }
                if let Some(obligation) =
                    game_agreements::obligation(&self.host.storage, ObligationId(entity.0))
                {
                    agreement_term_ids(&obligation.term, &mut selected);
                    selected.extend(obligation.cargo.keys().map(|id| StableId(id.0)));
                }
                if let Some(battle) = game_conflict::actor_encounter(&self.host.storage, actor) {
                    selected.extend([StableId(battle.id.0), StableId(battle.map.0)]);
                }
            }
            if selected.len() == before {
                return Ok(selected);
            }
        }
    }

    pub(crate) fn prepare_combat_opening_followups(
        &self,
        id: EncounterId,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_projection(
            id,
            origin,
            origin_ids,
            origin_events,
            origin_revisions,
            None,
        )
    }
    pub(crate) fn prepare_combat_opening_followups_with_force(
        &self,
        id: EncounterId,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
        force: &game_conflict::force::Runtime,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_projection(
            id,
            origin,
            origin_ids,
            origin_events,
            origin_revisions,
            Some(force),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_combat_opening_projection(
        &self,
        id: EncounterId,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
        force: Option<&game_conflict::force::Runtime>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_callbacks(
            Some(id),
            origin,
            origin_ids,
            origin_events,
            origin_revisions,
            force,
            BTreeSet::new(),
            false,
            None,
            None,
            |_| Ok(()),
            |_| Ok(()),
        )
    }
    pub(crate) fn prepare_combat_opening_followups_with_property(
        &self,
        id: EncounterId,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_callbacks(
            Some(id),
            origin,
            origin_ids,
            origin_events,
            origin_revisions,
            None,
            BTreeSet::new(),
            true,
            None,
            None,
            |_| Ok(()),
            |preview| preview.update_property_events(),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_combat_opening_followups_with_force_context(
        &self,
        id: EncounterId,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
        force: &game_conflict::force::Runtime,
        context: &crate::maintenance::PhysicalMaintenanceContext,
        prefix: impl FnOnce(&mut Simulation),
        suffix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_callbacks(
            Some(id),
            origin,
            origin_ids,
            origin_events,
            origin_revisions,
            Some(force),
            BTreeSet::new(),
            true,
            Some(context),
            None,
            |preview| {
                prefix(preview);
                Ok(())
            },
            suffix,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_property_source_followups(
        &self,
        encounter: Option<EncounterId>,
        origin: &Batch,
        origin_ids: u64,
        origin_revisions: u64,
        extra_selected: BTreeSet<StableId>,
        prefix: impl FnOnce(&mut Simulation),
        suffix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_property_source_followups_fallible(
            encounter,
            origin,
            origin_ids,
            origin_revisions,
            extra_selected,
            |preview| {
                prefix(preview);
                Ok(())
            },
            suffix,
        )
    }
    /// Run a source transition and its receivers in the same bounded scene.
    /// Both callbacks finish all fallible native owner work before admission.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_property_source_followups_fallible(
        &self,
        encounter: Option<EncounterId>,
        origin: &Batch,
        origin_ids: u64,
        origin_revisions: u64,
        extra_selected: BTreeSet<StableId>,
        prefix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
        suffix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        let through = self.host.next_event() - 1;
        let context = AgreementPreviewContext {
            contacts: self.physical_maintenance_contacts(&Batch::new())?,
            inputs: self.physical_maintenance_agreement_inputs(&Batch::new(), &[])?,
            pending_cursor: self
                .host
                .state
                .property
                .as_ref()
                .map_or(through, |owner| owner.last_event)
                .min(
                    self.host
                        .state
                        .agreements
                        .as_ref()
                        .map_or(through, |owner| {
                            owner.last_event.min(owner.last_observation_event)
                        }),
                ),
        };
        self.prepare_combat_opening_callbacks(
            encounter,
            origin,
            origin_ids,
            &[],
            origin_revisions,
            None,
            extra_selected,
            true,
            None,
            Some(&context),
            prefix,
            suffix,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_source_followups_with_context(
        &self,
        encounter: Option<EncounterId>,
        origin: &Batch,
        origin_ids: u64,
        origin_revisions: u64,
        extra_selected: BTreeSet<StableId>,
        context: &crate::maintenance::PhysicalMaintenanceContext,
        prefix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
        suffix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.prepare_combat_opening_callbacks(
            encounter,
            origin,
            origin_ids,
            &[],
            origin_revisions,
            None,
            extra_selected,
            true,
            Some(context),
            None,
            prefix,
            suffix,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_combat_opening_callbacks(
        &self,
        id: Option<EncounterId>,
        origin: &Batch,
        origin_ids: u64,
        origin_events: &[EventKind],
        origin_revisions: u64,
        force: Option<&game_conflict::force::Runtime>,
        mut extra_selected: BTreeSet<StableId>,
        property_tail: bool,
        context: Option<&crate::maintenance::PhysicalMaintenanceContext>,
        agreement_context: Option<&AgreementPreviewContext>,
        prefix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
        suffix: impl FnOnce(&mut Simulation) -> Result<(), Error>,
    ) -> Result<PreparedCombatOpeningFollowups, Error> {
        self.host.storage.validate_batch(origin)?;
        let tail = if let Some(context) = agreement_context {
            self.host.events_since(context.pending_cursor)
        } else if let Some(context) = context {
            self.host.events_since(context.pending_cursor())
        } else if property_tail {
            self.host
                .state
                .property
                .as_ref()
                .map_or(&[][..], |owner| self.host.events_since(owner.last_event))
        } else {
            &[][..]
        };
        for event in tail {
            self.combat_pending_receipt_entities(&event.event, &mut extra_selected)?;
        }
        if property_tail && self.host.state.property.is_some() {
            extra_selected.extend(game_cargo::inventories(&self.host.storage).filter_map(
                |(id, _)| {
                    game_property::portions(&self.host.storage, id)
                        .is_none()
                        .then_some(StableId(id.0))
                },
            ));
        }
        let selected = self.combat_preview_entities(id, origin, extra_selected)?;
        let mut opening_projection = self
            .host
            .storage
            .project_entities(selected.iter().copied())?;
        if let Some(context) = context {
            context.import_native_inputs(self, &mut opening_projection, &selected)?;
        }
        if let Some(context) = agreement_context {
            context.import(self, &mut opening_projection, &selected)?;
        }
        let mut projected = opening_projection.clone();
        origin.apply_to_projection(&mut projected)?;
        let inputs = self.combat_preview_inputs(&selected, id);
        let mut preview_inputs = inputs.clone();
        if let Some(force) = force {
            preview_inputs.force = Some(force.clone());
        }
        let host = game_world::GenericHost::preview_with_tail(
            Components::from(projected),
            preview_inputs,
            &self.host.transaction_opening(),
            self.host.seed(),
            tail,
        )?;
        let mut preview = self.combat_preview_facade(host, &selected);
        preview.host.allocate_ids(origin_ids)?;
        preview.host.bump_revision(origin_revisions)?;
        let before = preview.host.state.clone();
        let inventory_before = preview.inventory_index.clone();
        let pregnancies_before = preview.pregnancy_index.clone();
        let care_before = preview.care_index.clone();
        let dependents_before = preview.dependent_index.clone();
        preview.host.append_many(origin_events.to_vec())?;
        prefix(&mut preview)?;
        if let Some(id) = id {
            preview.drive_combat_npcs(id)?;
        }
        suffix(&mut preview)?;
        let mut batch = Batch::new();
        let (reads, footprint) = preview
            .host
            .storage
            .diff_into(&opening_projection, &mut batch)?;
        let guard: ReadGuard = reads.bind(&self.host.storage, &batch, &footprint)?;
        guard.validate(&self.host.storage, &batch)?;
        self.host.storage.validate_batch(&batch)?;
        let mut transaction = game_world::Transaction::new(self.host.transaction_opening(), batch);
        transaction.ids = preview
            .host
            .next_id()
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        transaction.revisions = preview
            .host
            .clock()
            .revision
            .checked_sub(self.host.clock().revision)
            .ok_or(Error::Overflow)?;
        transaction.events = preview
            .host
            .events_since(self.host.next_event() - 1)
            .iter()
            .map(|event| event.event.clone())
            .collect();
        transaction.guards.push(guard);
        if let Some(context) = context {
            transaction
                .guards
                .push(context.source_roster_guard(self, &transaction.batch)?);
        }
        if let Some(context) = agreement_context {
            transaction
                .guards
                .push(context.guard(self, &transaction.batch)?);
        }
        self.host.preflight_records(
            transaction.ids,
            u64::try_from(transaction.events.len()).map_err(|_| Error::Overflow)?,
            transaction.revisions,
        )?;
        let finish = CombatOpeningFinalization {
            before,
            after: preview.host.state,
            inventory_before,
            inventory_after: preview.inventory_index,
            pregnancies_before,
            pregnancies_after: preview.pregnancy_index,
            care_before,
            care_after: preview.care_index,
            dependents_before,
            dependents_after: preview.dependent_index,
        };
        Ok(PreparedCombatOpeningFollowups {
            encounter: id,
            transaction,
            finish,
            inputs,
            selected,
            context: context.cloned(),
            agreement_context: agreement_context.cloned(),
        })
    }
    fn combat_pending_receipt_entities(
        &self,
        event: &EventKind,
        selected: &mut BTreeSet<StableId>,
    ) -> Result<(), Error> {
        match *event {
            EventKind::Transferred {
                source,
                destination,
                ..
            } => {
                selected.extend([StableId(source.0), StableId(destination.0)]);
            }
            EventKind::Ate { source, .. }
            | EventKind::Consumed { source, .. }
            | EventKind::Cared { source, .. } => {
                selected.insert(StableId(source.0));
            }
            EventKind::Spoiled { inventory, .. } => {
                selected.insert(StableId(inventory.0));
            }
            EventKind::Produced { output, .. }
            | EventKind::ResourceCollected {
                destination: output,
                ..
            } => {
                selected.insert(StableId(output.0));
            }
            EventKind::CropHarvested { field, .. } => {
                selected.insert(StableId(field.0));
                let field = self
                    .host
                    .state
                    .agriculture
                    .as_ref()
                    .and_then(|owner| owner.field(&self.host.storage, field))
                    .ok_or(Error::Unavailable)?;
                selected.extend([StableId(field.food.0), StableId(field.seed.0)]);
            }
            EventKind::Born { child, .. } => {
                selected.insert(StableId(child.0));
            }
            _ => {}
        }
        Ok(())
    }

    fn combat_preview_facade(
        &self,
        mut host: GameHost,
        selected: &BTreeSet<StableId>,
    ) -> Simulation {
        let person_ids = game_characters::person_ids(&host.storage);
        let person_index = person_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();
        let inventory_index = self
            .inventory_index
            .iter()
            .filter_map(|(location, ids)| {
                let ids: Vec<_> = ids
                    .iter()
                    .filter(|id| selected.contains(&StableId(id.0)))
                    .copied()
                    .collect();
                (!ids.is_empty()).then_some((location.clone(), ids))
            })
            .collect();
        let pregnancy_index = self
            .pregnancy_index
            .iter()
            .filter(|(actor, id)| {
                selected.contains(&StableId(actor.0)) || selected.contains(&StableId(id.0))
            })
            .map(|(actor, id)| (*actor, *id))
            .collect();
        let care_index = self
            .care_index
            .iter()
            .filter(|(child, caregiver)| {
                selected.contains(&StableId(child.0)) || selected.contains(&StableId(caregiver.0))
            })
            .map(|(child, caregiver)| (*child, *caregiver))
            .collect();
        let dependent_index = self
            .dependent_index
            .iter()
            .filter_map(|(area, actors)| {
                let actors: BTreeSet<_> = actors
                    .iter()
                    .filter(|actor| person_ids.contains(actor))
                    .copied()
                    .collect();
                (!actors.is_empty()).then_some((*area, actors))
            })
            .collect();
        let mission_membership = game_missions::MembershipIndex::build(&host.storage);
        let agriculture_queries = game_agriculture::Runtime::bind(&mut host.storage);
        let physical_query = host
            .storage
            .query2_optional::<game_physiology::Body, Activity, game_workshop::Job>();
        let environment_queries = game_environment::Runtime::bind(&mut host.storage);
        let ecology_queries = game_ecology::Runtime::bind(&mut host.storage);
        Simulation {
            host,
            engine: self.engine.clone(),
            execution: self.execution.clone(),
            profile: crate::Profile::default(),
            person_ids,
            person_index,
            inventory_index,
            pregnancy_index,
            care_index,
            dependent_index,
            agriculture_queries,
            physical_query,
            environment_queries,
            ecology_queries,
            mission_membership,
        }
    }
}

fn objective_ids(objective: &game_missions::MissionObjective, ids: &mut BTreeSet<StableId>) {
    use game_missions::MissionObjective as O;
    match objective {
        O::Visit { destination } => {
            ids.insert(StableId(destination.0));
        }
        O::Deliver {
            source,
            destination,
            ..
        } => {
            ids.extend([StableId(source.0), StableId(destination.0)]);
        }
        O::Rescue { person, return_to } => {
            ids.extend([StableId(person.0), StableId(return_to.0)]);
        }
        O::Defeat { person } | O::Confine { person, .. } => {
            ids.insert(StableId(person.0));
        }
        O::Patrol { areas, .. } => {
            ids.extend(areas.iter().map(|id| StableId(id.0)));
        }
    }
}
fn agreement_term_ids(term: &game_agreements::AgreementTerm, ids: &mut BTreeSet<StableId>) {
    use game_agreements::AgreementTerm as T;
    ids.extend(term.parties().map(|actor| StableId(actor.0)));
    match term {
        T::SiteAccess { site, .. } | T::Labor { site, .. } => {
            ids.insert(StableId(site.0));
        }
        T::OutputShare {
            site, destination, ..
        } => {
            ids.extend([StableId(site.0), StableId(destination.0)]);
        }
        T::ConstructionLabor { project, .. } => {
            ids.insert(StableId(project.0));
        }
        T::Delivery {
            source,
            destination,
            ..
        } => {
            ids.extend([StableId(source.0), StableId(destination.0)]);
        }
        T::Access { inventory, .. } => {
            ids.insert(StableId(inventory.0));
        }
    }
}
impl PreparedCombatOpeningFollowups {
    pub(crate) fn into_transaction(
        self,
        sim: &Simulation,
    ) -> Result<
        (
            game_world::Transaction<EventKind>,
            CombatOpeningFinalization,
        ),
        Error,
    > {
        if let Some(context) = &self.agreement_context {
            context.validate(sim)?;
        }
        if let Some(context) = &self.context {
            context.validate_source(sim)?;
        }
        if sim.combat_preview_inputs(&self.selected, self.encounter) != self.inputs {
            return Err(Error::Unavailable);
        }
        for guard in &self.transaction.guards {
            guard.validate(&sim.host.storage, &self.transaction.batch)?;
        }
        Ok((self.transaction, self.finish))
    }
}
fn patch_map<K: Ord + Clone, V: PartialEq + Clone>(
    actual: &mut BTreeMap<K, V>,
    before: BTreeMap<K, V>,
    after: BTreeMap<K, V>,
) {
    for (key, value) in &before {
        if after.get(key) != Some(value) {
            actual.remove(key);
        }
    }
    for (key, value) in after {
        if before.get(&key) != Some(&value) {
            actual.insert(key, value);
        }
    }
}
fn patch_inventory_index(
    actual: &mut BTreeMap<InventoryLocation, Vec<InventoryId>>,
    before: BTreeMap<InventoryLocation, Vec<InventoryId>>,
    after: BTreeMap<InventoryLocation, Vec<InventoryId>>,
) {
    for (location, ids) in &before {
        if let Some(current) = actual.get_mut(location) {
            for id in ids {
                if !after.get(location).is_some_and(|next| next.contains(id)) {
                    current.retain(|existing| existing != id);
                }
            }
        }
    }
    for (location, ids) in after {
        for id in ids {
            if !before.get(&location).is_some_and(|old| old.contains(&id)) {
                let current = actual.entry(location.clone()).or_default();
                if !current.contains(&id) {
                    current.push(id);
                }
            }
        }
    }
    actual.retain(|_, ids| !ids.is_empty());
}
impl CombatOpeningFinalization {
    pub(crate) fn finish(self, sim: &mut Simulation) {
        for (at, old) in &self.before.due {
            if let Some(actual) = sim.host.state.due.get_mut(at) {
                for due in old {
                    if !self.after.due.get(at).is_some_and(|new| new.contains(due)) {
                        actual.retain(|current| current != due);
                    }
                }
            }
        }
        for (at, new) in &self.after.due {
            for due in new {
                if !self.before.due.get(at).is_some_and(|old| old.contains(due)) {
                    let actual = sim.host.state.due.entry(*at).or_default();
                    if !actual.contains(due) {
                        actual.push(due.clone());
                    }
                }
            }
        }
        sim.host.state.due.retain(|_, due| !due.is_empty());
        macro_rules! changed { ($($field:ident),*) => { $(if self.before.$field != self.after.$field { sim.host.state.$field = self.after.$field; })* }; }
        changed!(
            ruler,
            armies,
            realm,
            transport,
            genetics,
            physical_traits,
            temperament,
            heredity,
            physical,
            decisions,
            development,
            combat,
            environment,
            agriculture,
            agreements,
            lineage,
            property,
            institutions,
            skills,
            projects,
            invocation,
            force,
            missions,
            ecology,
            health,
            social,
            enforcement
        );
        patch_inventory_index(
            &mut sim.inventory_index,
            self.inventory_before,
            self.inventory_after,
        );
        patch_map(
            &mut sim.pregnancy_index,
            self.pregnancies_before,
            self.pregnancies_after,
        );
        patch_map(&mut sim.care_index, self.care_before, self.care_after);
        for (area, old) in &self.dependents_before {
            if let Some(actual) = sim.dependent_index.get_mut(area) {
                for actor in old {
                    if !self
                        .dependents_after
                        .get(area)
                        .is_some_and(|new| new.contains(actor))
                    {
                        actual.remove(actor);
                    }
                }
            }
        }
        for (area, new) in self.dependents_after {
            for actor in new {
                if !self
                    .dependents_before
                    .get(&area)
                    .is_some_and(|old| old.contains(&actor))
                {
                    sim.dependent_index.entry(area).or_default().insert(actor);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn npc_pair(sim: &Simulation) -> Vec<(PersonId, u32)> {
        sim.people()
            .filter(|person| {
                person.id != sim.host.state.ruler
                    && sim.independent(person.id)
                    && sim.can_combat_act(person.id)
            })
            .take(2)
            .enumerate()
            .map(|(team, person)| (person.id, team as u32))
            .collect()
    }
    #[test]
    fn opening_npc_revision_failure_preserves_every_source_row_record_and_counter() {
        let mut sim = Simulation::combat_fixture(29, Execution::default()).unwrap();
        let participants = npc_pair(&sim);
        assert_eq!(participants.len(), 2);
        sim.host
            .bump_revision(u64::MAX - sim.host.clock().revision - 1)
            .unwrap();
        let before = sim.host.clone();
        assert_eq!(
            sim.debug_start_encounter(participants),
            Err(Error::Overflow)
        );
        assert_eq!(sim.host, before);
    }
    #[test]
    fn valid_large_damage_and_skilled_npc_rejection_does_not_publish_opening() {
        let mut sim = Simulation::combat_fixture(31, Execution::default()).unwrap();
        let participants = npc_pair(&sim);
        assert_eq!(participants.len(), 2);
        let runtime = game_skills::Runtime::default();
        for (actor, _) in &participants {
            game_skills::insert_skills(
                &mut sim.host.storage,
                *actor,
                game_skills::Skills::default(),
            )
            .unwrap();
            runtime
                .credit(
                    &mut sim.host.storage,
                    *actor,
                    Skill::Combat,
                    runtime.rules.per_level.0,
                    None,
                )
                .unwrap();
        }
        sim.host.state.skills = Some(runtime);
        sim.host.state.combat.as_mut().unwrap().rules.damage = u64::MAX;
        sim.host
            .state
            .combat
            .as_ref()
            .unwrap()
            .rules
            .validate()
            .unwrap();
        let before = sim.host.clone();
        assert_eq!(
            sim.debug_start_encounter(participants),
            Err(Error::Overflow)
        );
        assert_eq!(sim.host, before);
    }
    #[test]
    fn sparse_inventory_metadata_preserves_unselected_area_stock() {
        let place = InventoryLocation::Area(AreaId(1));
        let mut actual = BTreeMap::from([(place.clone(), vec![InventoryId(1), InventoryId(2)])]);
        patch_inventory_index(
            &mut actual,
            BTreeMap::from([(place.clone(), vec![InventoryId(1)])]),
            BTreeMap::from([(place.clone(), vec![InventoryId(3)])]),
        );
        assert_eq!(actual[&place], vec![InventoryId(2), InventoryId(3)]);
    }
    #[test]
    fn pending_receipt_tail_is_consumed_by_sequence_and_never_republished() {
        let mut sim = Simulation::property_fixture(41, Execution::default()).unwrap();
        let ruler = sim.host.state.ruler;
        let other = npc_pair(&sim)[0].0;
        let source = game_cargo::inventories(&sim.host.storage)
            .find(|(_, stock)| {
                stock.good == Good::Food
                    && matches!(stock.location, InventoryLocation::Area(_))
                    && stock.quantity.0 > 0
            })
            .unwrap()
            .0;
        let destination = sim.person(other).inventory;
        let quantity = Quantity(1);
        let stock = game_cargo::inventory_mut(&mut sim.host.storage, source).unwrap();
        assert!(stock.quantity.0 >= quantity.0);
        stock.quantity.0 -= quantity.0;
        let stock = game_cargo::inventory_mut(&mut sim.host.storage, destination).unwrap();
        assert!(stock.quantity.0 + quantity.0 <= stock.capacity.0);
        stock.quantity.0 += quantity.0;
        sim.host
            .append_many(vec![EventKind::Transferred {
                actor: ruler,
                source,
                destination,
                quantity,
            }])
            .unwrap();
        let encounter = sim
            .debug_start_encounter(vec![(ruler, 0), (other, 1)])
            .unwrap();
        let events = sim.host.events().to_vec();
        let next_event = sim.host.next_event();
        let prepared = sim
            .prepare_combat_opening_followups_with_property(encounter, &Batch::new(), 0, &[], 0)
            .unwrap();
        let (transaction, finish) = prepared.into_transaction(&sim).unwrap();
        assert!(transaction.events.is_empty());
        sim.host.commit_transaction(transaction).unwrap();
        finish.finish(&mut sim);
        assert_eq!(sim.host.events(), events);
        assert_eq!(sim.host.next_event(), next_event);
        assert_eq!(
            sim.host.state.property.as_ref().unwrap().last_event,
            next_event - 1
        );
        for id in [source, destination] {
            let portions: u64 = game_property::portions(&sim.host.storage, id)
                .unwrap()
                .values()
                .sum();
            assert_eq!(
                portions,
                game_cargo::inventory(&sim.host.storage, id)
                    .unwrap()
                    .quantity
                    .0
            );
        }
    }
}

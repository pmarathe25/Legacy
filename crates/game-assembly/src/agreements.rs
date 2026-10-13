//! Public bindings for native voluntary promises and their owner-prepared effects.
use crate::*;

struct OfferKnowledgeFacts<'a> {
    stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    sites: &'a BTreeMap<SiteId, decisions::SiteKnowledge>,
    projects: BTreeSet<ProjectId>,
    shares: BTreeMap<(SiteId, u64, u64), game_agreements::ShareProspect>,
    speakers: BTreeMap<PersonId, game_agreements::SpeakerFacts>,
}
impl game_agreements::OfferKnowledge for OfferKnowledgeFacts<'_> {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        self.stocks.get(&id)
    }
    fn work_site_access(&self, id: SiteId) -> bool {
        self.sites.get(&id).is_some_and(|site| site.access)
    }
    fn project_known(&self, id: ProjectId) -> bool {
        self.projects.contains(&id)
    }
    fn share_prospect(
        &self,
        id: SiteId,
        numerator: u64,
        denominator: u64,
    ) -> game_agreements::ShareProspect {
        self.shares
            .get(&(id, numerator, denominator))
            .copied()
            .unwrap_or(game_agreements::ShareProspect::Unknown)
    }
    fn speaker(&self, id: PersonId) -> Option<game_agreements::SpeakerFacts> {
        self.speakers.get(&id).copied()
    }
}

struct AgreementAssets<'a, 'b>(&'a CanonicalState<'b>);
impl game_agreements::TermAssets for AgreementAssets<'_, '_> {
    fn site_exists(&self, site: SiteId) -> bool {
        let state = self.0;
        state
            .state
            .physical
            .as_ref()
            .is_some_and(|_| game_workshop::site(state.components, site).is_some())
            || state
                .state
                .agriculture
                .as_ref()
                .is_some_and(|w| w.field(state.components, site).is_some())
            || state
                .state
                .ecology
                .as_ref()
                .is_some_and(|w| w.plot(state.components, site).is_some())
    }
    fn site_custodian(&self, site: SiteId) -> Option<PersonId> {
        let state = self.0;
        state
            .state
            .agriculture
            .as_ref()
            .and_then(|w| w.field(state.components, site))
            .and_then(|f| f.custodian)
            .or_else(|| {
                state
                    .state
                    .ecology
                    .as_ref()
                    .and_then(|w| w.plot(state.components, site))
                    .and_then(|p| p.custodian)
            })
    }
    fn project_exists(&self, id: ProjectId) -> bool {
        game_projects::project(self.0.components, id).is_some()
    }
}
impl game_agreements::OutputSources for AgreementAssets<'_, '_> {
    fn output_source(&self, site: SiteId, good: Good) -> Option<InventoryId> {
        let state = self.0;
        state
            .state
            .agriculture
            .as_ref()
            .and_then(|w| w.field(state.components, site))
            .and_then(|f| match good {
                Good::Food => Some(f.food),
                Good::Seed => Some(f.seed),
                _ => None,
            })
            .or_else(|| game_projects::site_output(state.components, site, good))
            .or_else(|| {
                state
                    .state
                    .physical
                    .as_ref()
                    .and_then(|_| game_workshop::site(state.components, site))
                    .and_then(|s| {
                        (game_cargo::inventory(state.components, s.output)
                            .expect("inventory exists")
                            .good
                            == good)
                            .then_some(s.output)
                    })
            })
    }
}
struct AgreementBindings<'a>(&'a Simulation);
impl game_agreements::Presence for AgreementBindings<'_> {
    fn contact(&self, actor: PersonId) -> Option<game_agreements::ContactFacts> {
        self.0.agreement_contact_facts(actor)
    }
}
impl game_agreements::OutputSources for AgreementBindings<'_> {
    fn output_source(&self, site: SiteId, good: Good) -> Option<InventoryId> {
        game_agreements::OutputSources::output_source(
            &AgreementAssets(&self.0.canonical_state()),
            site,
            good,
        )
    }
}
impl game_agreements::JudgmentFacts for AgreementBindings<'_> {
    fn actor_facts(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<game_agreements::OfferActorFacts, Error> {
        let mind = game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        Ok(game_agreements::OfferActorFacts {
            actor,
            now: self.0.host.clock().time,
            hunger: game_physiology::body(&self.0.host.storage, actor)
                .ok_or(Error::Unavailable)?
                .vitals
                .hunger,
            empathy: mind.personality.empathy,
            risk_tolerance: mind.personality.risk_tolerance,
            ambition: mind.personality.ambition,
            hedonism: mind.personality.hedonism,
            financing_utility: self.0.project_financing_utility(actor, terms),
        })
    }
    fn knowledge(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<impl game_agreements::OfferKnowledge, Error> {
        self.0.agreement_offer_knowledge(actor, terms)
    }
}
impl game_agreements::DutyFacts<Command> for AgreementBindings<'_> {
    fn actor_facts(&self, actor: PersonId) -> Option<game_agreements::DutyActorFacts> {
        let body = game_physiology::body(&self.0.host.storage, actor)?;
        let mind = game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)?;
        let rules = &self.0.host.state.physical.as_ref()?.rules;
        Some(game_agreements::DutyActorFacts {
            controlled: actor == self.0.host.state.ruler,
            restricted: self.0.combat_restricts(actor),
            in_combat: self.0.in_combat(actor),
            idle_or_rest: matches!(
                self.0.host.storage.activity(actor),
                Some(Activity::Idle | Activity::Rest)
            ),
            hunger: body.vitals.hunger,
            meal_relief: rules.meal_relief,
            fatigue: body.vitals.fatigue,
            fatigue_limit: rules.fatigue_limit,
            discipline: mind.personality.discipline,
            honesty: mind.personality.honesty,
        })
    }
    fn known_site(&self, actor: PersonId, site: SiteId) -> Option<game_agreements::KnownSiteRight> {
        game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)?
            .sites
            .get(&site)
            .map(|row| game_agreements::KnownSiteRight {
                site: row.site,
                area: row.area,
                accessible: row.access,
            })
    }
    fn routes(&self, actor: PersonId) -> Option<&game_logistics::KnownRoutes> {
        game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)
            .map(|mind| &mind.routes)
    }
    fn cultivated_site(&self, site: SiteId) -> bool {
        self.0
            .host
            .state
            .agriculture
            .as_ref()
            .is_some_and(|r| r.field(&self.0.host.storage, site).is_some())
    }
    fn cultivation(&self, actor: PersonId) -> Vec<(SiteId, Command)> {
        self.0
            .available_cultivation(actor)
            .into_iter()
            .filter_map(|c| {
                if let Command::Cultivate { field, .. } = c {
                    Some((field, c))
                } else {
                    None
                }
            })
            .collect()
    }
    fn construction_admitted(&self, actor: PersonId, id: ProjectId) -> bool {
        self.0
            .construction_facts(actor, id, None)
            .ok()
            .zip(self.0.host.state.projects.as_ref())
            .is_some_and(|(facts, r)| {
                r.prepare_construction(&self.0.host.storage, id, facts)
                    .is_ok()
            })
    }
    fn source_good(&self, actor: PersonId, source: InventoryId) -> Option<Good> {
        game_characters::decisions::stocks(&self.0.host.storage, actor)?
            .known
            .get(&source)
            .map(|row| row.good)
    }
    fn inventory_destination(
        &self,
        actor: PersonId,
        id: InventoryId,
    ) -> Option<game_logistics::KnownDestination> {
        self.0.delivery_known_inventory_destination(actor, id)
    }
    fn delivery_facts<'a>(
        &'a self,
        actor: PersonId,
        delivery: &game_logistics::Delivery,
        destination: &game_logistics::KnownDestination,
        cargo: &BTreeMap<InventoryId, Quantity>,
    ) -> game_logistics::DeliveryFacts<'a, Command> {
        self.0.delivery_facts_with_source_cargo(
            actor,
            delivery,
            destination,
            logistics::DeliveryProviders {
                source_cargo: cargo,
                offers: &[],
                resources: None,
            },
        )
    }
    fn acquisition(&self, actor: PersonId) -> impl game_logistics::AcquisitionSource<Command> {
        logistics::AcquisitionFacts {
            simulation: self.0,
            actor,
            resources: None,
            personality: self
                .0
                .invocation_personality(actor)
                .or_else(|| {
                    game_characters::decisions::mind::<Command>(&self.0.host.storage, actor)
                        .map(|mind| mind.personality.clone())
                })
                .unwrap_or_else(decisions::Personality::restful),
        }
    }
}
pub(crate) struct PhysicalAgreementMaintenance {
    opening: game_world::TransactionOpening,
    before: Option<game_agreements::Runtime>,
    finish: Option<game_agreements::ReconciledPublicationFinalization>,
    events: Vec<EventKind>,
    guards: Vec<game_ecs::ReadGuard>,
    contacts: game_agreements::ContactSet,
    people: BTreeSet<PersonId>,
    source_contacts: Option<game_agreements::ContactSet>,
}
impl PhysicalAgreementMaintenance {
    pub(crate) fn events(&self) -> &[EventKind] {
        &self.events
    }
    pub(crate) fn guards(&self) -> Vec<game_ecs::ReadGuard> {
        self.guards.clone()
    }
    pub(crate) fn validate(
        &self,
        simulation: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        if self.opening != simulation.host.transaction_opening()
            || simulation.host.state.agreements != self.before
        {
            return Err(Error::Unavailable);
        }
        if simulation
            .maintenance_agreement_contacts(
                batch,
                self.people.iter().copied(),
                self.source_contacts.as_ref(),
            )?
            .0
            != self.contacts.0
        {
            return Err(Error::Unavailable);
        }
        if let Some(finish) = &self.finish {
            finish.validate(
                simulation
                    .host
                    .state
                    .agreements
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &simulation.host.storage,
                batch,
            )?;
        }
        for guard in &self.guards {
            guard.validate(&simulation.host.storage, batch)?;
        }
        Ok(())
    }
    pub(crate) fn finish(self, simulation: &mut Simulation) {
        if let Some(finish) = self.finish {
            finish.finish(simulation.host.state.agreements.as_mut().unwrap());
        }
    }
}
fn reconciliation_event(event: game_agreements::ReconciliationEvent) -> EventKind {
    match event {
        game_agreements::ReconciliationEvent::Accrued {
            obligation,
            actor,
            quantity,
        } => EventKind::ObligationAccrued {
            obligation,
            actor,
            quantity,
        },
        game_agreements::ReconciliationEvent::Performed {
            obligation,
            actor,
            quantity,
        } => EventKind::ObligationPerformed {
            obligation,
            actor,
            quantity,
        },
        game_agreements::ReconciliationEvent::Defaulted { obligation } => {
            EventKind::ObligationDefaulted { obligation }
        }
    }
}
fn observation_event(kind: &EventKind, at: SimTime) -> Option<game_agreements::ObservationEvent> {
    use game_agreements::ObservationKind as O;
    let kind = match *kind {
        EventKind::AgreementProposed {
            agreement,
            proposer,
        } => O::Proposed {
            agreement,
            proposer,
        },
        EventKind::AgreementActivated { agreement } => O::Activated { agreement },
        EventKind::AgreementRejected { agreement, actor } => O::Rejected { agreement, actor },
        EventKind::ObligationAccrued {
            obligation, actor, ..
        }
        | EventKind::ObligationPerformed {
            obligation, actor, ..
        } => O::Obligation {
            obligation,
            performer: actor,
        },
        EventKind::ObligationDefaulted { obligation } => O::Defaulted { obligation },
        _ => return None,
    };
    Some(game_agreements::ObservationEvent { at, kind })
}
fn physical_receipt(
    kind: &EventKind,
    at: SimTime,
    store: &game_ecs::Store,
    batch: &game_ecs::Batch,
) -> Result<Option<game_agreements::Receipt>, Error> {
    use game_agreements::ReceiptKind as R;
    let kind = match *kind {
        EventKind::Produced {
            actor,
            site,
            output,
            quantity,
        } => R::Produced {
            actor,
            site,
            good: batch
                .projected::<game_cargo::Inventory>(store, game_ecs::StableId(output.0))?
                .ok_or(Error::Unavailable)?
                .good,
            quantity,
        },
        EventKind::CropHarvested {
            actor,
            field,
            food,
            seed,
        } => R::Harvested {
            actor,
            field,
            food,
            seed,
        },
        EventKind::Transferred {
            actor,
            source,
            destination,
            quantity,
        } => R::Transferred {
            actor,
            source,
            destination,
            quantity,
        },
        EventKind::Cared {
            source, quantity, ..
        }
        | EventKind::Ate {
            source, quantity, ..
        }
        | EventKind::Consumed {
            source, quantity, ..
        } => R::Lost { source, quantity },
        EventKind::Spoiled {
            inventory,
            quantity,
        } => R::Lost {
            source: inventory,
            quantity,
        },
        EventKind::ProjectCompleted { .. } => R::ProjectCompleted,
        _ => return Ok(None),
    };
    Ok(Some(game_agreements::Receipt { at, kind }))
}

pub(crate) struct StagedAgreementPublication {
    pub(crate) opening: game_world::TransactionOpening,
    pub(crate) ids: u64,
    pub(crate) events: Vec<EventKind>,
    pub(crate) guards: Vec<game_ecs::ReadGuard>,
    observation: game_agreements::TransitionPublicationFinalization,
    contacts: game_agreements::ContactSet,
    people: BTreeSet<PersonId>,
    source_contacts: Option<game_agreements::ContactSet>,
}
impl StagedAgreementPublication {
    pub(crate) fn validate(
        &self,
        simulation: &Simulation,
        batch: &game_ecs::Batch,
    ) -> Result<(), Error> {
        if self.opening != simulation.host.transaction_opening() {
            return Err(Error::Unavailable);
        }
        if simulation
            .maintenance_agreement_contacts(
                batch,
                self.people.iter().copied(),
                self.source_contacts.as_ref(),
            )?
            .0
            != self.contacts.0
        {
            return Err(Error::Unavailable);
        }
        self.observation.validate(
            simulation
                .host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?,
            &simulation.host.storage,
            batch,
        )?;
        for guard in &self.guards {
            guard.validate(&simulation.host.storage, batch)?;
        }
        Ok(())
    }
    pub(crate) fn finish(self, simulation: &mut Simulation) {
        self.observation
            .finish(simulation.host.state.agreements.as_mut().unwrap());
    }
}
pub(crate) fn agreement_event(event: game_agreements::TransitionEvent) -> EventKind {
    use game_agreements::TransitionEvent;
    match event {
        TransitionEvent::Proposed {
            agreement,
            proposer,
        } => EventKind::AgreementProposed {
            agreement,
            proposer,
        },
        TransitionEvent::Activated { agreement } => EventKind::AgreementActivated { agreement },
        TransitionEvent::Rejected { agreement, actor } => {
            EventKind::AgreementRejected { agreement, actor }
        }
        TransitionEvent::Amended {
            agreement,
            amendment,
        } => EventKind::AgreementAmended {
            agreement,
            amendment,
        },
        TransitionEvent::Performed {
            obligation,
            actor,
            quantity,
        } => EventKind::ObligationPerformed {
            obligation,
            actor,
            quantity,
        },
    }
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(runtime) = &s.state.agreements else {
        return game_agreements::validate_absent(s.components);
    };
    let people = s.people().map(|row| row.id).collect();
    game_agreements::validate_runtime(
        s.components,
        runtime,
        game_agreements::ValidationContext {
            people: &people,
            now: s.time,
            next_id: s.next_id,
            next_event: s.next_event,
        },
        &AgreementAssets(s),
    )?;
    game_agreements::validate_report_activities(
        s.components,
        s.components
            .activities()
            .map(|(actor, activity)| game_agreements::ReportActivityFact {
                actor,
                reporting_until: match activity {
                    Activity::ReportAgreement { completes } => Some(*completes),
                    _ => None,
                },
            }),
    )?;
    Ok(())
}
impl Simulation {
    pub fn agreement_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "agreement-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let local = sim.host.state.areas[0].id;
        let ruler = sim.host.state.ruler;
        let citizen = sim
            .people()
            .find(|p| {
                p.role == Role::Citizen
                    && *game_spatial::location(&sim.host.storage, game_ecs::StableId((p.id).0))
                        .expect("location exists")
                        == Location::AtArea(local)
            })
            .unwrap()
            .id;
        let custodians: BTreeMap<InventoryId, PersonId> =
            game_cargo::inventories(&sim.host.storage)
                .map(|(_, row)| row)
                .filter(|i| i.location == InventoryLocation::Area(local))
                .map(|i| (i.id, if i.good == Good::Food { ruler } else { citizen }))
                .collect();
        // This fixture authors one controlled store; sharing is negotiated below.
        for __stock_id in game_cargo::inventories(&sim.host.storage)
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
        {
            let inventory = game_cargo::inventory_mut(&mut sim.host.storage, __stock_id)
                .expect("inventory exists");
            if inventory.location == InventoryLocation::Area(local) && inventory.good == Good::Food
            {
                inventory.access = BTreeSet::from([ruler]);
            }
        }
        for (id, owner) in custodians {
            game_property::declare_custody(&mut sim.host.storage, id, owner)?;
        }
        sim.host.state.agreements = Some(game_agreements::Runtime {
            last_event: sim.host.next_event() - 1,
            last_observation_event: sim.host.next_event() - 1,
        });
        Ok(sim)
    }
    pub fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        game_agreements::view(&self.host.storage, actor)
    }
    fn agreement_contact_facts(&self, actor: PersonId) -> Option<game_agreements::ContactFacts> {
        self.person_index.get(&actor)?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        Some(game_agreements::ContactFacts {
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.independent(actor),
            in_combat: self.in_combat(actor),
            area: match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            },
        })
    }
    fn agreement_actor_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        game_agreements::contact_area(&AgreementBindings(self), actor).ok_or(Error::Unavailable)
    }
    fn agreement_contacts(
        &self,
        ids: impl IntoIterator<Item = PersonId>,
    ) -> game_agreements::ContactSet {
        game_agreements::ContactSet(
            ids.into_iter()
                .filter_map(|id| self.agreement_contact_facts(id).map(|facts| (id, facts)))
                .collect(),
        )
    }
    fn projected_agreement_contacts(
        &self,
        batch: &game_ecs::Batch,
        ids: impl IntoIterator<Item = PersonId>,
    ) -> Result<game_agreements::ContactSet, Error> {
        let mut contacts = BTreeMap::new();
        for actor in ids {
            if !self.person_index.contains_key(&actor) {
                continue;
            }
            let entity = game_ecs::StableId(actor.0);
            let Some(body) =
                batch.projected::<game_physiology::Body>(&self.host.storage, entity)?
            else {
                continue;
            };
            let life = batch.projected::<game_lifecycle::Life>(&self.host.storage, entity)?;
            let activity = batch.projected::<Activity>(&self.host.storage, entity)?;
            let location = batch.projected::<Location>(&self.host.storage, entity)?;
            contacts.insert(
                actor,
                game_agreements::ContactFacts {
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                    independent: game_lifecycle::independent(
                        life.map(|life| life.stage),
                        self.host.state.development.is_some(),
                    ),
                    in_combat: self.host.state.physical.is_some()
                        && matches!(activity, Some(Activity::Combat { .. })),
                    area: match location {
                        Some(Location::AtArea(area)) => Some(*area),
                        _ => None,
                    },
                },
            );
        }
        Ok(game_agreements::ContactSet(contacts))
    }
    pub(crate) fn physical_maintenance_contacts(
        &self,
        batch: &game_ecs::Batch,
    ) -> Result<game_agreements::ContactSet, Error> {
        self.projected_agreement_contacts(batch, self.person_ids.iter().copied())
    }
    fn maintenance_agreement_contacts(
        &self,
        batch: &game_ecs::Batch,
        ids: impl IntoIterator<Item = PersonId>,
        source: Option<&game_agreements::ContactSet>,
    ) -> Result<game_agreements::ContactSet, Error> {
        let ids = ids.into_iter().collect::<BTreeSet<_>>();
        let Some(source) = source else {
            return self.projected_agreement_contacts(batch, ids);
        };
        let mut contacts = source
            .0
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(id, facts)| (*id, *facts))
            .collect::<BTreeMap<_, _>>();
        let local = ids
            .iter()
            .filter(|id| self.person_index.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in &local {
            contacts.remove(id);
        }
        contacts.extend(self.projected_agreement_contacts(batch, local)?.0);
        Ok(game_agreements::ContactSet(contacts))
    }
    fn apply_agreement_learning(&mut self, effects: Vec<game_agreements::NewDefaultKnowledge>) {
        for effect in effects {
            game_characters::decisions::learn_defaults::<Command>(
                &mut self.host.storage,
                effect.observer,
                &[effect.debtor],
            );
        }
    }

    pub(crate) fn agreement_observation_events(&self) -> Vec<game_agreements::ObservationEvent> {
        use game_agreements::ObservationKind as O;
        let Some(runtime) = &self.host.state.agreements else {
            return vec![];
        };
        self.host
            .events_since(runtime.last_observation_event)
            .iter()
            .filter_map(|e| {
                let kind = match e.event {
                    EventKind::AgreementProposed {
                        agreement,
                        proposer,
                    } => O::Proposed {
                        agreement,
                        proposer,
                    },
                    EventKind::AgreementActivated { agreement } => O::Activated { agreement },
                    EventKind::AgreementRejected { agreement, actor } => {
                        O::Rejected { agreement, actor }
                    }
                    EventKind::ObligationAccrued {
                        obligation, actor, ..
                    }
                    | EventKind::ObligationPerformed {
                        obligation, actor, ..
                    } => O::Obligation {
                        obligation,
                        performer: actor,
                    },
                    EventKind::ObligationDefaulted { obligation } => O::Defaulted { obligation },
                    _ => return None,
                };
                Some(game_agreements::ObservationEvent { at: e.at, kind })
            })
            .collect::<Vec<_>>()
    }
    pub(crate) fn prepare_realm_agreement_publication(
        &self,
        prepared: game_agreements::PreparedTransition,
    ) -> Result<game_agreements::PreparedProposalPublication, Error> {
        let events = self.agreement_observation_events();
        let people = prepared.proposal_publication_people(&self.host.storage, &events)?;
        let cursor = (self.host.next_event() - 1)
            .checked_add(u64::try_from(prepared.events.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        game_agreements::PreparedProposalPublication::prepare(
            prepared,
            &self.host.storage,
            self.host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?,
            self.time(),
            cursor,
            &events,
            &self.agreement_contacts(people),
        )
    }
    pub(crate) fn observe_agreement_events(&mut self) {
        if self.host.state.agreements.is_none() {
            return;
        };
        let events = self.agreement_observation_events();
        let contacts = self.agreement_contacts(game_agreements::observation_people(
            &self.host.storage,
            &events,
        ));
        let now = self.host.clock().time;
        let cursor = self.host.next_event() - 1;
        let effects = game_agreements::observe_events(
            &mut self.host.storage,
            self.host.state.agreements.as_mut().unwrap(),
            now,
            cursor,
            &events,
            &contacts,
        )
        .expect("validated agreement observation");
        self.apply_agreement_learning(effects);
    }
    fn start_agreement_report_with_revision(
        &mut self,
        sender: PersonId,
        recipient: PersonId,
        id: AgreementId,
        origin_revision: u64,
    ) -> Result<(), Error> {
        let revisions = origin_revision.checked_add(1).ok_or(Error::Overflow)?;
        self.host.preflight_records(0, 0, revisions)?;
        let facts = game_agreements::ReportFacts {
            sender_under_care: self.care_index.contains_key(&sender),
            recipient_idle_or_rest: matches!(
                self.host.storage.activity(recipient),
                Some(Activity::Idle | Activity::Rest)
            ),
        };
        let presence = self.agreement_contacts([sender, recipient]);
        let now = self.host.clock().time;
        let pending = game_agreements::prepare_report_start(
            &self.host.storage,
            now,
            sender,
            recipient,
            id,
            facts,
            &presence,
        )?;
        self.ensure_event_capacity(1)?;
        let completes = pending.pending().completes;
        let mut batch = game_ecs::Batch::new();
        let replacement = pending.stage_into_with_replacement(
            &self.host.storage,
            now,
            facts,
            &presence,
            &mut batch,
        )?;
        let final_activities = BTreeMap::from([(sender, Activity::ReportAgreement { completes })]);
        let interruption =
            self.prepare_interruptions(&[sender], commands::ActivityOwner::Agreements)?;
        let mut finish = interruption.stage_into_with_agreement_replacement(
            self,
            &mut batch,
            &final_activities,
            &replacement,
        )?;
        let events = finish.events_for(&[sender]);
        let observation = if origin_revision > 0 {
            let runtime = self
                .host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?;
            let prior = self.agreement_observation_events();
            let people = game_agreements::observation_people(&self.host.storage, &prior);
            let contacts = self.projected_agreement_contacts(&batch, people.iter().copied())?;
            let cursor = (self.host.next_event() - 1)
                .checked_add(u64::try_from(events.len()).map_err(|_| Error::Overflow)?)
                .ok_or(Error::Overflow)?;
            let plan = game_agreements::prepare_observations(
                &self.host.storage,
                runtime,
                now,
                cursor,
                &prior,
                &contacts,
            )?;
            let trust = game_characters::receivers::prepare_default_knowledge_reactions::<Command>(
                &self.host.storage,
                plan.default_knowledge()
                    .iter()
                    .map(|effect| (effect.observer, effect.debtor)),
            )?;
            let observation = plan.stage_into(&self.host.storage, runtime, &mut batch)?;
            let trust = trust.stage_into(&self.host.storage, &mut batch)?;
            Some((observation, trust, people, contacts))
        } else {
            None
        };
        finish.validate_batch(self, &batch)?;
        let mut guards = vec![replacement.read_guard(&self.host.storage, &batch)?];
        if let Some((observation, trust, people, contacts)) = &observation {
            if self
                .projected_agreement_contacts(&batch, people.iter().copied())?
                .0
                != contacts.0
            {
                return Err(Error::Unavailable);
            }
            observation.validate(
                self.host.state.agreements.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
            trust.validate(&self.host.storage, &batch)?;
            guards.extend([observation.read_guard(), trust.clone()]);
        }
        let mut transaction = game_world::Transaction::new(self.host.transaction_opening(), batch);
        transaction.revisions = revisions;
        transaction.events = events;
        transaction.guards = guards;
        self.host.commit_transaction(transaction)?;
        finish.finish_actor_metadata(self, sender);
        if let Some((observation, _, _, _)) = observation {
            observation.finish(self.host.state.agreements.as_mut().unwrap());
        }
        self.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(sender));
        Ok(())
    }
    pub(crate) fn complete_agreement_report(&mut self, sender: PersonId) {
        let Some(pending) = game_agreements::report(&self.host.storage, sender) else {
            return;
        };
        let recipient = pending.recipient;
        let contacts = self.agreement_contacts([sender, recipient]);
        let idle = matches!(
            self.host.storage.activity(recipient),
            Some(Activity::Idle | Activity::Rest)
        );
        let now = self.host.clock().time;
        let prepared = game_agreements::prepare_report_completion(
            &self.host.storage,
            now,
            sender,
            idle,
            &contacts,
        )
        .expect("validated report completion");
        let completion = prepared.completion.clone();
        let mut batch = game_ecs::Batch::new();
        prepared
            .stage_into(&self.host.storage, &mut batch)
            .expect("current report completion");
        batch.insert(game_ecs::StableId(sender.0), Activity::Idle);
        self.host
            .storage
            .apply_batch(batch)
            .expect("admitted report completion");
        self.apply_agreement_learning(completion.knowledge);
        self.emit(if completion.reported {
            EventKind::AgreementReported {
                sender,
                recipient,
                agreement: completion.agreement,
            }
        } else {
            EventKind::Interrupted { actor: sender }
        });
    }
    pub(crate) fn agreement_accepts_delivery(
        &self,
        actor: PersonId,
        destination: InventoryId,
        quantity: Quantity,
    ) -> bool {
        let area = game_cargo::inventory(&self.host.storage, destination)
            .and_then(|row| self.inventory_area(row));
        game_agreements::accepts_delivery(
            &self.host.storage,
            actor,
            destination,
            quantity,
            self.agreement_actor_area(actor).ok(),
            area,
        )
    }
    pub(crate) fn available_agreement_commands(&self, actor: PersonId) -> Vec<Command> {
        game_agreements::available_actions(
            &self.host.storage,
            self.host.clock().time,
            actor,
            &AgreementBindings(self),
        )
        .into_iter()
        .map(|action| Command::Agreement { action })
        .collect()
    }
    pub fn debug_agreement_action(
        &mut self,
        actor: PersonId,
        action: AgreementAction,
    ) -> Result<(), Error> {
        self.agreement_command(actor, &action)
    }
    pub(crate) fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        self.agreement_command_with_revision(actor, action, 0)
    }
    pub(crate) fn agreement_command_with_revision(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
        origin_revision: u64,
    ) -> Result<(), Error> {
        self.agreement_actor_area(actor)?;
        self.reserve_agreement_ids(0)?;
        self.host
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?;
        if let AgreementAction::Report {
            agreement,
            recipient,
        } = action
        {
            return self.start_agreement_report_with_revision(
                actor,
                *recipient,
                *agreement,
                origin_revision,
            );
        }
        let requirements =
            game_agreements::Ledger::new(&self.host.storage).transition_requirements(action)?;
        let assets = self.economic_terms_context(&requirements.terms);
        let facts = game_agreements::transition_facts(
            &self.host.storage,
            actor,
            action,
            assets,
            &AgreementBindings(self),
        )?;
        let (ids, next) =
            game_agreements::transition_ids(self.host.next_id(), requirements.allocation)?;
        self.reserve_agreement_ids(next - self.host.next_id())?;
        let prepared = game_agreements::Ledger::new(&self.host.storage)
            .prepare_transition(actor, action, &facts, &ids)?;
        self.apply_prepared_agreement_with_revision(actor, prepared, next, origin_revision)
    }
    pub(crate) fn prepare_agreement_command(
        &self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(game_agreements::PreparedTransition, u64), Error> {
        self.agreement_actor_area(actor)?;
        self.reserve_agreement_ids(0)?;
        let requirements =
            game_agreements::Ledger::new(&self.host.storage).transition_requirements(action)?;
        let facts = game_agreements::transition_facts(
            &self.host.storage,
            actor,
            action,
            self.economic_terms_context(&requirements.terms),
            &AgreementBindings(self),
        )?;
        let (ids, next) =
            game_agreements::transition_ids(self.host.next_id(), requirements.allocation)?;
        self.reserve_agreement_ids(next - self.host.next_id())?;
        Ok((
            game_agreements::Ledger::new(&self.host.storage)
                .prepare_transition(actor, action, &facts, &ids)?,
            next,
        ))
    }
    pub(crate) fn prepare_service_agreement(
        &self,
        consent: &game_missions::ServiceConsent,
    ) -> Result<(game_agreements::PreparedTransition, u64), Error> {
        let area = self.agreement_actor_area(consent.recruit)?;
        if self.agreement_actor_area(consent.sponsor)? != area {
            return Err(Error::Unavailable);
        }
        let offer = self
            .host
            .state
            .missions
            .as_ref()
            .and_then(|_| game_missions::recruitment(&self.host.storage, consent.recruitment))
            .ok_or(Error::Unavailable)?;
        let mission =
            &game_missions::mission(&self.host.storage, offer.mission).expect("record exists");
        if consent.agreement.0 != self.host.next_id()
            || offer.person != consent.recruit
            || mission.sponsor != consent.sponsor
        {
            return Err(Error::Unavailable);
        }
        let count = 1 + consent.terms.len() as u64;
        self.reserve_agreement_ids(count)?;
        let next = self.host.next_id() + count;
        let facts = game_agreements::TransitionFacts {
            actor_available: true,
            present: BTreeSet::from([consent.sponsor, consent.recruit]),
            assets: self.economic_terms_context(&consent.terms),
        };
        let ids = game_agreements::TransitionIds {
            agreement: Some(consent.agreement),
            obligations: (self.host.next_id() + 1..next).map(ObligationId).collect(),
            ..Default::default()
        };
        let accepted = game_agreements::HeardServiceConsent {
            recruitment: consent.recruitment,
            sponsor: consent.sponsor,
            recruit: consent.recruit,
            offered_terms: offer.compensation.clone(),
            accepted_terms: consent.terms.clone(),
            unanswered_counteroffer: offer.countered,
        };
        let prepared = game_agreements::Ledger::new(&self.host.storage)
            .prepare_service_commitment(&accepted, &facts, &ids)?;
        Ok((prepared, next))
    }
    pub(crate) fn physical_maintenance_agreement_inputs(
        &self,
        batch: &game_ecs::Batch,
        prefix: &[EventKind],
    ) -> Result<game_agreements::MaintenanceInputs, Error> {
        let mut receipts = Vec::new();
        let mut observations = self.agreement_observation_events();
        if let Some(runtime) = self.host.state.agreements.as_ref() {
            for event in self.host.events_since(runtime.last_event).iter() {
                if let Some(receipt) =
                    physical_receipt(&event.event, event.at, &self.host.storage, batch)?
                {
                    receipts.push(receipt);
                }
            }
        }
        for event in prefix {
            if let Some(receipt) = physical_receipt(event, self.time(), &self.host.storage, batch)?
            {
                receipts.push(receipt);
            }
            if let Some(observation) = observation_event(event, self.time()) {
                observations.push(observation);
            }
        }
        Ok(game_agreements::maintenance_inputs(
            &self.host.storage,
            &receipts,
            &observations,
            &AgreementBindings(self),
        ))
    }
    pub(crate) fn prepare_physical_agreement_maintenance(
        &self,
        batch: &mut game_ecs::Batch,
        prefix: &[EventKind],
    ) -> Result<PhysicalAgreementMaintenance, Error> {
        self.prepare_physical_agreement_maintenance_from(batch, prefix, None, None)
    }
    pub(crate) fn prepare_physical_agreement_maintenance_with_context(
        &self,
        batch: &mut game_ecs::Batch,
        prefix: &[EventKind],
        source_contacts: &game_agreements::ContactSet,
        inputs: &game_agreements::MaintenanceInputs,
    ) -> Result<PhysicalAgreementMaintenance, Error> {
        self.prepare_physical_agreement_maintenance_from(
            batch,
            prefix,
            Some(source_contacts),
            Some(inputs),
        )
    }
    fn prepare_physical_agreement_maintenance_from(
        &self,
        batch: &mut game_ecs::Batch,
        prefix: &[EventKind],
        source_contacts: Option<&game_agreements::ContactSet>,
        source_inputs: Option<&game_agreements::MaintenanceInputs>,
    ) -> Result<PhysicalAgreementMaintenance, Error> {
        let opening = self.host.transaction_opening();
        let Some(runtime) = self.host.state.agreements.as_ref() else {
            return Ok(PhysicalAgreementMaintenance {
                opening,
                before: None,
                finish: None,
                events: Vec::new(),
                guards: Vec::new(),
                contacts: game_agreements::ContactSet(BTreeMap::new()),
                people: BTreeSet::new(),
                source_contacts: source_contacts.cloned(),
            });
        };
        let source_cursor = (self.host.next_event() - 1)
            .checked_add(u64::try_from(prefix.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let mut receipts = Vec::new();
        for event in self.host.events_since(runtime.last_event).iter() {
            if let Some(receipt) =
                physical_receipt(&event.event, event.at, &self.host.storage, batch)?
            {
                receipts.push(receipt);
            }
        }
        for event in prefix {
            if let Some(receipt) = physical_receipt(event, self.time(), &self.host.storage, batch)?
            {
                receipts.push(receipt);
            }
        }
        let reconciliation = if let Some(inputs) = source_inputs {
            game_agreements::prepare_reconciliation_projected(
                &self.host.storage,
                runtime,
                self.time(),
                source_cursor,
                &receipts,
                inputs,
                batch,
            )?
        } else {
            game_agreements::prepare_reconciliation_projected(
                &self.host.storage,
                runtime,
                self.time(),
                source_cursor,
                &receipts,
                &AgreementBindings(self),
                batch,
            )?
        };
        let events = reconciliation
            .events
            .iter()
            .copied()
            .map(reconciliation_event)
            .collect::<Vec<_>>();
        let observer_cursor = source_cursor
            .checked_add(u64::try_from(events.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let mut observations = self.agreement_observation_events();
        observations.extend(
            prefix
                .iter()
                .filter_map(|event| observation_event(event, self.time())),
        );
        let mut all_observations = observations.clone();
        all_observations.extend(
            events
                .iter()
                .filter_map(|event| observation_event(event, self.time())),
        );
        let people = game_agreements::observation_people(&self.host.storage, &all_observations);
        let contacts =
            self.maintenance_agreement_contacts(batch, people.iter().copied(), source_contacts)?;
        let publication = game_agreements::PreparedReconciledPublication::prepare(
            reconciliation,
            &self.host.storage,
            runtime,
            self.time(),
            observer_cursor,
            &observations,
            &contacts,
        )?;
        let trust = game_characters::receivers::prepare_default_knowledge_reactions::<Command>(
            &self.host.storage,
            publication
                .default_knowledge()
                .iter()
                .map(|effect| (effect.observer, effect.debtor)),
        )?;
        let finish = publication.stage_into(&self.host.storage, runtime, batch)?;
        let mut guards = finish.read_guards().into_iter().collect::<Vec<_>>();
        guards.push(trust.stage_into(&self.host.storage, batch)?);
        Ok(PhysicalAgreementMaintenance {
            opening,
            before: Some(runtime.clone()),
            finish: Some(finish),
            events,
            guards,
            contacts,
            people,
            source_contacts: source_contacts.cloned(),
        })
    }

    fn apply_prepared_agreement_with_revision(
        &mut self,
        actor: PersonId,
        prepared: game_agreements::PreparedTransition,
        next: u64,
        revisions: u64,
    ) -> Result<(), Error> {
        let mut batch = game_ecs::Batch::new();
        let staged = self.stage_prepared_agreement(actor, &prepared, next, &mut batch)?;
        staged.validate(self, &batch)?;
        let mut transaction = game_world::Transaction::new(staged.opening.clone(), batch);
        transaction.ids = staged.ids;
        transaction.revisions = revisions;
        transaction.events = staged.events.clone();
        transaction.guards = staged.guards.clone();
        self.host.commit_transaction(transaction)?;
        staged.finish(self);
        Ok(())
    }
    pub(crate) fn stage_prepared_agreement(
        &self,
        actor: PersonId,
        prepared: &game_agreements::PreparedTransition,
        next: u64,
        batch: &mut game_ecs::Batch,
    ) -> Result<StagedAgreementPublication, Error> {
        let opening = self.host.transaction_opening();
        let ids = next
            .checked_sub(self.host.next_id())
            .ok_or(Error::Overflow)?;
        prepared.check(&game_agreements::Ledger::new(&self.host.storage))?;
        self.ensure_event_capacity(prepared.events.len())?;
        let mut stocks = Vec::new();
        let mut fields = Vec::new();
        let mut plots = Vec::new();
        for effect in &prepared.permissions {
            match effect.asset {
                game_agreements::PermissionAsset::Inventory(id) => {
                    stocks.push((id, effect.grantee, effect.allowed))
                }
                game_agreements::PermissionAsset::Site(id) => {
                    if self
                        .host
                        .state
                        .agriculture
                        .as_ref()
                        .is_some_and(|r| r.field(&self.host.storage, id).is_some())
                    {
                        fields.push((id, effect.grantor, effect.grantee, effect.allowed));
                    } else {
                        plots.push((id, effect.grantor, effect.grantee, effect.allowed));
                    }
                }
            }
        }
        let cargo = game_cargo::prepare_access_changes(&self.host.storage, &stocks)?;
        let agriculture = self
            .host
            .state
            .agriculture
            .as_ref()
            .filter(|_| !fields.is_empty())
            .map(|r| r.prepare_site_access(&self.host.storage, &fields))
            .transpose()?;
        let ecology = self
            .host
            .state
            .ecology
            .as_ref()
            .filter(|_| !plots.is_empty())
            .map(|r| r.prepare_site_access(&self.host.storage, &plots))
            .transpose()?;
        let prior_events = self.agreement_observation_events();
        let mut people = prepared.publication_people(&self.host.storage, &prior_events);
        people.insert(actor);
        let cursor = (self.host.next_event() - 1)
            .checked_add(u64::try_from(prepared.events.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let contacts = self.projected_agreement_contacts(batch, people.iter().copied())?;
        let publication = game_agreements::PreparedTransitionPublication::prepare(
            prepared.clone(),
            &self.host.storage,
            self.host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?,
            self.time(),
            cursor,
            &prior_events,
            actor,
            &contacts,
        )?;
        let trust = game_characters::receivers::prepare_default_knowledge_reactions::<Command>(
            &self.host.storage,
            publication
                .default_knowledge()
                .iter()
                .map(|effect| (effect.observer, effect.debtor)),
        )?;
        let observation = publication.stage_into(
            &self.host.storage,
            self.host
                .state
                .agreements
                .as_ref()
                .ok_or(Error::Unavailable)?,
            batch,
        )?;
        let mut guards = observation.read_guards().into_iter().collect::<Vec<_>>();
        guards.push(trust.stage_into(&self.host.storage, batch)?);
        if let Some(skills) = self.stage_agreement_skill_events(prepared, batch)? {
            guards.push(skills.into_guard(
                self.host.state.skills.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                batch,
            )?);
        }
        guards.push(cargo.stage_into(&self.host.storage, batch)?);
        if let Some(p) = &agriculture {
            guards.push(
                p.stage_into(
                    self.host
                        .state
                        .agriculture
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    batch,
                )?,
            );
        }
        if let Some(p) = ecology {
            guards.push(p.stage_into(
                self.host.state.ecology.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                batch,
            )?);
        }
        Ok(StagedAgreementPublication {
            opening,
            ids,
            events: prepared
                .events
                .iter()
                .cloned()
                .map(agreement_event)
                .collect(),
            guards,
            observation,
            contacts,
            people,
            source_contacts: None,
        })
    }
    /// Receiving effects for an origin whose Agreement rows are already staged.
    /// The caller supplies the complete event order of the joint publication.
    pub(crate) fn stage_agreement_receivers(
        &self,
        actor: PersonId,
        prepared: &game_agreements::PreparedTransition,
        events: &[EventKind],
        batch: &mut game_ecs::Batch,
    ) -> Result<StagedAgreementPublication, Error> {
        self.stage_agreement_receivers_from(actor, prepared, events, batch, None)
    }
    pub(crate) fn stage_agreement_receivers_with_contacts(
        &self,
        actor: PersonId,
        prepared: &game_agreements::PreparedTransition,
        events: &[EventKind],
        batch: &mut game_ecs::Batch,
        contacts: &game_agreements::ContactSet,
    ) -> Result<StagedAgreementPublication, Error> {
        self.stage_agreement_receivers_from(actor, prepared, events, batch, Some(contacts))
    }
    fn stage_agreement_receivers_from(
        &self,
        actor: PersonId,
        prepared: &game_agreements::PreparedTransition,
        events: &[EventKind],
        batch: &mut game_ecs::Batch,
        source_contacts: Option<&game_agreements::ContactSet>,
    ) -> Result<StagedAgreementPublication, Error> {
        let runtime = self
            .host
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let prior_events = self.agreement_observation_events();
        let mut people = prepared.publication_people(&self.host.storage, &prior_events);
        people.insert(actor);
        let contacts =
            self.maintenance_agreement_contacts(batch, people.iter().copied(), source_contacts)?;
        let cursor = (self.host.next_event() - 1)
            .checked_add(u64::try_from(events.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        let publication = game_agreements::PreparedTransitionPublication::prepare(
            prepared.clone(),
            &self.host.storage,
            runtime,
            self.time(),
            cursor,
            &prior_events,
            actor,
            &contacts,
        )?;
        let trust = game_characters::receivers::prepare_default_knowledge_reactions::<Command>(
            &self.host.storage,
            publication
                .default_knowledge()
                .iter()
                .map(|effect| (effect.observer, effect.debtor)),
        )?;
        let observation = publication.stage_after_transition(&self.host.storage, runtime, batch)?;
        let mut guards = observation.read_guards().into_iter().collect::<Vec<_>>();
        guards.push(trust.stage_into(&self.host.storage, batch)?);
        if let Some(skills) = self.stage_agreement_skill_events(prepared, batch)? {
            guards.push(skills.into_guard(
                self.host.state.skills.as_ref().ok_or(Error::Unavailable)?,
                &self.host.storage,
                batch,
            )?);
        }
        Ok(StagedAgreementPublication {
            opening: self.host.transaction_opening(),
            ids: 0,
            events: events.to_vec(),
            guards,
            observation,
            contacts,
            people,
            source_contacts: source_contacts.cloned(),
        })
    }
    fn reserve_agreement_ids(&self, count: u64) -> Result<(), Error> {
        self.host
            .next_id()
            .checked_add(count)
            .ok_or(Error::Overflow)?;
        self.host
            .next_event()
            .checked_add(game_agreements::transition_event_reserve(
                &self.host.storage,
                count,
            )?)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn preflight_output_shares(
        &self,
        production: &[(SiteId, Good, Quantity)],
    ) -> Result<(), Error> {
        game_agreements::preflight_output_shares(&self.host.storage, production)
    }
    pub(crate) fn closing_agreement_demand(
        &self,
        at: SimTime,
        due: &[Due],
        outputs: &[(SiteId, Good, Quantity)],
    ) -> Result<(game_agreements::ClosingReceiptDemand, u64), Error> {
        let Some(world) = &self.host.state.agreements else {
            return Ok((Default::default(), 0));
        };
        let mut receipts = Vec::new();
        let mut direct = outputs.len() as u64;
        for event in self.host.events_since(world.last_event).iter() {
            match event.event {
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    ..
                } => receipts.push(game_agreements::ClosingReceipt::Transfer {
                    actor,
                    source,
                    destination,
                }),
                EventKind::Produced {
                    actor,
                    site,
                    output,
                    ..
                } => receipts.push(game_agreements::ClosingReceipt::Production {
                    actor: Some(actor),
                    site,
                    good: game_cargo::inventory(&self.host.storage, output)
                        .expect("inventory exists")
                        .good,
                }),
                EventKind::CropHarvested { actor, field, .. } => {
                    for good in [Good::Food, Good::Seed] {
                        receipts.push(game_agreements::ClosingReceipt::Production {
                            actor: Some(actor),
                            site: field,
                            good,
                        });
                    }
                }
                _ => {}
            }
        }
        for (site, good, _) in outputs {
            receipts.push(game_agreements::ClosingReceipt::Production {
                actor: None,
                site: *site,
                good: *good,
            });
        }
        for event in due {
            let transfers = match event {
                Due::Transfers(transfers) => transfers.clone(),
                Due::Physical(actor) => {
                    match *self.host.storage.activity(*actor).expect("body exists") {
                        Activity::Load {
                            source,
                            destination,
                            quantity,
                            ..
                        } => vec![TransferIntent {
                            actor: *actor,
                            source,
                            destination,
                            quantity,
                        }],
                        Activity::Eat { .. } => {
                            direct = direct.checked_add(1).ok_or(Error::Overflow)?;
                            Vec::new()
                        }
                        _ => Vec::new(),
                    }
                }
                _ => Vec::new(),
            };
            for transfer in transfers {
                direct = direct.checked_add(1).ok_or(Error::Overflow)?;
                receipts.push(game_agreements::ClosingReceipt::Transfer {
                    actor: transfer.actor,
                    source: transfer.source,
                    destination: transfer.destination,
                });
            }
        }
        let facts = game_physiology::bodies(&self.host.storage)
            .map(|(actor, body)| game_agreements::ClosingWorkFact {
                actor,
                alive: body.vitals.alive,
                health: body.vitals.health,
                target: match self.host.storage.activity(actor) {
                    Some(Activity::Work { site }) => Some(game_agreements::WorkTarget::Site(*site)),
                    Some(Activity::Cultivate { field, .. }) => {
                        Some(game_agreements::WorkTarget::Site(*field))
                    }
                    Some(Activity::Construct { project, .. }) => {
                        Some(game_agreements::WorkTarget::Project(*project))
                    }
                    _ => None,
                },
            })
            .collect::<Vec<_>>();
        let work = game_agreements::closing_work_assignments(self.host.clock().time, at, &facts);
        Ok((
            game_agreements::Ledger::new(&self.host.storage)
                .closing_receipt_demand(at, &receipts, &work)?,
            direct,
        ))
    }
    pub(crate) fn preflight_agreements(&self) -> Result<(), Error> {
        let events = self
            .closing_agreement_demand(self.host.clock().time, &[], &[])?
            .0
            .events()?;
        self.host
            .next_event()
            .checked_add(events.checked_add(2).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn economic_terms_context(
        &self,
        terms: &[AgreementTerm],
    ) -> game_agreements::TermsContext {
        game_agreements::terms_context(
            &self.host.storage,
            self.host.clock().time,
            terms,
            &AgreementAssets(&self.canonical_state()),
        )
    }
    pub(crate) fn next_agreement_deadline(&self) -> Option<SimTime> {
        game_agreements::next_deadline(&self.host.storage, self.host.clock().time)
    }
    pub(crate) fn reconcile_agreements(&mut self) -> Result<(), Error> {
        use game_agreements::ReceiptKind as R;
        let Some(runtime) = &self.host.state.agreements else {
            return Ok(());
        };
        let receipts = self
            .host
            .events_since(runtime.last_event)
            .iter()
            .filter_map(|e| {
                let kind = match e.event {
                    EventKind::Produced {
                        actor,
                        site,
                        output,
                        quantity,
                    } => R::Produced {
                        actor,
                        site,
                        good: game_cargo::inventory(&self.host.storage, output)?.good,
                        quantity,
                    },
                    EventKind::CropHarvested {
                        actor,
                        field,
                        food,
                        seed,
                    } => R::Harvested {
                        actor,
                        field,
                        food,
                        seed,
                    },
                    EventKind::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                    } => R::Transferred {
                        actor,
                        source,
                        destination,
                        quantity,
                    },
                    EventKind::Cared {
                        source, quantity, ..
                    }
                    | EventKind::Ate {
                        source, quantity, ..
                    }
                    | EventKind::Consumed {
                        source, quantity, ..
                    } => R::Lost { source, quantity },
                    EventKind::Spoiled {
                        inventory,
                        quantity,
                    } => R::Lost {
                        source: inventory,
                        quantity,
                    },
                    EventKind::ProjectCompleted { .. } => R::ProjectCompleted,
                    _ => return None,
                };
                Some(game_agreements::Receipt { at: e.at, kind })
            })
            .collect::<Vec<_>>();
        let plan = game_agreements::prepare_reconciliation(
            &self.host.storage,
            runtime,
            self.host.clock().time,
            self.host.next_event() - 1,
            &receipts,
            &AgreementBindings(self),
        )?;
        self.ensure_event_capacity(plan.events.len())?;
        let events = plan.events.clone();
        plan.commit(
            &mut self.host.storage,
            self.host.state.agreements.as_mut().unwrap(),
        )?;
        for event in events {
            self.emit(match event {
                game_agreements::ReconciliationEvent::Accrued {
                    obligation,
                    actor,
                    quantity,
                } => EventKind::ObligationAccrued {
                    obligation,
                    actor,
                    quantity,
                },
                game_agreements::ReconciliationEvent::Performed {
                    obligation,
                    actor,
                    quantity,
                } => EventKind::ObligationPerformed {
                    obligation,
                    actor,
                    quantity,
                },
                game_agreements::ReconciliationEvent::Defaulted { obligation } => {
                    EventKind::ObligationDefaulted { obligation }
                }
            });
        }
        Ok(())
    }
    pub(crate) fn review_agreements(&mut self) -> Result<(), Error> {
        if self.host.state.agreements.is_none() {
            return Ok(());
        }
        let answers = game_agreements::proposal_responses(
            &self.host.storage,
            self.host.state.ruler,
            &AgreementBindings(self),
        )?;
        for (actor, answer) in answers {
            let _ = self.agreement_command(actor, &answer);
        }
        let votes = game_agreements::amendment_votes(
            &self.host.storage,
            self.host.state.ruler,
            &AgreementBindings(self),
        )?;
        for (actor, vote) in votes {
            let _ = self.agreement_command(actor, &vote);
        }
        self.seek_local_exchange()?;
        let intentions =
            game_agreements::chosen_tasks(&self.host.storage, &AgreementBindings(self));
        for (actor, action) in intentions {
            use game_agreements::DutyAction as D;
            let command = match action {
                D::Construct { project } => Command::Project {
                    action: ProjectAction::Work { project },
                },
                D::Work { site } => Command::Work { site },
                D::Move { destination } => Command::Move { destination },
                D::Cultivate(command) => command,
                D::Travel(game_transport::RouteIntent::MoveArea { destination }) => {
                    Command::Move { destination }
                }
                D::Travel(game_transport::RouteIntent::Transport(action)) => {
                    Command::Transport { action }
                }
                D::Delivery(game_logistics::Step::Acquire(command)) => command,
                D::Delivery(game_logistics::Step::Carry { source, quantity }) => {
                    Command::Carry { source, quantity }
                }
                D::Delivery(game_logistics::Step::Transfer {
                    source,
                    destination,
                    quantity,
                }) => Command::Load {
                    source,
                    destination,
                    quantity,
                },
                D::Delivery(_) => unreachable!("owner emits actionable delivery steps"),
            };
            if let Command::Move { destination } = command {
                let _ = self.start_movement(actor, destination);
            } else {
                let _ = self.debug_actor_command(actor, command);
            }
        }
        Ok(())
    }
    fn agreement_offer_knowledge(
        &self,
        actor: PersonId,
        terms: &[AgreementTerm],
    ) -> Result<OfferKnowledgeFacts<'_>, Error> {
        let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let mind = game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            .ok_or(Error::Unavailable)?;
        let mut shares = BTreeMap::new();
        for (site, numerator, denominator) in game_agreements::share_estimate_requests(terms) {
            let fertility = game_ecology::knowledge(&self.host.storage, actor)
                .and_then(|known| known.known_plots.get(&site))
                .map(|plot| plot.soil_fertility);
            let remembered_plan = self.host.state.projects.as_ref().is_some_and(|_| {
                game_projects::remembered_farm_plan(&self.host.storage, actor, site)
            });
            let planned = game_agriculture::known_food_share(
                self.host.state.agriculture.as_ref(),
                fertility,
                remembered_plan,
                numerator,
                denominator,
            )?;
            let remembered_established = self.host.state.projects.as_ref().is_some_and(|_| {
                game_projects::remembered_established_site(&self.host.storage, actor, site)
            });
            let locally_established = self
                .host
                .state
                .agriculture
                .as_ref()
                .zip(game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId(actor.0),
                ))
                .is_some_and(|(owner, location)| {
                    owner.local_field(&self.host.storage, site, location)
                });
            let prospect = game_agreements::share_prospect_from_sources(
                planned,
                remembered_established,
                locally_established,
            );
            shares.insert((site, numerator, denominator), prospect);
        }
        let area = self.agreement_actor_area(actor).ok();
        let speakers = game_agreements::speaker_fact_requests(actor, terms)
            .into_iter()
            .map(|speaker| {
                (
                    speaker,
                    game_agreements::SpeakerFacts {
                        locally_present: area.is_some()
                            && self.agreement_actor_area(speaker).ok() == area,
                        trust: mind
                            .relationships
                            .get(&speaker)
                            .map_or(0, |relation| relation.trust),
                        observed_invocation_penalty: if area.is_some()
                            && self.agreement_actor_area(speaker).ok() == area
                        {
                            self.invocation_social_penalty(speaker)
                        } else {
                            0
                        },
                    },
                )
            })
            .collect();
        let projects = self
            .host
            .state
            .projects
            .as_ref()
            .and_then(|_| {
                game_projects::knowledge(&self.host.storage, actor).map(|row| &row.projects)
            })
            .map(|known| known.keys().copied().collect())
            .unwrap_or_default();
        Ok(OfferKnowledgeFacts {
            stocks: &stocks.known,
            sites: &mind.sites,
            projects,
            shares,
            speakers,
        })
    }
    fn seek_local_exchange(&mut self) -> Result<(), Error> {
        self.host
            .state
            .agreements
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let mut proposals = Vec::new();
        for (actor, mind) in
            game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
        {
            let stocks = game_characters::decisions::stocks(&self.host.storage, actor)
                .ok_or(Error::Unavailable)?;
            let sites: Vec<_> = mind
                .sites
                .values()
                .map(|site| game_agreements::KnownSiteRight {
                    site: site.site,
                    area: site.area,
                    accessible: site.access,
                })
                .collect();
            let facts = game_agreements::LocalExchangeFacts {
                actor,
                autonomous: actor != self.host.state.ruler,
                area: self.agreement_actor_area(actor).ok(),
                hunger: game_physiology::body(&self.host.storage, actor)
                    .ok_or(Error::Unavailable)?
                    .vitals
                    .hunger,
                stocks: &stocks.known,
                sites: &sites,
            };
            if let Some(proposal) = game_agreements::Ledger::new(&self.host.storage).local_exchange(
                self.host.clock().time,
                facts,
                &AgreementBindings(self),
            ) {
                proposals.push(proposal);
            }
        }
        for proposal in proposals {
            self.agreement_command(proposal.actor, &proposal.action)?;
            game_agreements::insert_requests(
                &mut self.host.storage,
                proposal.actor,
                game_agreements::Requests {
                    next_search: proposal.next_search,
                },
            )?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod share_tests {
    use super::*;
    #[test]
    fn routed_agreement_revision_is_admitted_before_native_publication() {
        let mut sim = Simulation::agreement_fixture(7, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let area = sim.agreement_actor_area(actor).unwrap();
        let partner = sim
            .people()
            .find(|person| {
                person.id != actor && sim.agreement_actor_area(person.id).ok() == Some(area)
            })
            .unwrap()
            .id;
        let stock = game_cargo::inventories(&sim.host.storage)
            .find(|(_, stock)| stock.access.contains(&actor))
            .unwrap()
            .0;
        let action = AgreementAction::Propose {
            parties: BTreeSet::from([actor, partner]),
            terms: vec![AgreementTerm::Access {
                grantor: actor,
                grantee: partner,
                inventory: stock,
                allowed: true,
            }],
            procedure: None,
        };
        let mut clock = sim.host.clock();
        clock.revision = u64::MAX;
        sim.host.publish_clock(clock);
        let before = sim.host.storage.snapshot().unwrap();
        let opening = sim.host.transaction_opening();
        assert_eq!(
            sim.agreement_command_with_revision(actor, &action, 1),
            Err(Error::Overflow)
        );
        assert_eq!(sim.host.storage.snapshot().unwrap(), before);
        assert_eq!(sim.host.transaction_opening(), opening);
        clock.revision = 0;
        sim.host.publish_clock(clock);
        sim.agreement_command_with_revision(actor, &action, 1)
            .unwrap();
        assert_eq!(sim.host.clock().revision, 1);
        assert_eq!(
            sim.host
                .state
                .agreements
                .as_ref()
                .unwrap()
                .last_observation_event,
            sim.host.next_event() - 1
        );
    }
    #[test]
    fn changing_future_fraction_retains_the_fraction_already_owed() {
        let (amount, remainder, scale) = game_agreements::accrue_share(0, 0, 0, 1, 3, 1).unwrap();
        assert_eq!((amount, remainder, scale), (0, 1, 3));
        let (amount, remainder, scale) =
            game_agreements::accrue_share(amount, remainder, scale, 1, 2, 1).unwrap();
        assert_eq!((amount, remainder, scale), (0, 5, 6));
        assert_eq!(
            game_agreements::accrue_share(amount, remainder, scale, 1, 2, 1).unwrap(),
            (1, 1, 3)
        );
    }
    #[test]
    fn many_small_physical_batches_owe_the_same_share_as_one_batch() {
        let mut split = (0, 0, 0);
        for quantity in [7, 1, 3, 8, 2] {
            split =
                game_agreements::accrue_share(split.0, split.1, split.2, 2, 7, quantity).unwrap();
        }
        assert_eq!(
            split,
            game_agreements::accrue_share(0, 0, 0, 2, 7, 21).unwrap()
        );
        assert_eq!(split, (6, 0, 1));
        assert_eq!(
            game_agreements::accrue_share(u64::MAX, 0, 0, 1, 1, 1),
            Err(Error::Overflow)
        );
    }
}

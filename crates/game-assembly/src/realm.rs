//! Public bindings for the regional owner's native records and prepared effects.
use crate::*;
use game_realm::{Action, PendingOwner, RealmEvent, Request, TermKey};
pub use game_realm::{PendingTalk as Talk, Rules, Runtime as World};

struct RealmBindings<'a> {
    sim: &'a Simulation,
    positional: &'a BTreeMap<AreaId, Vec<PersonId>>,
}
impl game_realm::Presence for RealmBindings<'_> {
    type Activity = Activity;
    fn actor_reads(&self, actor: PersonId) -> game_ecs::ReadFootprint {
        let id = game_ecs::StableId(actor.0);
        let mut reads = game_ecs::ReadFootprint::new();
        reads.protect::<game_physiology::Body>(id);
        reads.protect::<game_lifecycle::Life>(id);
        reads.protect::<Activity>(id);
        reads.protect::<game_property::Activity>(id);
        reads.protect::<game_invocation::ControlSession>(id);
        for (person, _) in game_invocation::sessions(&self.sim.host.storage) {
            reads.protect::<game_invocation::ControlSession>(game_ecs::StableId(person.0));
        }
        for (speaker, _) in game_missions::talks(&self.sim.host.storage)
            .filter(|(speaker, talk)| *speaker == actor || talk.listener == actor)
        {
            reads.protect::<game_missions::PendingTalk>(game_ecs::StableId(speaker.0));
        }
        reads.extend(
            game_spatial::location_reads(&self.sim.host.storage, id)
                .expect("validated carrier chain"),
        );
        reads
    }
    fn inventory_reads(&self, inventory: InventoryId) -> game_ecs::ReadFootprint {
        let mut reads = game_ecs::ReadFootprint::new();
        reads.protect::<game_cargo::Inventory>(game_ecs::StableId(inventory.0));
        if let Some(stock) = game_cargo::inventory(&self.sim.host.storage, inventory)
            && let InventoryLocation::Carried(actor) = stock.location
        {
            reads.extend(
                game_spatial::location_reads(&self.sim.host.storage, game_ecs::StableId(actor.0))
                    .expect("validated inventory carrier chain"),
            );
        }
        reads
    }

    fn actor(&self, actor: PersonId) -> Option<game_realm::ActorFacts> {
        let body = game_physiology::body(&self.sim.host.storage, actor)?;
        Some(game_realm::ActorFacts {
            alive: body.vitals.alive,
            health: body.vitals.health,
            independent: self.sim.independent(actor),
            in_combat: self.sim.in_combat(actor),
            location: self.sim.resolved_location(actor),
            idle_or_rest: matches!(
                self.sim.host.storage.activity(actor),
                Some(Activity::Idle | Activity::Rest)
            ),
            foreign_talking: self.sim.host.state.armies.as_ref().is_some_and(|w| {
                w.talks
                    .values()
                    .any(|t| t.speaker == actor || t.listener == actor)
            }) || self
                .sim
                .host
                .state
                .missions
                .as_ref()
                .is_some_and(|w| w.actor_talking(&self.sim.host.storage, actor))
                || self.sim.property_actor_talking(actor)
                || self
                    .sim
                    .host
                    .state
                    .social
                    .as_ref()
                    .is_some_and(|w| w.participants.contains_key(&actor)),
            controlled: actor == self.sim.host.state.ruler,
            past_life_controlled: self.sim.past_life_controls_actor(actor),
        })
    }
    fn same_place(&self, a: PersonId, b: PersonId) -> bool {
        self.sim.transport_same_place(a, b)
    }
    fn people(&self) -> impl Iterator<Item = PersonId> {
        self.sim.person_index.keys().copied()
    }
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId> {
        self.positional.get(&area).into_iter().flatten().copied()
    }
    fn inventory_area(&self, id: InventoryId) -> Option<AreaId> {
        game_cargo::inventory(&self.sim.host.storage, id).and_then(|i| self.sim.inventory_area(i))
    }
}
impl game_realm::PendingFacts for RealmBindings<'_> {
    fn talk_activity(&self, actor: PersonId) -> Option<game_realm::TalkActivity> {
        match self.sim.host.storage.activity(actor)? {
            Activity::RealmTalk {
                commitment,
                recipient,
                completes,
            } => Some(game_realm::TalkActivity {
                commitment: *commitment,
                recipient: *recipient,
                completes: *completes,
            }),
            _ => None,
        }
    }
    fn physical_due(&self, actor: PersonId, at: SimTime) -> bool {
        self.sim
            .host
            .state
            .due
            .get(&at)
            .is_some_and(|d| d.contains(&Due::Physical(actor)))
    }
}
impl Simulation {
    pub fn realm_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::enforcement_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "realm-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let a = sim.host.state.ruler;
        let av = sim.host.state.people[3].id;
        let b = sim.host.state.people[4].id;
        let bv = sim.host.state.people[5].id;
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
            sim.interrupt_actor_activity(actor);
        }
        for actor in [b, bv] {
            *game_spatial::location_mut(
                &mut sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0),
            )
            .expect("location exists") = Location::AtArea(AreaId(3));
        }
        let food_a = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(1)),
            Quantity(6_000),
            Quantity(6_000),
            BTreeSet::from([a]),
        )?;
        let timber_a = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(1)),
            Quantity(0),
            Quantity(6_000),
            BTreeSet::from([a]),
        )?;
        let food_b = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(0),
            Quantity(6_000),
            BTreeSet::from([b]),
        )?;
        let timber_b = sim.debug_add_inventory(
            Good::Timber,
            InventoryLocation::Area(AreaId(3)),
            Quantity(6_000),
            Quantity(6_000),
            BTreeSet::from([b]),
        )?;
        for (id, holder) in [(food_a, a), (timber_a, a), (food_b, b), (timber_b, b)] {
            game_property::declare_custody(&mut sim.host.storage, id, holder)
                .expect("declared custody");
        }
        for (area, holder, voter, stores, name) in [
            (
                AreaId(1),
                a,
                av,
                BTreeSet::from([food_a, timber_a]),
                "Willowbank trading council",
            ),
            (
                AreaId(3),
                b,
                bv,
                BTreeSet::from([food_b, timber_b]),
                "Northwood trading council",
            ),
        ] {
            sim.debug_institution_action(
                holder,
                InstitutionAction::Found {
                    charter: InstitutionCharter {
                        purpose: name.into(),
                        jurisdiction: area,
                        founders: BTreeSet::from([holder, voter]),
                        open_membership: true,
                        procedure: DecisionProcedure {
                            proposers: Subjects::members(),
                            voters: Subjects::members(),
                            minimum_support: 2,
                        },
                        offices: vec![OfficeMandate {
                            slot: 0,
                            holder,
                            inventories: stores,
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([
                                OfficePower::Negotiate,
                                OfficePower::Delivery,
                                OfficePower::Access,
                                OfficePower::Enforce,
                            ]),
                            minimum_support: 2,
                        }],
                        rules: Vec::new(),
                        contributions: Vec::new(),
                        evidence: BTreeSet::new(),
                    },
                    origin: None,
                },
            )?;
            let institution = game_society::institutions(&sim.host.storage)
                .map(|(id, _)| id)
                .next_back()
                .unwrap();
            sim.debug_institution_action(voter, InstitutionAction::Join { institution })?;
        }
        sim.initialize_realm(Rules::default())?;
        // These fixture delegates await the scripted negotiating choices. Their
        // ordinary biological needs, voting, and real agreement tasks continue.
        for actor in sim.person_index.keys().copied() {
            game_realm::insert_requests(
                &mut sim.host.storage,
                actor,
                game_realm::Requests {
                    reconsider_after: SimTime(86_400_000),
                },
            )?;
        }
        sim.review_decisions()?;
        Ok(sim)
    }
    pub fn initialize_realm(&mut self, rules: Rules) -> Result<(), Error> {
        rules.validate()?;
        if self.host.state.realm.is_some()
            || self.host.state.institutions.is_none()
            || self.host.state.agreements.is_none()
        {
            return Err(Error::Unavailable);
        }
        self.host.state.realm = Some(World {
            rules,
            last_event: self.host.next_event() - 1,
        });
        for actor in self.person_index.keys().copied() {
            game_realm::insert_knowledge(
                &mut self.host.storage,
                actor,
                game_realm::PersonalKnowledge::default(),
            )?;
            game_realm::insert_requests(
                &mut self.host.storage,
                actor,
                game_realm::Requests::default(),
            )?;
        }
        Ok(())
    }
    pub(crate) fn realm_positional_membership(
        &self,
    ) -> Result<BTreeMap<AreaId, Vec<PersonId>>, Error> {
        let roster = game_spatial::LocalRoster::build(&self.host.storage)?;
        Ok(self
            .host
            .state
            .areas
            .iter()
            .map(|area| {
                (
                    area.id,
                    roster
                        .resolved_at_area(area.id)
                        .map(|id| PersonId(id.0))
                        .collect(),
                )
            })
            .collect())
    }
    fn realm_authority(
        &self,
        actor: PersonId,
        extra: &[InstitutionId],
    ) -> game_society::institutions::AuthorityFacts {
        let ids = self
            .host
            .state
            .realm
            .as_ref()
            .map_or_else(BTreeSet::new, |w| {
                w.authority_institutions(&self.host.storage, actor, extra)
            });
        game_society::institutions::AuthorityFacts {
            alive: game_society::authority_people(&self.host.storage, ids)
                .into_iter()
                .filter(|id| {
                    game_physiology::body(&self.host.storage, *id).is_some_and(|b| b.vitals.alive)
                })
                .collect(),
        }
    }
    pub(crate) fn realm_view(&self, actor: PersonId) -> game_realm::View {
        self.host
            .state
            .realm
            .as_ref()
            .map_or_else(game_realm::View::default, |w| {
                w.view(&self.host.storage, actor)
            })
    }
    pub(crate) fn publish_realm_event(&mut self, event: RealmEvent) {
        self.emit(match event {
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
            RealmEvent::WorkRequested { key, actor } => {
                EventKind::RealmWorkRequested { key, actor }
            }
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
        });
    }
    pub(crate) fn realm_command(&mut self, actor: PersonId, action: &Action) -> Result<(), Error> {
        let positional = self.realm_positional_membership()?;
        self.host.preflight_records(0, 0, 1)?;
        let world = self.host.state.realm.as_ref().ok_or(Error::Unavailable)?;
        let extra = match action {
            Action::Propose { proposal } | Action::Revise { proposal, .. } => {
                proposal.participants.keys().copied().collect::<Vec<_>>()
            }
            _ => Vec::new(),
        };
        let authority = self.realm_authority(actor, &extra);
        let context = game_realm::RealmContext {
            now: self.time(),
            authority: &authority,
            presence: &RealmBindings {
                sim: self,
                positional: &positional,
            },
        };
        match action {
            Action::Propose { proposal } => {
                self.host.preflight_records(1, 1, 0)?;
                let id = RealmCommitmentId(self.host.next_id());
                let plan = world.prepare_proposal::<Command>(
                    &self.host.storage,
                    actor,
                    id,
                    proposal,
                    &context,
                )?;
                let mut batch = game_ecs::Batch::new();
                let guard = plan.stage_into(world, &self.host.storage, &context, &mut batch)?;
                guard.validate(&batch)?;
                self.host.storage.apply_batch(batch)?;
                self.host
                    .allocate_id()
                    .expect("preflighted regional allocation");
                self.publish_realm_event(RealmEvent::Proposed {
                    commitment: id,
                    actor,
                    revision: 0,
                });
            }
            Action::Revise {
                commitment,
                proposal,
            } => {
                self.host.preflight_records(0, 1, 0)?;
                let plan = world.prepare_revision::<Command>(
                    &self.host.storage,
                    actor,
                    *commitment,
                    proposal,
                    &context,
                )?;
                let revision = plan.revision_number().unwrap();
                let mut batch = game_ecs::Batch::new();
                let guard = plan.stage_into(world, &self.host.storage, &context, &mut batch)?;
                guard.validate(&batch)?;
                self.host.storage.apply_batch(batch)?;
                self.publish_realm_event(RealmEvent::Proposed {
                    commitment: *commitment,
                    actor,
                    revision,
                });
            }
            Action::Report {
                commitment,
                recipient,
            } => {
                let plan = world.prepare_report_start(
                    &self.host.storage,
                    self.time(),
                    actor,
                    *recipient,
                    *commitment,
                    &RealmBindings {
                        sim: self,
                        positional: &positional,
                    },
                )?;
                let pending = plan.pending().clone();
                self.ensure_event_capacity(1)?;
                let interruption =
                    self.prepare_interruptions(&[actor], commands::ActivityOwner::Realm)?;
                self.ensure_event_capacity(
                    usize::try_from(interruption.event_count()?).map_err(|_| Error::Overflow)?,
                )?;
                let mut batch = game_ecs::Batch::new();
                let replacement = plan.stage_into_with_replacement(
                    self.host.state.realm.as_ref().unwrap(),
                    &self.host.storage,
                    self.time(),
                    &RealmBindings {
                        sim: self,
                        positional: &positional,
                    },
                    &mut batch,
                )?;
                let mut finish = interruption.stage_into_with_realm_replacement(
                    self,
                    &mut batch,
                    &BTreeMap::from([(
                        actor,
                        Activity::RealmTalk {
                            commitment: *commitment,
                            recipient: *recipient,
                            completes: pending.completes,
                        },
                    )]),
                    &replacement,
                )?;
                finish.validate_batch(&batch)?;
                self.host.storage.apply_batch(batch)?;
                finish.finish_actor(self, actor);
                self.host
                    .state
                    .due
                    .entry(pending.completes)
                    .or_default()
                    .push(Due::Physical(actor));
            }
            Action::RequestWork { key } => self.realm_request_work(actor, *key, &positional)?,
        }
        self.host
            .bump_revision(1)
            .expect("preflighted regional revision");
        Ok(())
    }
    pub(crate) fn complete_realm_talk(&mut self, actor: PersonId) {
        let positional = BTreeMap::new();
        let world = self.host.state.realm.as_ref().expect("regional runtime");
        let now = self.time();
        let Ok(plan) = world.prepare_report_completion(
            &self.host.storage,
            now,
            actor,
            &RealmBindings {
                sim: self,
                positional: &positional,
            },
        ) else {
            return;
        };
        let event = plan.event.clone();
        let receiving = plan
            .reception
            .as_ref()
            .map(|r| {
                Ok::<_, Error>((
                    game_society::perception::prepare_institution_reports(
                        &self.host.storage,
                        r.recipient,
                        &r.institutions,
                    )?,
                    game_characters::decisions::prepare_stock_reports::<Command>(
                        &self.host.storage,
                        r.recipient,
                        r.sender,
                        now,
                        &r.stocks,
                    )?,
                ))
            })
            .transpose()
            .expect("validated frozen regional report");
        let mut batch = game_ecs::Batch::new();
        let (talk_guard, realm_guard) = plan
            .stage_into(
                &self.host.storage,
                now,
                &RealmBindings {
                    sim: self,
                    positional: &positional,
                },
                Activity::Idle,
                &mut batch,
            )
            .expect("fresh regional report");
        let guards = receiving
            .map(|(institutions, stocks)| {
                Ok::<_, Error>((
                    institutions.stage_into(&self.host.storage, &mut batch)?,
                    stocks.stage_into(&self.host.storage, &mut batch)?,
                ))
            })
            .transpose()
            .expect("fresh receiving memories");
        talk_guard
            .validate(&batch)
            .expect("regional report footprint");
        if let Some(g) = realm_guard {
            g.validate(&batch).expect("regional knowledge footprint");
        }
        if let Some((a, b)) = guards {
            a.validate(&batch).expect("institutional report footprint");
            b.validate(&batch).expect("stock report footprint");
        }
        self.host
            .storage
            .apply_batch(batch)
            .expect("admitted regional report");
        self.publish_realm_event(event);
    }
    pub(crate) fn realm_actor_busy(&self, actor: PersonId) -> bool {
        self.host
            .state
            .realm
            .as_ref()
            .is_some_and(|w| w.actor_busy(&self.host.storage, actor))
    }
    pub(crate) fn interrupt_realm(&mut self, actor: PersonId) {
        let Some(world) = self.host.state.realm.as_ref() else {
            return;
        };
        let plan = world.prepare_cancel_talks(&self.host.storage, &[actor]);
        let speakers = plan.speakers().collect::<Vec<_>>();
        let events = plan.events.clone();
        let mut batch = game_ecs::Batch::new();
        let guard = plan
            .stage_into(&self.host.storage, &mut batch, None)
            .expect("fresh regional interruption");
        for speaker in &speakers {
            if matches!(
                self.host.storage.activity(*speaker),
                Some(Activity::RealmTalk { .. })
            ) {
                batch.insert(game_ecs::StableId(speaker.0), Activity::Idle);
            }
        }
        guard
            .validate(&batch)
            .expect("regional interruption footprint");
        self.host
            .storage
            .apply_batch(batch)
            .expect("admitted regional interruption");
        for speaker in speakers {
            for due in self.host.state.due.values_mut() {
                due.retain(|d| *d != Due::Physical(speaker));
            }
        }
        for event in events {
            self.publish_realm_event(event);
        }
    }
    pub(crate) fn validate_realm_change(
        &self,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        if !matches!(
            change,
            InstitutionChange::RatifyRegional { .. }
                | InstitutionChange::AuthorizeRegionalPassage { .. }
        ) {
            return Ok(());
        }
        self.host
            .state
            .realm
            .as_ref()
            .ok_or(Error::Unavailable)?
            .validate_change(&self.host.storage, i, change, self.time())
    }
    pub(crate) fn realm_change_actor_allowed(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
        proposing: bool,
    ) -> bool {
        let positional = BTreeMap::new();
        if !matches!(
            change,
            InstitutionChange::RatifyRegional { .. }
                | InstitutionChange::AuthorizeRegionalPassage { .. }
        ) {
            return true;
        }
        self.host.state.realm.as_ref().is_some_and(|w| {
            w.change_actor_allowed(
                &self.host.storage,
                actor,
                i,
                change,
                proposing,
                &game_realm::RealmContext {
                    now: self.time(),
                    authority: &self.realm_authority(actor, &[i.id]),
                    presence: &RealmBindings {
                        sim: self,
                        positional: &positional,
                    },
                },
            )
        })
    }
    pub(crate) fn prepare_realm_outcome(
        &self,
        institution: InstitutionId,
        proposal: RuleProposalId,
        status: RuleProposalStatus,
    ) -> Result<Option<(game_realm::PreparedChange, RealmEvent)>, Error> {
        let positional = self.realm_positional_membership()?;
        let Some(world) = self.host.state.realm.as_ref() else {
            return Ok(None);
        };
        world
            .native_outcome(
                &self.host.storage,
                institution,
                proposal,
                status,
                self.time(),
            )?
            .map(|(plan, event)| {
                let area = game_society::institution(&self.host.storage, institution)
                    .ok_or(Error::Unavailable)?
                    .charter
                    .jurisdiction;
                let revision = plan.revision_number().ok_or(Error::Unavailable)?;
                Ok((
                    plan.publish_decision(
                        &self.host.storage,
                        revision,
                        institution,
                        area,
                        self.time(),
                        &RealmBindings {
                            sim: self,
                            positional: &positional,
                        },
                    )?,
                    event,
                ))
            })
            .transpose()
    }
    fn realm_request_work(
        &mut self,
        actor: PersonId,
        key: TermKey,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
    ) -> Result<(), Error> {
        let world = self.host.state.realm.as_ref().ok_or(Error::Unavailable)?;
        let plan = world.prepare_work::<Command>(
            &self.host.storage,
            actor,
            key,
            self.time(),
            &self.realm_authority(actor, &[]),
            &RealmBindings {
                sim: self,
                positional,
            },
        )?;
        self.ensure_event_capacity(65)?;
        let authority = self.realm_authority(actor, &[]);
        let bindings = RealmBindings {
            sim: self,
            positional,
        };
        let context = game_realm::RealmContext {
            now: self.time(),
            authority: &authority,
            presence: &bindings,
        };
        if let Some(owner) = plan.existing().cloned() {
            let mut batch = game_ecs::Batch::new();
            let guard =
                plan.bind(owner)?
                    .stage_into(world, &self.host.storage, &context, &mut batch)?;
            guard.validate(&batch)?;
            self.host.storage.apply_batch(batch)?;
            self.publish_realm_event(RealmEvent::WorkRequested { key, actor });
            return Ok(());
        }
        match plan.request().clone() {
            Request::Delivery { parties, term, .. } => {
                let (foreign, next) = self.prepare_agreement_command(
                    actor,
                    &AgreementAction::Propose {
                        parties,
                        terms: vec![term],
                        procedure: None,
                    },
                )?;
                let id = foreign
                    .created_agreements()
                    .next()
                    .ok_or(Error::Unavailable)?;
                let regional = plan.bind(PendingOwner::Agreement(id))?;
                let events = foreign.events.clone();
                self.host.preflight_records(
                    next.checked_sub(self.host.next_id())
                        .ok_or(Error::Overflow)?,
                    u64::try_from(events.len()).map_err(|_| Error::Overflow)?,
                    0,
                )?;
                let publication = self.prepare_realm_agreement_publication(foreign)?;
                let trust = game_characters::decisions::PreparedTrustReactions::prepare_defaults::<
                    Command,
                >(
                    &self.host.storage,
                    publication
                        .default_knowledge()
                        .iter()
                        .map(|effect| (effect.observer, effect.debtor)),
                )?;
                let mut batch = game_ecs::Batch::new();
                let finish = publication.stage_into(
                    &self.host.storage,
                    self.host
                        .state
                        .agreements
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &mut batch,
                )?;
                let guard = regional.stage_with_agreement_publication(
                    world,
                    &self.host.storage,
                    &context,
                    &finish,
                    &mut batch,
                )?;
                finish.validate_batch(&batch)?;
                guard.validate(&batch)?;
                let trust_finish =
                    trust.prepare_finalization::<Command>(&self.host.storage, &batch)?;
                trust_finish.validate_batch(&self.host.storage, &batch)?;
                self.host.storage.apply_batch(batch)?;
                finish.finish(self.host.state.agreements.as_mut().unwrap());
                trust_finish.finish(&mut self.host.storage);
                self.host
                    .allocate_ids(next - self.host.next_id())
                    .expect("preflighted regional agreement allocation");
                self.publish_agreement_events(events);
            }
            Request::Protection { patrol, .. } => {
                let action = MissionAction::Propose { proposal: patrol };
                let area = self.mission_area(actor)?;
                let facts = self.mission_command_facts(actor, area, &action)?;
                let runtime = self
                    .host
                    .state
                    .missions
                    .as_ref()
                    .ok_or(Error::Unavailable)?;
                let (ids, next) =
                    runtime.command_ids(&self.host.storage, &action, self.host.next_id())?;
                let foreign =
                    runtime.prepare_command(&self.host.storage, actor, &action, &facts, &ids)?;
                let id = foreign
                    .created_missions()
                    .next()
                    .ok_or(Error::Unavailable)?;
                let events = foreign.events.clone();
                let created = foreign.created_missions().collect::<Vec<_>>();
                self.host.preflight_records(
                    next.checked_sub(self.host.next_id())
                        .ok_or(Error::Overflow)?,
                    u64::try_from(events.len()).map_err(|_| Error::Overflow)?,
                    0,
                )?;
                let regional = plan.bind(PendingOwner::Mission(id))?;
                let mut batch = game_ecs::Batch::new();
                let (finish, publication_guard) = foreign.stage_created_with_publications(
                    runtime,
                    &self.host.storage,
                    self.time(),
                    &missions::MissionPresence { simulation: self },
                    &mut batch,
                )?;
                let guard = regional.stage_into(world, &self.host.storage, &context, &mut batch)?;
                publication_guard.validate_batch(&batch)?;
                guard.validate(&batch)?;
                self.host.storage.apply_batch(batch)?;
                finish.finish(self.host.state.missions.as_mut().unwrap());
                self.host
                    .allocate_ids(next - self.host.next_id())
                    .expect("preflighted regional mission allocation");
                self.mission_membership
                    .register_created(&self.host.storage, created);
                self.publish_mission_effects(events, BTreeSet::new());
            }
            Request::Passage { institution, .. } => {
                let action = InstitutionAction::Propose {
                    institution,
                    change: InstitutionChange::AuthorizeRegionalPassage {
                        commitment: key.commitment,
                        revision: key.revision,
                        index: key.index,
                    },
                };
                let foreign =
                    self.prepare_institution_transition(actor, self.social_area(actor)?, &action)?;
                let id = foreign
                    .created_proposals()
                    .next()
                    .ok_or(Error::Unavailable)?;
                let next = foreign.next_id;
                let publications = self.prepare_institution_publications(&foreign, positional)?;
                let regional = plan.bind(PendingOwner::Passage(id))?.receive_publications(
                    world,
                    &self.host.storage,
                    &context,
                    &publications,
                )?;
                self.host.preflight_records(
                    next.checked_sub(self.host.next_id())
                        .ok_or(Error::Overflow)?,
                    4,
                    0,
                )?;
                let mut batch = game_ecs::Batch::new();
                let publication_guard = foreign.stage_with_publications(
                    self.host.state.institutions.as_ref().unwrap(),
                    &self.host.storage,
                    publications,
                    &mut batch,
                )?;
                let guard = regional.stage_into(
                    world,
                    &self.host.storage,
                    &context,
                    &publication_guard,
                    &mut batch,
                )?;
                publication_guard.validate(&batch)?;
                guard.validate(&batch)?;
                self.host.storage.apply_batch(batch)?;
                self.host
                    .allocate_ids(next - self.host.next_id())
                    .expect("preflighted regional passage allocation");
                let through = self.host.next_event() - 1;
                self.host.state.realm.as_mut().unwrap().last_event = through;
            }
            Request::Ratification { .. } => return Err(Error::Unavailable),
        }
        self.publish_realm_event(RealmEvent::WorkRequested { key, actor });
        Ok(())
    }
    pub(crate) fn prepare_realm_institution_reaction(
        &self,
        publications: &[game_society::projected_publication::PreparedCommandPublication],
        initial: Vec<game_realm::PreparedChange>,
        observe: bool,
        positional: &BTreeMap<AreaId, Vec<PersonId>>,
    ) -> Result<Option<game_realm::PreparedInstitutionReaction>, Error> {
        self.host
            .state
            .realm
            .as_ref()
            .map(|w| {
                w.prepare_institution_reaction(
                    &self.host.storage,
                    self.time(),
                    publications,
                    initial,
                    observe,
                    &RealmBindings {
                        sim: self,
                        positional,
                    },
                )
            })
            .transpose()
    }
    pub(crate) fn realm_vote_score(&self, actor: PersonId, change: &InstitutionChange) -> i64 {
        self.host.state.realm.as_ref().map_or(0, |w| {
            w.vote_score::<Command>(
                &self.host.storage,
                actor,
                change,
                &self.realm_judgment(actor),
            )
        })
    }
    fn realm_judgment(&self, actor: PersonId) -> game_realm::JudgmentFacts<'_> {
        let rules = self.body_rules(actor);
        game_realm::JudgmentFacts {
            hunger: game_physiology::body(&self.host.storage, actor)
                .expect("body exists")
                .vitals
                .hunger,
            hunger_exempt: self.host.state.people[self.person_index[&actor]].hunger_exempt,
            meal_relief: rules.meal_relief,
            meal_size: rules.meal_size,
            hydration_urgency: self.observed_hydration_urgency(actor),
            drink_quantity: self
                .host
                .state
                .health
                .as_ref()
                .map_or(Quantity(u64::MAX), |w| w.rules.drink_quantity),
            concepts: &self.host.state.decisions.as_ref().unwrap().concepts,
        }
    }
    pub(crate) fn observe_realm(&mut self) -> Result<(), Error> {
        let Some(world) = self.host.state.realm.as_ref() else {
            return Ok(());
        };
        if !world.has_commitments(&self.host.storage) {
            self.host.state.realm.as_mut().unwrap().last_event = self.host.next_event() - 1;
            return Ok(());
        }
        let positional = self.realm_positional_membership()?;
        let now = self.time();
        let observation = world.prepare_observation(
            &self.host.storage,
            now,
            &RealmBindings {
                sim: self,
                positional: &positional,
            },
        )?;
        let mut batch = game_ecs::Batch::new();
        let guard = game_realm::stage_changes(observation, &self.host.storage, &mut batch)?;
        guard.validate(&batch)?;
        self.host.storage.apply_batch(batch)?;
        let ledger = game_realm::Ledger::new(&self.host.storage);
        let plans = ledger
            .reconciliation_ids()
            .into_iter()
            .map(|id| {
                ledger.prepare_reconciliation(
                    id,
                    now,
                    &RealmBindings {
                        sim: self,
                        positional: &positional,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut batch = game_ecs::Batch::new();
        let guard = game_realm::stage_changes(plans, &self.host.storage, &mut batch)?;
        guard.validate(&batch)?;
        self.host.storage.apply_batch(batch)?;
        self.host.state.realm.as_mut().unwrap().last_event = self.host.next_event() - 1;
        Ok(())
    }
    pub(crate) fn available_realm_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.host.state.realm.is_none() {
            return Vec::new();
        }
        let positional = self
            .realm_positional_membership()
            .expect("validated spatial roster");
        self.host.state.realm.as_ref().map_or_else(Vec::new, |w| {
            w.available_actions::<Command>(
                &self.host.storage,
                actor,
                self.time(),
                &self.realm_authority(actor, &[]),
                &RealmBindings {
                    sim: self,
                    positional: &positional,
                },
            )
            .into_iter()
            .map(|action| Command::Realm { action })
            .collect()
        })
    }
    pub(crate) fn review_realm(&mut self) -> Result<(), Error> {
        let positional = if self
            .host
            .state
            .realm
            .as_ref()
            .is_some_and(|w| w.has_commitments(&self.host.storage))
        {
            self.realm_positional_membership()?
        } else {
            BTreeMap::new()
        };
        self.observe_realm()?;
        let Some(world) = self.host.state.realm.as_ref() else {
            return Ok(());
        };
        let now = self.time();
        let plans = world
            .expiry_ids(&self.host.storage, now)
            .into_iter()
            .map(|id| game_realm::Ledger::new(&self.host.storage).prepare_expire(id, now))
            .collect::<Result<Vec<_>, _>>()?;
        let mut batch = game_ecs::Batch::new();
        let guard = game_realm::stage_changes(plans, &self.host.storage, &mut batch)?;
        guard.validate(&batch)?;
        self.host.storage.apply_batch(batch)?;
        for actor in self.person_index.keys().copied().collect::<Vec<_>>() {
            let authority = self.realm_authority(actor, &[]);
            let context = game_realm::RealmContext {
                now,
                authority: &authority,
                presence: &RealmBindings {
                    sim: self,
                    positional: &positional,
                },
            };
            let Some(mut plan) = self
                .host
                .state
                .realm
                .as_ref()
                .unwrap()
                .prepare_review::<Command>(
                    &self.host.storage,
                    actor,
                    &context,
                    &self.realm_judgment(actor),
                )
            else {
                continue;
            };
            while let Some(choice) = plan.next_attempt() {
                let result = match choice {
                    game_realm::ReviewChoice::Ratify {
                        institution,
                        change,
                    } => self.execute_institution_transition_with_positions(
                        actor,
                        self.social_area(actor)?,
                        &InstitutionAction::Propose {
                            institution,
                            change,
                        },
                        &positional,
                    ),
                    game_realm::ReviewChoice::Work { key } => {
                        self.realm_request_work(actor, key, &positional)
                    }
                };
                plan.receive_result(result.is_ok());
            }
            game_realm::insert_requests(&mut self.host.storage, actor, plan.reconsideration)?;
        }
        Ok(())
    }
    pub(crate) fn realm_allocation_bound(&self) -> u64 {
        self.host
            .state
            .realm
            .as_ref()
            .map_or(0, |w| w.allocation_bound(self.host.state.people.len()))
    }
    pub(crate) fn realm_boundary(&self, limit: SimTime) -> SimTime {
        self.host.state.realm.as_ref().map_or(limit, |w| {
            w.boundary(&self.host.storage, self.time(), limit)
        })
    }
    pub(crate) fn validate_realm(&self) -> Result<(), Error> {
        let positional = BTreeMap::new();
        let Some(world) = self.host.state.realm.as_ref() else {
            return Ok(());
        };
        let refs = game_realm::References {
            people: self.person_index.keys().copied().collect(),
            areas: self.host.state.areas.iter().map(|a| a.id).collect(),
            institutions: game_society::institutions(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            inventories: game_cargo::inventories(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            agreements: game_agreements::agreements(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            obligations: game_agreements::obligations(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            proposals: game_society::proposals(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            missions: game_missions::missions(&self.host.storage)
                .map(|(id, _)| id)
                .collect(),
            records: BTreeSet::new(),
        };
        world.validate(
            &self.host.storage,
            &refs,
            self.time(),
            self.host.next_event(),
            &RealmBindings {
                sim: self,
                positional: &positional,
            },
        )
    }
}

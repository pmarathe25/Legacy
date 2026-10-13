//! Assembly bindings for feature-owned enforcement decisions and canonical rows.
use crate::*;
pub use game_society::enforcement::{Case, Conversation, Talk, World};
impl Simulation {
    pub fn enforcement_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::property_fixture(seed, execution)?;
        sim.host.state.scenario = ScenarioInfo {
            id: "enforcement-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let judge = sim.host.state.ruler;
        let filer = sim.person_at(3).id;
        let officer = sim.person_at(4).id;
        let target = sim.person_at(5).id;
        let court = AreaId(1);
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.debug_actor_command(actor, Command::Cancel)?;
            sim.debug_set_personality(
                actor,
                decisions::Personality::restful(),
                decisions::Aspiration::Comfort,
            )?;
        }
        // This opening supplies a court and watch rather than the autonomy
        // fixture's recipe jobs. Old private plans must not pull witnesses away.
        let sites: Vec<_> = game_workshop::sites(&sim.host.storage)
            .map(|(id, _)| id)
            .collect();
        for site in sites {
            game_workshop::remove_site(&mut sim.host.storage, site)?;
        }
        let workers: Vec<_> = game_workshop::jobs(&sim.host.storage)
            .map(|(actor, _)| actor)
            .collect();
        for actor in workers {
            game_workshop::remove_job(&mut sim.host.storage, actor).map(|_| ())?;
        }
        let actors: Vec<_> =
            game_characters::decisions::minds::<game_api::Command>(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
        for actor in actors {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.sites.clear();
            mind.plan = None;
        }
        if let Some(world) = &mut sim.host.state.development {
            world.rules = development::Rules::human();
            let adulthood = world.rules.adulthood;
            let actors: Vec<_> = game_lifecycle::all_lives(&sim.host.storage)
                .map(|(id, _)| id)
                .collect();
            for actor in actors {
                let life = game_lifecycle::life_mut(&mut sim.host.storage, actor).unwrap();
                life.initial_age = adulthood;
                life.stage = LifeStage::Adult;
            }
            for due in sim.host.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Age(_)));
            }
        }
        for actor in sim.people().map(|p| p.id).collect::<Vec<_>>() {
            sim.schedule_age(actor);
        }
        let rules = &mut sim.host.state.physical.as_mut().unwrap().rules;
        rules.hunger_harm = 3 * 86_400_000;
        rules.meal_relief = 86_400_000;
        rules.fatigue_limit = 16 * 3_600_000;
        for actor in [judge, filer, officer, target] {
            let entity = game_ecs::StableId(sim.person(actor).id.0);
            *game_spatial::location_mut(&mut sim.host.storage, entity).expect("location exists") =
                Location::AtArea(court);
        }
        sim.host.state.missions = Some(missions::Runtime {
            last_event: sim.host.next_event() - 1,
            last_review: sim.host.clock().time,
        });
        sim.host.state.institutions = Some(institutions::Runtime {
            last_event: sim.host.next_event() - 1,
        });
        for actor in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            game_society::insert_knowledge(
                &mut sim.host.storage,
                actor,
                game_society::Knowledge::default(),
            )?;
        }
        let treasury = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(court),
            Quantity(20_000),
            Quantity(30_000),
            BTreeSet::from([judge, officer]),
        )?;
        let provisions = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(court),
            Quantity(10_000),
            Quantity(10_000),
            BTreeSet::from([officer]),
        )?;
        let binding = sim.debug_add_inventory(
            Good::Binding,
            InventoryLocation::Area(court),
            Quantity(3_000),
            Quantity(3_000),
            BTreeSet::from([officer]),
        )?;
        let tool = sim.debug_add_inventory(
            Good::Tool,
            InventoryLocation::Area(court),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([officer]),
        )?;
        for (source, custodian) in [
            (treasury, judge),
            (provisions, officer),
            (binding, officer),
            (tool, officer),
        ] {
            game_property::declare_custody(&mut sim.host.storage, source, custodian)
                .expect("declared custody");
        }
        let forest_store = sim.debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(2_000),
            Quantity(10_000),
            BTreeSet::from([filer]),
        )?;
        game_property::declare_custody(&mut sim.host.storage, forest_store, filer)
            .expect("declared custody");
        sim.debug_set_property_attitude(
            filer,
            property::Attitude {
                exclusive_claims: 900,
                sharing: 100,
                willingness_to_defend: 0,
            },
        )?;
        let founders = BTreeSet::from([judge, filer, officer, target]);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Found {
                charter: InstitutionCharter {
                    purpose: "A locally chosen court and funded watch".into(),
                    jurisdiction: court,
                    founders: founders.clone(),
                    open_membership: true,
                    procedure: DecisionProcedure {
                        proposers: Subjects::members(),
                        voters: Subjects::members(),
                        minimum_support: 2,
                    },
                    offices: vec![
                        OfficeMandate {
                            slot: 0,
                            holder: judge,
                            inventories: BTreeSet::from([treasury]),
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([OfficePower::Adjudicate]),
                            minimum_support: 2,
                        },
                        OfficeMandate {
                            slot: 1,
                            holder: officer,
                            inventories: BTreeSet::from([provisions, binding, tool]),
                            sites: BTreeSet::new(),
                            powers: BTreeSet::from([OfficePower::Enforce]),
                            minimum_support: 1,
                        },
                    ],
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
        for actor in founders.iter().copied().filter(|p| *p != judge) {
            sim.debug_institution_action(actor, InstitutionAction::Join { institution })?;
        }
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::EnforcementPolicy {
                    office: 0,
                    policy: Some(EnforcementPolicy {
                        topics: BTreeSet::from([
                            EnforcementTopic::Taking,
                            EnforcementTopic::Assault,
                            EnforcementTopic::Default,
                        ]),
                        areas: BTreeSet::from([AreaId(1), AreaId(2), AreaId(3)]),
                        subjects: Subjects::members(),
                        enforcer_office: 1,
                        remedies: BTreeSet::from([
                            RemedyKind::Dismiss,
                            RemedyKind::Settlement,
                            RemedyKind::Confinement,
                            RemedyKind::Deposition,
                        ]),
                        spending: BTreeMap::from([(treasury, Quantity(2_000))]),
                        supplies: BTreeMap::from([
                            (provisions, Quantity(3_000)),
                            (binding, Quantity(1_000)),
                            (tool, Quantity(1_000)),
                        ]),
                        maximum_confinement: SimDuration(1_200_000),
                    }),
                },
            },
        )?;
        let proposal = game_society::proposals(&sim.host.storage)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for actor in [judge, filer] {
            sim.debug_institution_action(
                actor,
                InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            )?;
        }
        let mut willing = decisions::Personality::restful();
        willing.discipline = 900;
        willing.honesty = 900;
        willing.aggression = 900;
        willing.risk_tolerance = 900;
        willing.empathy = 100;
        sim.debug_set_personality(officer, willing, decisions::Aspiration::SupportOthers)?;
        sim.debug_set_relationship(
            officer,
            judge,
            decisions::Relationship {
                trust: 800,
                affection: 500,
            },
        )?;
        sim.review_decisions()?;
        sim.observe_enforcement();
        sim.update_property_events()?;
        Ok(sim)
    }
    pub(crate) fn validate_enforcement_change(
        &self,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> Result<(), Error> {
        game_society::enforcement::System::for_change(EnforcementBindings::Read(self), change)
            .validate_enforcement_change(i, change)
    }
    pub(crate) fn enforcement_change_actor_allowed(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
        proposing: bool,
    ) -> bool {
        game_society::enforcement::System::for_actor(EnforcementBindings::Read(self), actor)
            .enforcement_change_actor_allowed(actor, i, change, proposing)
    }
    pub(crate) fn enforcement_change_ids(change: &InstitutionChange) -> u64 {
        game_society::enforcement::System::<EnforcementBindings<'_>>::enforcement_change_ids(change)
    }
    /// Primitive roster for an owner observer; scene callers overlay projected facts.
    pub(crate) fn enforcement_observer_people(
        &self,
    ) -> Vec<game_society::enforcement::PersonFacts> {
        let port = EnforcementBindings::Read(self);
        game_society::enforcement::Port::people(&port)
    }

    /// Stage the global observer from source Society rows and supplied closing facts.
    pub(crate) fn prepare_enforcement_observation_with_people(
        &self,
        batch: &mut game_ecs::Batch,
        people: &[game_society::enforcement::PersonFacts],
        assaults: &[game_society::enforcement::WitnessedAssault],
        reads: game_ecs::ReadSet,
        admitted: game_ecs::ReadFootprint,
    ) -> Result<game_society::enforcement::EnforcementObservationPublication, Error> {
        game_society::enforcement::stage_enforcement_observation(
            &self.host.storage,
            batch,
            game_society::enforcement::AdoptionObservationFacts {
                now: self.time(),
                people,
                assaults,
                last_event: self
                    .host
                    .state
                    .enforcement
                    .as_ref()
                    .map(|world| world.last_event),
            },
            reads,
            admitted,
        )
    }

    pub(crate) fn enforcement_assault_facts(
        &self,
    ) -> Vec<game_society::enforcement::WitnessedAssault> {
        self.host
            .events()
            .iter()
            .filter_map(|event| match event.event {
                EventKind::Struck {
                    encounter,
                    actor,
                    target,
                    damage,
                } => game_conflict::encounter(&self.host.storage, encounter).map(|row| {
                    game_society::enforcement::WitnessedAssault {
                        event: EventId(event.sequence),
                        attacker: actor,
                        target,
                        area: row.area,
                        damage,
                        observed_at: event.at,
                    }
                }),
                _ => None,
            })
            .collect()
    }
    pub(crate) fn enforcement_view(&self, actor: PersonId) -> EnforcementView {
        if self.host.state.enforcement.is_none() {
            return EnforcementView::default();
        }
        game_society::enforcement::System::for_actor(EnforcementBindings::Read(self), actor)
            .enforcement_view(actor)
    }
    pub fn debug_enforcement_action(
        &mut self,
        actor: PersonId,
        action: EnforcementAction,
    ) -> Result<(), Error> {
        self.enforcement_command(actor, &action)
    }
    pub(crate) fn enforcement_command(
        &mut self,
        actor: PersonId,
        action: &EnforcementAction,
    ) -> Result<(), Error> {
        let mut system = game_society::enforcement::System::for_action(
            EnforcementBindings::Write(self),
            actor,
            action,
        );
        system.enforcement_command(actor, action)?;
        system.publish()
    }
    pub(crate) fn complete_enforcement_talk(&mut self, actor: PersonId) {
        if self.host.state.enforcement.is_none() {
            return;
        }
        let mut system =
            game_society::enforcement::System::for_actor(EnforcementBindings::Write(self), actor);
        system.complete_enforcement_talk(actor);
        system.publish().expect("admitted enforcement conversation");
    }
    pub(crate) fn observe_enforcement(&mut self) {
        if self.host.state.enforcement.is_none() {
            return;
        }
        let mut system = game_society::enforcement::System::new(EnforcementBindings::Write(self));
        system.observe_enforcement();
        system.publish().expect("admitted enforcement observation");
    }
    pub(crate) fn review_enforcement(&mut self) -> Result<(), Error> {
        if self.host.state.enforcement.is_none() {
            return Ok(());
        }
        let mut system = game_society::enforcement::System::new(EnforcementBindings::Write(self));
        system.review_enforcement()?;
        system.publish()
    }
    pub(crate) fn available_enforcement_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.host.state.enforcement.is_none() {
            return Vec::new();
        }
        game_society::enforcement::System::for_actor(EnforcementBindings::Read(self), actor)
            .available_enforcement_commands(actor)
            .into_iter()
            .map(|action| Command::Enforcement { action })
            .collect()
    }
    pub(crate) fn validate_enforcement(&self) -> Result<(), Error> {
        game_society::enforcement::validate_runtime_presence(
            &self.host.storage,
            self.host.state.enforcement.as_ref(),
        )?;
        if self.host.state.enforcement.is_none() {
            return Ok(());
        }
        game_society::enforcement::System::for_validation(EnforcementBindings::Read(self))
            .validate_enforcement()
    }
    pub(crate) fn enforcement_allocation_bound(&self) -> u64 {
        game_society::enforcement::allocation_bound(
            self.host.state.enforcement.as_ref(),
            self.people().len(),
        )
    }
    pub(crate) fn enforcement_vote_score(
        &self,
        actor: PersonId,
        i: &institutions::Institution,
        change: &InstitutionChange,
    ) -> i64 {
        game_society::enforcement::System::for_actor(EnforcementBindings::Read(self), actor)
            .enforcement_vote_score(actor, i, change)
    }
}

/// Adapters expose factual components and delegate operations to their owners.
/// Enforcement's decisions and projected row changes live in Society.
enum EnforcementBindings<'a> {
    Read(&'a Simulation),
    Write(&'a mut Simulation),
}
impl EnforcementBindings<'_> {
    fn sim(&self) -> &Simulation {
        match self {
            Self::Read(sim) => sim,
            Self::Write(sim) => sim,
        }
    }
    fn sim_mut(&mut self) -> &mut Simulation {
        match self {
            Self::Write(sim) => sim,
            Self::Read(_) => panic!("read binding cannot publish"),
        }
    }
}
impl game_society::enforcement::Port for EnforcementBindings<'_> {
    fn store(&self) -> &game_ecs::Store {
        &self.sim().host.storage
    }
    fn runtime(&self) -> Option<&World> {
        self.sim().host.state.enforcement.as_ref()
    }
    fn set_runtime(&mut self, world: World) {
        self.sim_mut().host.state.enforcement = Some(world);
    }
    fn now(&self) -> SimTime {
        self.sim().time()
    }
    fn next_id(&self) -> u64 {
        self.sim().host.next_id()
    }
    fn next_event(&self) -> u64 {
        self.sim().host.next_event()
    }
    fn ruler(&self) -> PersonId {
        self.sim().host.state.ruler
    }
    fn people(&self) -> Vec<game_society::enforcement::PersonFacts> {
        self.sim()
            .people()
            .filter_map(|person| self.person(person.id))
            .collect()
    }
    fn local_people(&self, area: AreaId) -> Vec<game_society::enforcement::PersonFacts> {
        game_spatial::LocalRoster::build(&self.sim().host.storage)
            .expect("validated spatial roster")
            .raw_at_area(area)
            .filter_map(|id| self.person(PersonId(id.0)))
            .collect()
    }
    fn person(&self, actor: PersonId) -> Option<game_society::enforcement::PersonFacts> {
        let sim = self.sim();
        sim.person_index.get(&actor)?;
        let person = sim.person(actor);
        Some(game_society::enforcement::PersonFacts {
            id: actor,
            inventory: person.inventory,
            location: game_spatial::location(&sim.host.storage, game_ecs::StableId(actor.0))?
                .clone(),
            vitals: game_physiology::body(&sim.host.storage, actor)?
                .vitals
                .clone(),
            independent: sim.independent(actor),
            combat_restricted: sim.combat_restricts(actor),
        })
    }
    fn areas(&self) -> Vec<AreaId> {
        self.sim()
            .host
            .state
            .areas
            .iter()
            .map(|area| area.id)
            .collect()
    }
    fn authority(&self, people: &BTreeSet<PersonId>) -> game_society::institutions::AuthorityFacts {
        game_society::institutions::AuthorityFacts {
            alive: people
                .iter()
                .copied()
                .filter(|actor| {
                    game_physiology::body(&self.sim().host.storage, *actor)
                        .is_some_and(|body| body.vitals.alive)
                })
                .collect(),
        }
    }
    fn mind(&self, actor: PersonId) -> game_society::enforcement::MindFacts {
        let row =
            game_characters::decisions::mind::<game_api::Command>(&self.sim().host.storage, actor)
                .expect("character mind");
        game_society::enforcement::MindFacts {
            personality: row.personality.clone(),
            relationships: row.relationships.clone(),
        }
    }
    fn physical_rules(&self) -> Option<game_society::enforcement::PhysicalFacts> {
        self.sim().host.state.physical.as_ref().map(|runtime| {
            game_society::enforcement::PhysicalFacts {
                meal_size: runtime.rules.meal_size,
            }
        })
    }
    fn in_combat(&self, actor: PersonId) -> bool {
        self.sim().in_combat(actor)
    }
    fn past_life_controls_actor(&self, actor: PersonId) -> bool {
        self.sim().past_life_controls_actor(actor)
    }
    fn idle_or_resting(&self, actor: PersonId) -> bool {
        matches!(
            self.sim().host.storage.activity(actor),
            Some(Activity::Idle | Activity::Rest)
        )
    }
    fn talk_activity(&self, actor: PersonId) -> Option<(EnforcementCaseId, PersonId, SimTime)> {
        match self.sim().host.storage.activity(actor)? {
            Activity::EnforcementTalk {
                case,
                recipient,
                completes,
            } => Some((*case, *recipient, *completes)),
            _ => None,
        }
    }
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.sim()
            .host
            .state
            .due
            .get(&at)
            .is_some_and(|due| due.contains(&Due::Physical(actor)))
    }
    fn agreements_available(&self) -> bool {
        self.sim().host.state.agreements.is_some()
    }
    fn missions_available(&self) -> bool {
        self.sim().host.state.missions.is_some()
    }
    fn institutions_available(&self) -> bool {
        self.sim().host.state.institutions.is_some()
    }
    fn combat_available(&self) -> bool {
        self.sim().host.state.combat.is_some()
    }
    fn property_available(&self) -> bool {
        self.sim().host.state.property.is_some()
    }
    fn events(&self) -> Vec<game_society::enforcement::StrikeRecord> {
        self.sim()
            .host
            .events()
            .iter()
            .filter_map(|event| match event.event {
                EventKind::Struck {
                    encounter,
                    actor,
                    target,
                    damage,
                } => Some(game_society::enforcement::StrikeRecord {
                    sequence: event.sequence,
                    at: event.at,
                    event: game_society::enforcement::Strike::Struck {
                        encounter,
                        actor,
                        target,
                        damage,
                    },
                }),
                _ => None,
            })
            .collect()
    }
    fn encounter_area(&self, id: EncounterId) -> Option<AreaId> {
        game_conflict::encounter(&self.sim().host.storage, id).map(|row| row.area)
    }
    fn incidents(&self, actor: PersonId) -> Vec<game_property::IncidentView> {
        self.sim().property_view(actor).incidents
    }
    fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView> {
        self.sim().institution_view(actor)
    }
    fn agreement_view(&self, actor: PersonId) -> Vec<AgreementView> {
        self.sim().agreement_view(actor)
    }
    fn social_case_view(&self, actor: PersonId) -> Vec<SocialCaseView> {
        self.sim().social_case_view(actor)
    }
    fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        self.sim().mission_view(actor)
    }
    fn preflight(&self, revisions: u64) -> Result<(), Error> {
        self.sim().host.preflight_records(0, 0, revisions)
    }
    fn bump_revision(&mut self, revisions: u64) -> Result<(), Error> {
        self.sim_mut().host.bump_revision(revisions)
    }
    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error> {
        self.sim().ensure_event_capacity(count)
    }
    fn agreement_command(
        &mut self,
        actor: PersonId,
        action: &AgreementAction,
    ) -> Result<(), Error> {
        self.sim_mut().agreement_command(actor, action)
    }
    fn institution_command(
        &mut self,
        actor: PersonId,
        action: &InstitutionAction,
    ) -> Result<(), Error> {
        self.sim_mut().institution_command(actor, action)
    }
    fn mission_command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error> {
        self.sim_mut().mission_command(actor, action)
    }
    fn preflight_mission_target_report(
        &self,
        proposal: &MissionProposal,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error> {
        self.sim()
            .preflight_mission_target_report(proposal, sender, target, area, at)
    }
    fn seed_mission_target_report(
        &mut self,
        mission: MissionId,
        sender: PersonId,
        target: PersonId,
        area: AreaId,
        at: SimTime,
    ) -> Result<(), Error> {
        self.sim_mut()
            .seed_mission_target_report(mission, sender, target, area, at)
    }
    fn publish(
        &mut self,
        mut batch: game_ecs::Batch,
        guard: game_ecs::ReadGuard,
        activities: Vec<game_society::enforcement::ActivityEffect>,
        events: Vec<game_society::enforcement::Event>,
        allocations: u64,
        interruptions: Vec<PersonId>,
    ) -> Result<(), Error> {
        use game_society::enforcement::{ActivityEffect, Event};
        let sim = self.sim_mut();
        let mut deadlines = Vec::new();
        let mut final_activities = BTreeMap::new();
        for effect in activities {
            match effect {
                ActivityEffect::Talk {
                    actor,
                    case,
                    recipient,
                    completes,
                } => {
                    final_activities.insert(
                        actor,
                        Activity::EnforcementTalk {
                            case,
                            recipient,
                            completes,
                        },
                    );
                    deadlines.push((actor, completes));
                }
                ActivityEffect::Idle { actor } => {
                    final_activities.insert(actor, Activity::Idle);
                }
            }
        }
        let interruption =
            sim.prepare_interruptions(&interruptions, commands::ActivityOwner::Enforcement)?;
        sim.host.preflight_records(allocations, 0, 0)?;
        sim.ensure_event_capacity(
            events
                .len()
                .checked_add(
                    usize::try_from(interruption.event_count()?).map_err(|_| Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?,
        )?;
        let mut finish = interruption.stage_into(sim, &mut batch, &final_activities)?;
        guard.validate(&sim.host.storage, &batch)?;
        finish.validate_batch(sim, &batch)?;
        sim.host.storage.apply_batch(batch)?;
        sim.host.allocate_ids(allocations)?;
        for actor in interruptions {
            finish.finish_actor(sim, actor);
        }
        for (actor, at) in deadlines {
            sim.host
                .state
                .due
                .entry(at)
                .or_default()
                .push(Due::Physical(actor));
        }
        for event in events {
            sim.emit(match event {
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
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod atomic_tests {
    use super::*;
    fn actor(sim: &Simulation, index: usize) -> PersonId {
        sim.person_at(index).id
    }
    fn court(sim: &Simulation) -> InstitutionId {
        game_society::institutions(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next()
            .unwrap()
    }
    fn role(sim: &Simulation) -> PersonId {
        sim.canonical_state().state.ruler
    }
    fn travel(sim: &mut Simulation, who: PersonId, destination: AreaId) {
        sim.debug_move_actor(who, destination).unwrap();
        let Location::InTransit { arrives, .. } = *game_spatial::location(
            sim.canonical_state().components,
            game_ecs::StableId((who).0),
        )
        .expect("location exists") else {
            panic!("traveler did not depart")
        };
        sim.advance(SimDuration(arrives.0 - sim.canonical_state().time.0))
            .unwrap();
    }
    fn theft(sim: &mut Simulation) -> IncidentId {
        let filer = actor(sim, 3);
        let target = actor(sim, 5);
        travel(sim, filer, AreaId(3));
        travel(sim, target, AreaId(3));
        let source = game_cargo::inventories(sim.canonical_state().components)
            .map(|(_, row)| row)
            .find(|i| {
                i.good == Good::Food
                    && i.location == InventoryLocation::Area(AreaId(3))
                    && i.access == BTreeSet::from([filer])
            })
            .unwrap()
            .id;
        let destination = sim.people().find(|p| p.id == target).unwrap().inventory;
        sim.debug_property_action(
            target,
            PropertyAction::Take {
                source,
                destination,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        sim.property_view(filer).incidents.last().unwrap().id
    }
    fn file(sim: &mut Simulation, incident: IncidentId) -> EnforcementCaseId {
        let filer = actor(sim, 3);
        travel(sim, filer, AreaId(1));
        sim.debug_enforcement_action(
            filer,
            EnforcementAction::File {
                institution: court(sim),
                office: 0,
                evidence: EnforcementEvidenceRef::Incident(incident),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        game_society::enforcement::cases(&sim.host.storage)
            .map(|(id, _)| id)
            .last()
            .unwrap()
    }
    fn decide(sim: &mut Simulation, case: EnforcementCaseId, remedy: EnforcementRemedy) {
        let judge = role(sim);
        let filer = actor(sim, 3);
        let institution = court(sim);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::Adjudicate { case, remedy },
            },
        )
        .unwrap();
        let proposal = game_society::proposals(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        for who in [judge, filer] {
            sim.debug_institution_action(
                who,
                InstitutionAction::Vote {
                    proposal,
                    support: true,
                },
            )
            .unwrap();
        }
    }

    fn exhausted_command_revision(sim: &Simulation) -> Simulation {
        let mut copy = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        copy.host.publish_clock(game_sim::Clock {
            revision: u64::MAX,
            ..copy.host.clock()
        });
        copy
    }

    #[test]
    fn exhausted_command_revision_cannot_commit_a_vote_without_its_judgment() {
        let mut sim = Simulation::enforcement_fixture(36, Execution::default()).unwrap();
        let incident = theft(&mut sim);
        let case = file(&mut sim, incident);
        let judge = role(&sim);
        let institution = court(&sim);
        sim.debug_institution_action(
            judge,
            InstitutionAction::Propose {
                institution,
                change: InstitutionChange::Adjudicate {
                    case,
                    remedy: EnforcementRemedy::Dismiss,
                },
            },
        )
        .unwrap();
        let proposal = game_society::proposals(sim.canonical_state().components)
            .map(|(id, _)| id)
            .next_back()
            .unwrap();
        sim.debug_institution_action(
            judge,
            InstitutionAction::Vote {
                proposal,
                support: true,
            },
        )
        .unwrap();
        let mut sim = exhausted_command_revision(&sim);
        let before = sim.canonical_snapshot().unwrap();
        let filer = actor(&sim, 3);
        assert_eq!(
            sim.debug_actor_command(
                filer,
                Command::Institution {
                    action: InstitutionAction::Vote {
                        proposal,
                        support: true
                    }
                }
            ),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn exhausted_command_revision_cannot_create_a_promise_without_its_response() {
        let mut sim = Simulation::enforcement_fixture(37, Execution::default()).unwrap();
        let incident = theft(&mut sim);
        let case = file(&mut sim, incident);
        let target = actor(&sim, 5);
        let filer = actor(&sim, 3);
        travel(&mut sim, target, AreaId(1));
        let source = sim.people().find(|p| p.id == target).unwrap().inventory;
        let destination = sim.people().find(|p| p.id == filer).unwrap().inventory;
        let deadline = SimTime(sim.canonical_state().time.0 + 600_000);
        decide(
            &mut sim,
            case,
            EnforcementRemedy::Settlement {
                terms: vec![AgreementTerm::Delivery {
                    debtor: target,
                    creditor: filer,
                    source,
                    destination,
                    quantity: Quantity(1_000),
                    deadline,
                }],
            },
        );
        let mut sim = exhausted_command_revision(&sim);
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.debug_enforcement_action(target, EnforcementAction::Respond { case, comply: true }),
            Err(Error::Overflow)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }
}

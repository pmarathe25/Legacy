use super::*;

struct Fixture {
    store: Store,
    runtime: Option<World>,
    now: SimTime,
    next: u64,
    next_event: u64,
    revision: u64,
    people: BTreeMap<PersonId, PersonFacts>,
    events: Vec<Event>,
    activity: BTreeMap<PersonId, (EnforcementCaseId, PersonId, SimTime)>,
}
impl Fixture {
    fn new() -> Self {
        let mut schema = game_ecs::Schema::new();
        crate::storage::register_components(&mut schema).unwrap();
        crate::enforcement::register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        game_agreements::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let members = BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)]);
        let mut institution =
            crate::policy_fixtures::institution(10, Vec::new(), InstitutionStatus::Active);
        institution.members = members.clone();
        institution.charter.founders = members.clone();
        institution.charter.offices = vec![
            OfficeMandate {
                slot: 0,
                holder: PersonId(2),
                inventories: BTreeSet::new(),
                sites: BTreeSet::new(),
                powers: BTreeSet::from([OfficePower::Adjudicate]),
                minimum_support: 1,
            },
            OfficeMandate {
                slot: 1,
                holder: PersonId(2),
                inventories: BTreeSet::new(),
                sites: BTreeSet::new(),
                powers: BTreeSet::from([OfficePower::Enforce]),
                minimum_support: 1,
            },
        ];
        institution.office_support = BTreeMap::from([
            (0, BTreeSet::from([PersonId(1)])),
            (1, BTreeSet::from([PersonId(1)])),
        ]);
        crate::insert_institution(&mut store, InstitutionId(10), institution).unwrap();
        let policy = EnforcementPolicyView {
            institution: InstitutionId(10),
            office: 0,
            policy: EnforcementPolicy {
                topics: BTreeSet::from([EnforcementTopic::Assault]),
                areas: BTreeSet::from([AreaId(3)]),
                subjects: Subjects::members(),
                enforcer_office: 1,
                remedies: BTreeSet::from([RemedyKind::Dismiss, RemedyKind::Confinement]),
                spending: BTreeMap::new(),
                supplies: BTreeMap::new(),
                maximum_confinement: SimDuration(120_000),
            },
            adopted_at: SimTime(0),
            observed_at: SimTime(0),
        };
        policies_mut(&mut store, InstitutionId(10))
            .unwrap()
            .offices
            .insert(0, policy.clone());
        let memory = memory_mut(&mut store, PersonId(1)).unwrap();
        memory
            .known_policies
            .insert(InstitutionId(10), BTreeMap::from([(0, policy)]));
        memory.assaults.insert(
            EventId(1),
            WitnessedAssault {
                event: EventId(1),
                attacker: PersonId(3),
                target: PersonId(1),
                area: AreaId(3),
                damage: 1,
                observed_at: SimTime(0),
            },
        );
        let authority = AuthorityFacts {
            alive: members.clone(),
        };
        let view = crate::institutions::Runtime::default().actual_institution_view(
            &store,
            crate::institution(&store, InstitutionId(10)).unwrap(),
            &authority,
            SimTime(0),
        );
        for actor in &members {
            crate::insert_knowledge(
                &mut store,
                *actor,
                crate::Knowledge {
                    institutions: BTreeMap::from([(view.id, view.clone())]),
                    ..crate::Knowledge::default()
                },
            )
            .unwrap();
        }
        let people = members
            .into_iter()
            .map(|id| {
                (
                    id,
                    PersonFacts {
                        id,
                        inventory: InventoryId(id.0 + 100),
                        location: Location::AtArea(AreaId(3)),
                        vitals: Vitals {
                            hunger: 0,
                            fatigue: 0,
                            health: 100,
                            alive: true,
                        },
                        independent: true,
                        combat_restricted: false,
                    },
                )
            })
            .collect();
        Self {
            store,
            runtime: Some(World { last_event: 0 }),
            now: SimTime(0),
            next: 20,
            next_event: 2,
            revision: 0,
            people,
            events: Vec::new(),
            activity: BTreeMap::new(),
        }
    }
}
impl Port for Fixture {
    fn store(&self) -> &Store {
        &self.store
    }
    fn runtime(&self) -> Option<&World> {
        self.runtime.as_ref()
    }
    fn set_runtime(&mut self, world: World) {
        self.runtime = Some(world);
    }
    fn now(&self) -> SimTime {
        self.now
    }
    fn next_id(&self) -> u64 {
        self.next
    }
    fn next_event(&self) -> u64 {
        self.next_event
    }
    fn ruler(&self) -> PersonId {
        PersonId(2)
    }
    fn people(&self) -> Vec<PersonFacts> {
        self.people.values().cloned().collect()
    }
    fn local_people(&self, area: AreaId) -> Vec<PersonFacts> {
        self.people
            .values()
            .filter(|person| person.location == Location::AtArea(area))
            .cloned()
            .collect()
    }
    fn person(&self, actor: PersonId) -> Option<PersonFacts> {
        self.people.get(&actor).cloned()
    }
    fn areas(&self) -> Vec<AreaId> {
        vec![AreaId(3), AreaId(4)]
    }
    fn authority(&self, people: &BTreeSet<PersonId>) -> AuthorityFacts {
        AuthorityFacts {
            alive: people
                .iter()
                .copied()
                .filter(|actor| self.people.get(actor).is_some_and(|row| row.vitals.alive))
                .collect(),
        }
    }
    fn mind(&self, _: PersonId) -> MindFacts {
        MindFacts {
            personality: Personality::industrious(),
            relationships: BTreeMap::new(),
        }
    }
    fn physical_rules(&self) -> Option<PhysicalFacts> {
        Some(PhysicalFacts {
            meal_size: Quantity(1_000),
        })
    }
    fn in_combat(&self, _: PersonId) -> bool {
        false
    }
    fn past_life_controls_actor(&self, _: PersonId) -> bool {
        false
    }
    fn idle_or_resting(&self, actor: PersonId) -> bool {
        !self.activity.contains_key(&actor)
    }
    fn talk_activity(&self, actor: PersonId) -> Option<(EnforcementCaseId, PersonId, SimTime)> {
        self.activity.get(&actor).copied()
    }
    fn completion_scheduled(&self, actor: PersonId, at: SimTime) -> bool {
        self.activity.get(&actor).is_some_and(|row| row.2 == at)
    }
    fn agreements_available(&self) -> bool {
        true
    }
    fn missions_available(&self) -> bool {
        true
    }
    fn institutions_available(&self) -> bool {
        true
    }
    fn combat_available(&self) -> bool {
        true
    }
    fn property_available(&self) -> bool {
        true
    }
    fn events(&self) -> Vec<StrikeRecord> {
        vec![StrikeRecord {
            sequence: 1,
            at: SimTime(0),
            event: Strike::Struck {
                encounter: EncounterId(11),
                actor: PersonId(3),
                target: PersonId(1),
                damage: 1,
            },
        }]
    }
    fn encounter_area(&self, _: EncounterId) -> Option<AreaId> {
        Some(AreaId(3))
    }
    fn incidents(&self, _: PersonId) -> Vec<game_property::IncidentView> {
        Vec::new()
    }
    fn institution_view(&self, actor: PersonId) -> Vec<InstitutionView> {
        crate::institutions::Runtime::default().institution_views(&self.store, actor)
    }
    fn agreement_view(&self, _: PersonId) -> Vec<AgreementView> {
        Vec::new()
    }
    fn social_case_view(&self, _: PersonId) -> Vec<SocialCaseView> {
        Vec::new()
    }
    fn mission_view(&self, _: PersonId) -> Vec<MissionView> {
        Vec::new()
    }
    fn preflight(&self, revisions: u64) -> Result<(), Error> {
        self.revision
            .checked_add(revisions)
            .ok_or(Error::Overflow)
            .map(|_| ())
    }
    fn bump_revision(&mut self, revisions: u64) -> Result<(), Error> {
        self.revision = self
            .revision
            .checked_add(revisions)
            .ok_or(Error::Overflow)?;
        Ok(())
    }
    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error> {
        self.next_event
            .checked_add(count as u64)
            .ok_or(Error::Overflow)
            .map(|_| ())
    }
    fn agreement_command(&mut self, _: PersonId, _: &AgreementAction) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn institution_command(&mut self, _: PersonId, _: &InstitutionAction) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn mission_command(&mut self, _: PersonId, _: &MissionAction) -> Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn preflight_mission_target_report(
        &self,
        _: &MissionProposal,
        _: PersonId,
        _: PersonId,
        _: AreaId,
        _: SimTime,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn seed_mission_target_report(
        &mut self,
        _: MissionId,
        _: PersonId,
        _: PersonId,
        _: AreaId,
        _: SimTime,
    ) -> Result<(), Error> {
        Ok(())
    }
    fn publish(
        &mut self,
        batch: Batch,
        guard: ReadGuard,
        activities: Vec<ActivityEffect>,
        events: Vec<Event>,
        allocations: u64,
        interruptions: Vec<PersonId>,
    ) -> Result<(), Error> {
        guard.validate(&self.store, &batch)?;
        let next = self.next.checked_add(allocations).ok_or(Error::Overflow)?;
        self.store.apply_batch(batch)?;
        self.next = next;
        for actor in interruptions {
            self.activity.remove(&actor);
        }
        for activity in activities {
            match activity {
                ActivityEffect::Talk {
                    actor,
                    case,
                    recipient,
                    completes,
                } => {
                    self.activity.insert(actor, (case, recipient, completes));
                }
                ActivityEffect::Idle { actor } => {
                    self.activity.remove(&actor);
                }
            }
        }
        self.next_event += events.len() as u64;
        self.events.extend(events);
        Ok(())
    }
}
fn file(port: Fixture) -> System<Fixture> {
    let action = EnforcementAction::File {
        institution: InstitutionId(10),
        office: 0,
        evidence: EnforcementEvidenceRef::Assault(EventId(1)),
    };
    let mut system = System::for_action(port, PersonId(1), &action);
    system.enforcement_command(PersonId(1), &action).unwrap();
    system.publish().unwrap();
    system
}
#[test]
fn production_filing_and_received_complaint_use_canonical_owner_rows() {
    let system = file(Fixture::new());
    let mut port = system.port;
    assert_eq!(
        case(&port.store, EnforcementCaseId(20)).unwrap().status,
        EnforcementStatus::Filing
    );
    assert_eq!(
        talk(&port.store, PersonId(1)).unwrap().recipient,
        PersonId(2)
    );
    assert!(matches!(
        port.events.last(),
        Some(Event::EnforcementFiling {
            case: EnforcementCaseId(20),
            ..
        })
    ));
    port.now = SimTime(MINUTE.0);
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    system.publish().unwrap();
    assert_eq!(
        case(&system.port.store, EnforcementCaseId(20))
            .unwrap()
            .status,
        EnforcementStatus::Pending
    );
    assert!(talk(&system.port.store, PersonId(1)).is_none());
    assert_eq!(
        memory(&system.port.store, PersonId(2)).unwrap().known[&EnforcementCaseId(20)].status,
        EnforcementStatus::Pending
    );
    assert!(matches!(
        system.port.events.last(),
        Some(Event::EnforcementFiled {
            case: EnforcementCaseId(20),
            ..
        })
    ));
    System::for_validation(system.port)
        .validate_enforcement()
        .unwrap();
}
#[test]
fn lost_contact_withdraws_only_the_unreceived_complaint() {
    let system = file(Fixture::new());
    let mut port = system.port;
    port.now = SimTime(MINUTE.0);
    port.people.get_mut(&PersonId(2)).unwrap().vitals.alive = false;
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    system.publish().unwrap();
    assert_eq!(
        case(&system.port.store, EnforcementCaseId(20))
            .unwrap()
            .status,
        EnforcementStatus::Withdrawn
    );
    assert!(talk(&system.port.store, PersonId(1)).is_none());
    assert!(
        !system
            .port
            .events
            .iter()
            .any(|event| matches!(event, Event::EnforcementFiled { .. }))
    );
}
#[test]
fn stale_completion_and_interruption_reject_before_rows_or_events_are_published() {
    let system = file(Fixture::new());
    let mut port = system.port;
    port.now = SimTime(MINUTE.0);
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    case_mut(&mut system.port.store, EnforcementCaseId(20))
        .unwrap()
        .explanation = "changed by another accepted owner operation".into();
    let before = system.port.store.snapshot().unwrap();
    let events = system.port.events.len();
    assert_eq!(system.publish(), Err(Error::Unavailable));
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    assert_eq!(system.port.events.len(), events);
    let prepared = system
        .port
        .runtime
        .as_ref()
        .unwrap()
        .prepare_interruptions(&system.port.store, &[PersonId(1)], system.port.now)
        .unwrap();
    let mut batch = Batch::new();
    let guard = prepared.stage(&system.port.store, &mut batch).unwrap();
    batch.insert(
        StableId(20),
        case(&system.port.store, EnforcementCaseId(20))
            .unwrap()
            .clone(),
    );
    assert_eq!(
        guard.validate(&system.port.store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(system.port.store.snapshot().unwrap(), before);
}
#[test]
fn focused_command_projection_does_not_copy_unrelated_population_memory() {
    let mut port = Fixture::new();
    for id in 100..10_100 {
        memory_mut(&mut port.store, PersonId(id)).unwrap();
    }
    let system = System::for_actor(port, PersonId(1));
    assert_eq!(system.opening.memory_ids(), BTreeSet::from([PersonId(1)]));
    assert_eq!(system.state.policies.len(), 1);
    assert!(system.state.cases.is_empty());
}
#[test]
fn rejected_filing_preserves_allocations_events_and_revision() {
    let mut port = Fixture::new();
    policies_mut(&mut port.store, InstitutionId(10))
        .unwrap()
        .offices
        .get_mut(&0)
        .unwrap()
        .policy
        .topics
        .clear();
    let action = EnforcementAction::File {
        institution: InstitutionId(10),
        office: 0,
        evidence: EnforcementEvidenceRef::Assault(EventId(1)),
    };
    let mut system = System::for_action(port, PersonId(1), &action);
    let before = system.port.store.snapshot().unwrap();
    assert_eq!(
        system.enforcement_command(PersonId(1), &action),
        Err(Error::Unavailable)
    );
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    assert_eq!(system.port.next, 20);
    assert_eq!(system.port.revision, 0);
    assert!(system.port.events.is_empty());
}

#[test]
fn completion_deadline_and_restored_continuation_match_the_uninterrupted_owner_path() {
    let system = file(Fixture::new());
    let mut port = system.port;
    let before = port.store.snapshot().unwrap();
    let events = port.events.len();
    let mut early = System::for_actor(port, PersonId(1));
    early.complete_enforcement_talk(PersonId(1));
    early.publish().unwrap();
    assert_eq!(early.port.store.snapshot().unwrap(), before);
    assert_eq!(early.port.events.len(), events);
    port = early.port;
    let saved = serde_json::to_vec(&port.store.snapshot().unwrap()).unwrap();
    let mut restored = Fixture::new();
    restored.store = Store::restore(
        port.store.schema().clone(),
        serde_json::from_slice(&saved).unwrap(),
    )
    .unwrap();
    restored.runtime = port.runtime.clone();
    restored.activity = port.activity.clone();
    restored.next = port.next;
    restored.next_event = port.next_event;
    restored.revision = port.revision;
    port.now = SimTime(MINUTE.0);
    restored.now = port.now;
    let mut uninterrupted = System::for_actor(port, PersonId(1));
    let mut resumed = System::for_actor(restored, PersonId(1));
    uninterrupted.complete_enforcement_talk(PersonId(1));
    resumed.complete_enforcement_talk(PersonId(1));
    uninterrupted.publish().unwrap();
    resumed.publish().unwrap();
    assert_eq!(
        uninterrupted.port.store.snapshot().unwrap(),
        resumed.port.store.snapshot().unwrap()
    );
    assert_eq!(uninterrupted.port.runtime, resumed.port.runtime);
    assert_eq!(uninterrupted.port.next_event, resumed.port.next_event);
}
#[test]
fn judgment_scoring_uses_personal_evidence_and_relationships_without_assembly() {
    let system = file(Fixture::new());
    let port = system.port;
    let system = System::for_actor(port, PersonId(1));
    let institution = crate::institution(&system.port.store, InstitutionId(10)).unwrap();
    let change = |remedy| InstitutionChange::Adjudicate {
        case: EnforcementCaseId(20),
        remedy,
    };
    assert_eq!(
        system.enforcement_vote_score(
            PersonId(1),
            institution,
            &change(EnforcementRemedy::Dismiss)
        ),
        -800
    );
    assert_eq!(
        system.enforcement_vote_score(
            PersonId(1),
            institution,
            &change(EnforcementRemedy::Settlement { terms: Vec::new() })
        ),
        1_200
    );
    assert_eq!(
        system.enforcement_vote_score(
            PersonId(1),
            institution,
            &change(EnforcementRemedy::Confine {
                until: SimTime(90_000)
            })
        ),
        400
    );
}
#[test]
fn stale_institution_and_late_schema_rejection_preserve_allocation_counters() {
    let port = Fixture::new();
    let action = EnforcementAction::File {
        institution: InstitutionId(10),
        office: 0,
        evidence: EnforcementEvidenceRef::Assault(EventId(1)),
    };
    let mut system = System::for_action(port, PersonId(1), &action);
    system.enforcement_command(PersonId(1), &action).unwrap();
    crate::institution_mut(&mut system.port.store, InstitutionId(10))
        .unwrap()
        .charter
        .purpose = "newly accepted mandate".into();
    let before = system.port.store.snapshot().unwrap();
    assert_eq!(system.publish(), Err(Error::Unavailable));
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    assert_eq!(system.port.next, 20);
    assert_eq!(system.port.revision, 0);
    assert!(system.port.events.is_empty());
    let mut port = Fixture::new();
    let mut schema = game_ecs::Schema::new();
    crate::storage::register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_agreements::register_components(&mut schema).unwrap();
    schema
        .register::<Case>("society/enforcement-case", |row| {
            if row.status == EnforcementStatus::Filing {
                return Err(Error::Unavailable);
            }
            Ok(())
        })
        .unwrap();
    schema
        .register::<Talk>("society/enforcement-talk", |_| Ok(()))
        .unwrap();
    schema
        .register::<Policies>("society/enforcement-policies", |_| Ok(()))
        .unwrap();
    schema
        .register::<Memory>("society/enforcement-memory", |_| Ok(()))
        .unwrap();
    port.store = Store::restore(schema, port.store.snapshot().unwrap()).unwrap();
    let mut system = System::for_action(port, PersonId(1), &action);
    system.enforcement_command(PersonId(1), &action).unwrap();
    let before = system.port.store.snapshot().unwrap();
    assert_eq!(system.publish(), Err(Error::Unavailable));
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    assert_eq!(system.port.next, 20);
    assert_eq!(system.port.revision, 0);
    assert!(system.port.events.is_empty());
}

#[test]
fn institutional_adoption_composes_with_judgment_through_the_owner_receipt() {
    let system = file(Fixture::new());
    let mut port = system.port;
    port.now = SimTime(MINUTE.0);
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    system.publish().unwrap();
    let mut port = system.port;
    let proposal = RuleProposal {
        id: RuleProposalId(30),
        institution: InstitutionId(10),
        proposer: PersonId(2),
        change: InstitutionChange::Adjudicate {
            case: EnforcementCaseId(20),
            remedy: EnforcementRemedy::Dismiss,
        },
        votes: BTreeMap::from([(PersonId(1), true), (PersonId(2), true)]),
        base_revision: 0,
        status: RuleProposalStatus::Proposed,
    };
    crate::insert_proposal(&mut port.store, proposal.id, proposal.clone()).unwrap();
    let mut projected = proposal.clone();
    projected.status = RuleProposalStatus::Adopted;
    let prepared =
        prepare_adoption(&port.store, port.now, &projected, Some(JudgmentId(31))).unwrap();
    let before = port.store.snapshot().unwrap();
    let mut batch = Batch::new();
    batch.insert(StableId(30), projected.clone());
    let publication = prepared.stage(&port.store, &mut batch).unwrap();
    publication.validate(&port.store, &batch).unwrap();
    let people = port.people();
    let assaults = Vec::new();
    let observation = stage_adoption_observation(
        &port.store,
        &mut batch,
        &[&publication],
        AdoptionObservationFacts {
            now: port.now,
            people: &people,
            assaults: &assaults,
            last_event: port.runtime.as_ref().map(|world| world.last_event),
        },
        ReadSet::new(),
    )
    .unwrap();
    observation
        .validate(port.runtime.as_ref(), &port.store, &batch)
        .unwrap();
    publication.validate(&port.store, &batch).unwrap();
    assert_eq!(port.store.snapshot().unwrap(), before);
    port.store.apply_batch(batch).unwrap();
    assert_eq!(
        case(&port.store, EnforcementCaseId(20)).unwrap().status,
        EnforcementStatus::Dismissed
    );
    assert_eq!(
        crate::proposal(&port.store, RuleProposalId(30))
            .unwrap()
            .status,
        RuleProposalStatus::Adopted
    );
    assert_eq!(
        memory(&port.store, PersonId(1)).unwrap().known[&EnforcementCaseId(20)].status,
        EnforcementStatus::Dismissed
    );
    port.runtime = Some(observation.finish(port.next_event - 1));
    assert!(matches!(
        publication.finish(),
        Some(Event::EnforcementDecided {
            judgment: JudgmentId(31),
            ..
        })
    ));
    // The receiver refuses a batch that publishes the judgment without its adoption.
    let system = file(Fixture::new());
    let mut port = system.port;
    port.now = SimTime(MINUTE.0);
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    system.publish().unwrap();
    let port = system.port;
    let prepared =
        prepare_adoption(&port.store, port.now, &projected, Some(JudgmentId(31))).unwrap();
    let mut batch = Batch::new();
    let publication = prepared.stage(&port.store, &mut batch).unwrap();
    assert_eq!(
        publication.validate(&port.store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(
        case(&port.store, EnforcementCaseId(20)).unwrap().status,
        EnforcementStatus::Pending
    );
}

fn decided(mut port: Fixture) -> Fixture {
    port.now = SimTime(MINUTE.0);
    let mut system = System::for_actor(port, PersonId(1));
    system.complete_enforcement_talk(PersonId(1));
    system.publish().unwrap();
    let mut port = system.port;
    let proposal = RuleProposal {
        id: RuleProposalId(port.next),
        institution: InstitutionId(10),
        proposer: PersonId(2),
        change: InstitutionChange::Adjudicate {
            case: EnforcementCaseId(20),
            remedy: EnforcementRemedy::Confine {
                until: SimTime(port.now.0 + 90_000),
            },
        },
        votes: BTreeMap::from([(PersonId(1), true), (PersonId(2), true)]),
        base_revision: 0,
        status: RuleProposalStatus::Adopted,
    };
    let prepared = prepare_adoption(
        &port.store,
        port.now,
        &proposal,
        Some(JudgmentId(port.next + 1)),
    )
    .unwrap();
    let mut batch = Batch::new();
    batch.insert(StableId(proposal.id.0), proposal);
    let receipt = prepared.stage(&port.store, &mut batch).unwrap();
    let people = port.people();
    let observed = stage_adoption_observation(
        &port.store,
        &mut batch,
        &[&receipt],
        AdoptionObservationFacts {
            now: port.now,
            people: &people,
            assaults: &[],
            last_event: port.runtime.as_ref().map(|world| world.last_event),
        },
        ReadSet::new(),
    )
    .unwrap();
    receipt.validate(&port.store, &batch).unwrap();
    observed
        .validate(port.runtime.as_ref(), &port.store, &batch)
        .unwrap();
    port.store.apply_batch(batch).unwrap();
    port.next += 2;
    port.events.push(receipt.finish().unwrap());
    port.next_event += 1;
    port.runtime = Some(observed.finish(port.next_event - 1));
    port
}
#[test]
fn production_report_and_refusal_paths_preserve_personal_information() {
    let mut port = decided(file(Fixture::new()).port);
    port.people.get_mut(&PersonId(2)).unwrap().location = Location::AtArea(AreaId(4));
    let action = EnforcementAction::Respond {
        case: EnforcementCaseId(20),
        comply: false,
    };
    let mut system = System::for_action(port, PersonId(3), &action);
    system.enforcement_command(PersonId(3), &action).unwrap();
    system.publish().unwrap();
    assert_eq!(
        case(&system.port.store, EnforcementCaseId(20))
            .unwrap()
            .status,
        EnforcementStatus::Refused
    );
    let port = system.port;
    let action = EnforcementAction::Report {
        case: EnforcementCaseId(20),
        recipient: PersonId(1),
    };
    let mut system = System::for_action(port, PersonId(3), &action);
    system.enforcement_command(PersonId(3), &action).unwrap();
    system.publish().unwrap();
    let mut port = system.port;
    port.now = SimTime(port.now.0 + MINUTE.0);
    let mut system = System::for_actor(port, PersonId(3));
    system.complete_enforcement_talk(PersonId(3));
    system.publish().unwrap();
    let received = &memory(&system.port.store, PersonId(1)).unwrap().known[&EnforcementCaseId(20)];
    assert_eq!(received.status, EnforcementStatus::Refused);
    assert_eq!(received.source, KnowledgeSource::Reported(PersonId(3)));
    // An absent officer receives no automatic report about the refusal.
    assert_eq!(
        memory(&system.port.store, PersonId(2)).unwrap().known[&EnforcementCaseId(20)].status,
        EnforcementStatus::Decided
    );
}
#[test]
fn production_dispatch_rejects_wrong_leadership_and_unsupported_spending_before_external_operation()
{
    let port = decided(file(Fixture::new()).port);
    let until = SimTime(port.now.0 + 90_000);
    let proposal = MissionProposal {
        leader: PersonId(1),
        recruits: BTreeSet::new(),
        objective: MissionObjective::Confine {
            person: PersonId(3),
            until,
        },
        home: AreaId(3),
        supplies: Vec::new(),
        compensation: BTreeMap::new(),
        deadline: SimTime(until.0 + MINUTE.0),
    };
    let action = EnforcementAction::Dispatch {
        case: EnforcementCaseId(20),
        proposal: proposal.clone(),
    };
    let mut system = System::for_action(port, PersonId(2), &action);
    let before = system.port.store.snapshot().unwrap();
    let counters = (
        system.port.next,
        system.port.next_event,
        system.port.revision,
    );
    assert_eq!(
        system.enforcement_command(PersonId(2), &action),
        Err(Error::Unavailable)
    );
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    let mut proposal = proposal;
    proposal.leader = PersonId(2);
    proposal.supplies = vec![MissionSupply {
        carrier: PersonId(2),
        source: InventoryId(999),
        quantity: Quantity(1),
    }];
    let action = EnforcementAction::Dispatch {
        case: EnforcementCaseId(20),
        proposal,
    };
    assert_eq!(
        system.enforcement_command(PersonId(2), &action),
        Err(Error::Unavailable)
    );
    assert_eq!(system.port.store.snapshot().unwrap(), before);
    assert_eq!(
        (
            system.port.next,
            system.port.next_event,
            system.port.revision
        ),
        counters
    );
}

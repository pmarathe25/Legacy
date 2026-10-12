use crate::*;
use game_ecs::Schema;
const A: PersonId = PersonId(10);
const B: PersonId = PersonId(11);
const SOURCE: InventoryId = InventoryId(20);
const GOAL: InventoryId = InventoryId(21);
const CARRY: InventoryId = InventoryId(22);
fn inventory(
    id: InventoryId,
    place: game_cargo::InventoryLocation,
    q: u64,
) -> game_cargo::Inventory {
    game_cargo::Inventory {
        id,
        location: place,
        quantity: Quantity(q),
        capacity: Quantity(100),
        access: BTreeSet::from([A, B]),
        last_granted: None,
        good: Good::Food,
        spoils_at: None,
    }
}
fn world() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_property::register_components(&mut schema).unwrap();
    game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    for person in [A, B] {
        store.ensure(StableId(person.0)).unwrap();
    }
    for row in [
        inventory(SOURCE, game_cargo::InventoryLocation::Area(AreaId(1)), 20),
        inventory(GOAL, game_cargo::InventoryLocation::Area(AreaId(1)), 0),
        inventory(CARRY, game_cargo::InventoryLocation::Carried(A), 0),
    ] {
        game_cargo::insert_inventory(&mut store, row).unwrap();
    }
    game_property::declare_custody(&mut store, GOAL, B).unwrap();
    store
}
fn delivery() -> AgreementTerm {
    AgreementTerm::Delivery {
        debtor: A,
        creditor: B,
        source: SOURCE,
        destination: GOAL,
        quantity: Quantity(10),
        deadline: SimTime(100_000),
    }
}
fn active(store: &mut Store, terms: Vec<AgreementTerm>) {
    let assets = terms_context(store, SimTime(0), &terms, &Facts::default());
    let facts = TransitionFacts {
        actor_available: true,
        present: BTreeSet::from([A, B]),
        assets,
    };
    Ledger::new(store)
        .prepare_transition(
            A,
            &AgreementAction::Propose {
                parties: BTreeSet::from([A, B]),
                terms,
                procedure: None,
            },
            &facts,
            &TransitionIds {
                agreement: Some(AgreementId(30)),
                ..Default::default()
            },
        )
        .unwrap()
        .commit(store)
        .unwrap();
    let count = Ledger::new(store)
        .allocation_needs(&AgreementAction::Accept {
            agreement: AgreementId(30),
        })
        .unwrap()
        .obligations;
    Ledger::new(store)
        .prepare_transition(
            B,
            &AgreementAction::Accept {
                agreement: AgreementId(30),
            },
            &facts,
            &TransitionIds {
                obligations: (31..31 + count as u64).map(ObligationId).collect(),
                ..Default::default()
            },
        )
        .unwrap()
        .commit(store)
        .unwrap();
}
#[derive(Default)]
struct Facts {
    remote: bool,
    routes: game_logistics::KnownRoutes,
}
impl Presence for Facts {
    fn contact(&self, actor: PersonId) -> Option<ContactFacts> {
        Some(ContactFacts {
            alive: true,
            health: 1,
            independent: true,
            in_combat: false,
            area: Some(AreaId(if self.remote && actor == B { 2 } else { 1 })),
        })
    }
}
impl OutputSources for Facts {
    fn output_source(&self, _: SiteId, _: Good) -> Option<InventoryId> {
        Some(SOURCE)
    }
}
impl TermAssets for Facts {
    fn site_exists(&self, _: SiteId) -> bool {
        true
    }
    fn site_custodian(&self, _: SiteId) -> Option<PersonId> {
        Some(A)
    }
    fn project_exists(&self, _: ProjectId) -> bool {
        true
    }
}
fn transfer(
    store: &mut Store,
    property: &mut game_property::Runtime,
    sequence: u64,
    source: InventoryId,
    destination: InventoryId,
    q: u64,
) {
    let intent = game_cargo::TransferIntent {
        actor: A,
        source,
        destination,
        quantity: Quantity(q),
    };
    let request = game_cargo::TransferRequest {
        intent,
        facts: game_cargo::ClosingFacts {
            alive: true,
            health: 1,
            source_contact: true,
            source_access: true,
            receiver_contact: true,
            receiver_access: true,
            receiver_accepted: false,
            record_permission: true,
        },
    };
    let result = game_cargo::prepare_transfers(store, None, vec![request], None, |_| true)
        .unwrap()
        .commit(store)
        .unwrap();
    assert!(result[0].1);
    let balances = [source, destination]
        .into_iter()
        .map(|id| (id, game_cargo::inventory(store, id).unwrap().quantity))
        .collect();
    property
        .prepare_receipts(
            store,
            &[game_property::PropertyReceipt {
                event: sequence,
                change: game_property::ReceiptChange::Transfer {
                    source,
                    destination,
                    quantity: Quantity(q),
                },
            }],
            &balances,
            sequence,
        )
        .unwrap()
        .commit(property, store)
        .unwrap();
}
#[test]
fn cargo_property_and_promises_share_one_store_and_qualified_receipts_keep_chronology() {
    let mut store = world();
    active(&mut store, vec![delivery()]);
    let mut property = game_property::Runtime::default();
    let mut runtime = Runtime::default();
    transfer(&mut store, &mut property, 1, SOURCE, CARRY, 7);
    prepare_reconciliation(
        &store,
        &runtime,
        SimTime(1),
        1,
        &[Receipt {
            at: SimTime(1),
            kind: ReceiptKind::Transferred {
                actor: A,
                source: SOURCE,
                destination: CARRY,
                quantity: Quantity(7),
            },
        }],
        &Facts::default(),
    )
    .unwrap()
    .commit(&mut store, &mut runtime)
    .unwrap();
    assert_eq!(
        obligation(&store, ObligationId(31)).unwrap().cargo,
        BTreeMap::from([(CARRY, 7)])
    );
    assert_eq!(obligation(&store, ObligationId(31)).unwrap().performed, 0);
    notice(&mut store, SimTime(1), A, AgreementId(30)).unwrap();
    notice(&mut store, SimTime(1), B, AgreementId(30)).unwrap();
    observe_obligation(
        &mut store,
        SimTime(1),
        ObligationId(31),
        A,
        SimTime(1),
        &Facts::default(),
    )
    .unwrap();
    assert_eq!(
        known_agreements(&store, B).unwrap()[&AgreementId(30)].obligations[0].cargo,
        BTreeMap::new()
    );
    let logistics = game_logistics::Delivery {
        requirement: game_logistics::Requirement {
            good: Good::Food,
            quantity: Quantity(10),
            source: Some(SOURCE),
        },
        destination: game_logistics::Destination::Inventory(GOAL),
        remaining: Quantity(10),
        last_receipt: 0,
    };
    assert_eq!(
        delivery_cargo(&store, A, &logistics),
        BTreeMap::from([(CARRY, Quantity(7))])
    );
    assert_eq!(
        known_delivery_amounts(&store, A, Good::Food, Some(CARRY), Some(GOAL)),
        vec![Quantity(10)]
    );
    assert!(known_delivery_amounts(&store, B, Good::Food, Some(SOURCE), Some(GOAL)).is_empty());
    let frozen = prepare_report(
        &store,
        SimTime(1),
        A,
        B,
        AgreementId(30),
        ReportFacts {
            sender_under_care: false,
            recipient_idle_or_rest: true,
        },
        &Facts::default(),
    )
    .unwrap();
    game_cargo::prepare_consumptions(&store, &[(CARRY, Quantity(3))])
        .unwrap()
        .commit(&mut store)
        .unwrap();
    property
        .prepare_receipts(
            &store,
            &[game_property::PropertyReceipt {
                event: 2,
                change: game_property::ReceiptChange::Consume {
                    inventory: CARRY,
                    quantity: Quantity(3),
                },
            }],
            &BTreeMap::from([(CARRY, Quantity(4))]),
            2,
        )
        .unwrap()
        .commit(&mut property, &mut store)
        .unwrap();
    transfer(&mut store, &mut property, 3, CARRY, GOAL, 4);
    let plan = prepare_reconciliation(
        &store,
        &runtime,
        SimTime(3),
        3,
        &[
            Receipt {
                at: SimTime(2),
                kind: ReceiptKind::Lost {
                    source: CARRY,
                    quantity: Quantity(3),
                },
            },
            Receipt {
                at: SimTime(3),
                kind: ReceiptKind::Transferred {
                    actor: A,
                    source: CARRY,
                    destination: GOAL,
                    quantity: Quantity(4),
                },
            },
        ],
        &Facts::default(),
    )
    .unwrap();
    assert_eq!(
        plan.events,
        vec![ReconciliationEvent::Performed {
            obligation: ObligationId(31),
            actor: A,
            quantity: Quantity(4)
        }]
    );
    plan.commit(&mut store, &mut runtime).unwrap();
    assert_eq!(obligation(&store, ObligationId(31)).unwrap().performed, 4);
    assert!(
        obligation(&store, ObligationId(31))
            .unwrap()
            .cargo
            .is_empty()
    );
    assert_eq!(
        game_property::portions(&store, GOAL).unwrap(),
        &BTreeMap::from([(ClaimId(0), 4)])
    );
    observe_obligation(
        &mut store,
        SimTime(3),
        ObligationId(31),
        A,
        SimTime(3),
        &Facts {
            remote: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        known_delivery_amounts(&store, A, Good::Food, Some(SOURCE), Some(GOAL)),
        vec![Quantity(6)]
    );
    assert!(delivery_cargo(&store, A, &logistics).is_empty());
    assert_eq!(
        known_agreements(&store, B).unwrap()[&AgreementId(30)].obligations[0].performed,
        0
    );
    insert_report(&mut store, A, frozen).unwrap();
    let done = complete_report(&mut store, SimTime(60_001), A, true, &Facts::default()).unwrap();
    assert!(done.reported);
    let learned = &known_agreements(&store, B).unwrap()[&AgreementId(30)];
    assert_eq!(learned.obligations[0].performed, 0);
    assert_eq!(learned.obligations[0].observed_at, SimTime(1));
    assert_eq!(learned.obligations[0].learned_at, SimTime(60_001));
    assert_eq!(
        learned.obligations[0].source,
        Some(KnowledgeSource::Reported(A))
    );
    let plan = prepare_reconciliation(
        &store,
        &runtime,
        SimTime(100_000),
        4,
        &[],
        &Facts::default(),
    )
    .unwrap();
    assert_eq!(
        plan.events,
        vec![ReconciliationEvent::Defaulted {
            obligation: ObligationId(31)
        }]
    );
    plan.commit(&mut store, &mut runtime).unwrap();
    assert_eq!(
        agreement(&store, AgreementId(30)).unwrap().status,
        AgreementStatus::Breached
    );
    validate_runtime(
        &store,
        &runtime,
        ValidationContext {
            people: &BTreeSet::from([A, B]),
            now: SimTime(60_002),
            next_id: 40,
            next_event: 5,
        },
        &Facts::default(),
    )
    .unwrap_err();
    let snapshot = store.snapshot().unwrap();
    let restored = Store::restore(store.schema().clone(), snapshot).unwrap();
    assert_eq!(store, restored);
}
#[test]
fn native_allocations_reject_foreign_entities_and_late_schema_rejection_is_atomic() {
    let mut store = world();
    let before = store.clone();
    let facts = TransitionFacts {
        actor_available: true,
        present: BTreeSet::from([A, B]),
        assets: terms_context(&store, SimTime(0), &[delivery()], &Facts::default()),
    };
    assert_eq!(
        Ledger::new(&store)
            .prepare_transition(
                A,
                &AgreementAction::Propose {
                    parties: BTreeSet::from([A, B]),
                    terms: vec![delivery()],
                    procedure: None
                },
                &facts,
                &TransitionIds {
                    agreement: Some(AgreementId(SOURCE.0)),
                    ..Default::default()
                }
            )
            .unwrap_err(),
        Error::Unavailable
    );
    assert_eq!(store, before);
    let mut schema = Schema::new();
    schema
        .register::<Agreement>("agreements/agreement", Agreement::validate)
        .unwrap();
    schema
        .register::<Obligation>("agreements/obligation", |_| Err(Error::Unavailable))
        .unwrap();
    schema
        .register::<Amendment>("agreements/amendment", Amendment::validate)
        .unwrap();
    schema
        .register::<PendingWork>("agreements/pending_work", |_| Ok(()))
        .unwrap();
    let mut rejected = Store::with_schema(schema);
    Ledger::new(&rejected)
        .prepare_transition(
            A,
            &AgreementAction::Propose {
                parties: BTreeSet::from([A, B]),
                terms: vec![delivery()],
                procedure: None,
            },
            &facts,
            &TransitionIds {
                agreement: Some(AgreementId(30)),
                ..Default::default()
            },
        )
        .unwrap()
        .commit(&mut rejected)
        .unwrap();
    let plan = Ledger::new(&rejected)
        .prepare_transition(
            B,
            &AgreementAction::Accept {
                agreement: AgreementId(30),
            },
            &facts,
            &TransitionIds {
                obligations: vec![ObligationId(31)],
                ..Default::default()
            },
        )
        .unwrap();
    let before = rejected.clone();
    assert_eq!(plan.commit(&mut rejected), Err(Error::Unavailable));
    assert_eq!(rejected, before);
    assert!(!rejected.contains(StableId(31)));
    active(&mut store, vec![delivery()]);
    let plan = Ledger::new(&store)
        .prepare_labor(SimTime(0), SimTime(20), &[])
        .unwrap();
    plan.commit(&mut store).unwrap();
}
#[test]
fn stale_reconciliation_and_intrinsic_id_spoofs_preserve_native_rows() {
    let mut store = world();
    active(&mut store, vec![delivery()]);
    let mut runtime = Runtime::default();
    let plan = prepare_reconciliation(
        &store,
        &runtime,
        SimTime(100_000),
        1,
        &[],
        &Facts::default(),
    )
    .unwrap();
    obligation_mut(&mut store, ObligationId(31))
        .unwrap()
        .performed = 1;
    let before = store.clone();
    assert_eq!(
        plan.commit(&mut store, &mut runtime),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    assert_eq!(runtime.last_event, 0);
    let row = agreement(&store, AgreementId(30)).unwrap().clone();
    assert!(insert_agreement(&mut store, AgreementId(39), row).is_err());
    assert_eq!(store, before);
    let mut row = agreement(&store, AgreementId(30)).unwrap().clone();
    row.id = AgreementId(35);
    store.insert_or_spawn(StableId(36), row).unwrap();
    assert!(
        validate_runtime(
            &store,
            &runtime,
            ValidationContext {
                people: &BTreeSet::from([A, B]),
                now: SimTime(1),
                next_id: 40,
                next_event: 2
            },
            &Facts::default()
        )
        .is_err()
    );
}
#[test]
fn report_interruptions_and_public_choices_use_native_personal_rows() {
    let mut store = world();
    active(&mut store, vec![delivery()]);
    notice(&mut store, SimTime(0), A, AgreementId(30)).unwrap();
    let pending = prepare_report(
        &store,
        SimTime(1),
        A,
        B,
        AgreementId(30),
        ReportFacts {
            sender_under_care: false,
            recipient_idle_or_rest: true,
        },
        &Facts::default(),
    )
    .unwrap();
    insert_report(&mut store, A, pending).unwrap();
    let cancellation = prepare_cancel_reports(&store, &[A]);
    let before = store.clone();
    let mut foreign = Batch::new();
    foreign.insert(StableId(A.0), report(&store, A).unwrap().clone());
    assert_eq!(
        cancellation.clone().stage_into(&store, &mut foreign),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    let mut batch = Batch::new();
    cancellation.stage_into(&store, &mut batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(report(&store, A).is_none());
    assert!(known_agreements(&store, A).is_some());
    assert!(
        prepare_report(
            &store,
            SimTime(1),
            A,
            B,
            AgreementId(30),
            ReportFacts {
                sender_under_care: false,
                recipient_idle_or_rest: true
            },
            &Facts {
                remote: true,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(available_actions(&store, SimTime(1), A, &Facts::default()).is_empty());
}
impl OfferKnowledge for Facts {
    fn stock(&self, _: InventoryId) -> Option<&StockKnowledge> {
        None
    }
    fn work_site_access(&self, _: SiteId) -> bool {
        true
    }
    fn project_known(&self, _: ProjectId) -> bool {
        true
    }
    fn share_prospect(&self, _: SiteId, _: u64, _: u64) -> ShareProspect {
        ShareProspect::Unknown
    }
    fn speaker(&self, _: PersonId) -> Option<SpeakerFacts> {
        None
    }
}
impl JudgmentFacts for Facts {
    fn actor_facts(&self, actor: PersonId, _: &[AgreementTerm]) -> Result<OfferActorFacts, Error> {
        Ok(OfferActorFacts {
            actor,
            now: SimTime(0),
            hunger: 0,
            empathy: 500,
            risk_tolerance: 500,
            ambition: 500,
            hedonism: 500,
            financing_utility: 0,
        })
    }
    fn knowledge(&self, _: PersonId, _: &[AgreementTerm]) -> Result<impl OfferKnowledge, Error> {
        Ok(self)
    }
}
impl OfferKnowledge for &Facts {
    fn stock(&self, id: InventoryId) -> Option<&StockKnowledge> {
        (*self).stock(id)
    }
    fn work_site_access(&self, id: SiteId) -> bool {
        (*self).work_site_access(id)
    }
    fn project_known(&self, id: ProjectId) -> bool {
        (*self).project_known(id)
    }
    fn share_prospect(&self, id: SiteId, n: u64, d: u64) -> ShareProspect {
        (*self).share_prospect(id, n, d)
    }
    fn speaker(&self, id: PersonId) -> Option<SpeakerFacts> {
        (*self).speaker(id)
    }
}
struct NoAcquisition;
impl game_logistics::AcquisitionSource<u8> for NoAcquisition {
    fn offers(&self, _: Good, _: Quantity, _: Quantity) -> Vec<game_logistics::Candidate<u8>> {
        Vec::new()
    }
}
impl DutyFacts<u8> for Facts {
    fn actor_facts(&self, actor: PersonId) -> Option<DutyActorFacts> {
        Some(DutyActorFacts {
            controlled: actor == B,
            restricted: false,
            in_combat: false,
            idle_or_rest: true,
            hunger: 0,
            meal_relief: 100,
            fatigue: 0,
            fatigue_limit: 100,
            discipline: 500,
            honesty: 500,
        })
    }
    fn known_site(&self, _: PersonId, site: SiteId) -> Option<KnownSiteRight> {
        Some(KnownSiteRight {
            site,
            area: AreaId(1),
            accessible: true,
        })
    }
    fn routes(&self, _: PersonId) -> Option<&game_logistics::KnownRoutes> {
        Some(&self.routes)
    }
    fn cultivated_site(&self, _: SiteId) -> bool {
        false
    }
    fn cultivation(&self, _: PersonId) -> Vec<(SiteId, u8)> {
        Vec::new()
    }
    fn construction_admitted(&self, _: PersonId, _: ProjectId) -> bool {
        false
    }
    fn source_good(&self, _: PersonId, _: InventoryId) -> Option<Good> {
        None
    }
    fn inventory_destination(
        &self,
        _: PersonId,
        _: InventoryId,
    ) -> Option<game_logistics::KnownDestination> {
        None
    }
    fn delivery_facts<'a>(
        &'a self,
        _: PersonId,
        _: &game_logistics::Delivery,
        _: &game_logistics::KnownDestination,
        _: &BTreeMap<InventoryId, Quantity>,
    ) -> game_logistics::DeliveryFacts<'a, u8> {
        game_logistics::DeliveryFacts::absent(true, true, true)
    }
    fn acquisition(&self, _: PersonId) -> impl game_logistics::AcquisitionSource<u8> {
        NoAcquisition
    }
}
#[test]
fn the_public_choice_passes_preserve_consent_and_fall_through_to_first_ready_duty() {
    let mut store = world();
    let facts = Facts::default();
    let labor = AgreementTerm::Labor {
        debtor: A,
        creditor: B,
        site: SiteId(50),
        millis: 100,
        deadline: SimTime(100_000),
    };
    active(&mut store, vec![delivery(), labor]);
    notice(&mut store, SimTime(0), A, AgreementId(30)).unwrap();
    assert_eq!(
        chosen_tasks(&store, &facts),
        BTreeMap::from([(A, DutyAction::Work { site: SiteId(50) })])
    );
    let mut offered = world();
    let terms = vec![delivery()];
    let opening = TransitionFacts {
        actor_available: true,
        present: BTreeSet::from([A, B]),
        assets: terms_context(&offered, SimTime(0), &terms, &facts),
    };
    Ledger::new(&offered)
        .prepare_transition(
            A,
            &AgreementAction::Propose {
                parties: BTreeSet::from([A, B]),
                terms,
                procedure: Some(AmendmentProcedure {
                    eligible_proposers: BTreeSet::from([A]),
                    approvers: BTreeSet::from([B]),
                    threshold: 1,
                    term_approvers: BTreeMap::new(),
                }),
            },
            &opening,
            &TransitionIds {
                agreement: Some(AgreementId(30)),
                ..Default::default()
            },
        )
        .unwrap()
        .commit(&mut offered)
        .unwrap();
    notice(&mut offered, SimTime(0), B, AgreementId(30)).unwrap();
    let answers = proposal_responses(&offered, A, &facts).unwrap();
    assert_eq!(
        answers,
        vec![(
            B,
            AgreementAction::Accept {
                agreement: AgreementId(30)
            }
        )]
    );
    Ledger::new(&offered)
        .prepare_transition(
            B,
            &answers[0].1,
            &opening,
            &TransitionIds {
                obligations: vec![ObligationId(31)],
                ..Default::default()
            },
        )
        .unwrap()
        .commit(&mut offered)
        .unwrap();
    let mut changed = delivery();
    if let AgreementTerm::Delivery { quantity, .. } = &mut changed {
        *quantity = Quantity(5);
    }
    Ledger::new(&offered)
        .prepare_transition(
            A,
            &AgreementAction::Amend {
                agreement: AgreementId(30),
                replacements: BTreeMap::from([(0, changed)]),
            },
            &opening,
            &TransitionIds {
                amendment: Some(AmendmentId(32)),
                ..Default::default()
            },
        )
        .unwrap()
        .commit(&mut offered)
        .unwrap();
    local_notice(
        &mut offered,
        SimTime(0),
        AgreementId(30),
        A,
        Some(AmendmentId(32)),
        &facts,
    )
    .unwrap();
    assert_eq!(
        amendment_votes(&offered, A, &facts).unwrap(),
        vec![(
            B,
            AgreementAction::Vote {
                amendment: AmendmentId(32),
                support: false
            }
        )]
    );
    assert_eq!(agreement(&offered, AgreementId(30)).unwrap().revision, 0);
}
fn guarded_world(validate: fn(&Knowledge) -> Result<(), Error>) -> Store {
    let opening = world();
    let mut schema = Schema::new();
    schema
        .register::<Agreement>("agreements/agreement", Agreement::validate)
        .unwrap();
    schema
        .register::<Obligation>("agreements/obligation", Obligation::validate)
        .unwrap();
    schema
        .register::<Amendment>("agreements/amendment", Amendment::validate)
        .unwrap();
    schema
        .register::<Knowledge>("agreements/knowledge", validate)
        .unwrap();
    schema
        .register::<Requests>("agreements/requests", |_| Ok(()))
        .unwrap();
    schema
        .register::<PendingWork>("agreements/pending_work", |_| Ok(()))
        .unwrap();
    schema
        .register::<PendingReport>("agreements/report", PendingReport::validate)
        .unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    game_property::register_components(&mut schema).unwrap();
    game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
    Store::restore(schema, opening.snapshot().unwrap()).unwrap()
}
#[test]
fn a_late_multiobserver_validator_rejection_preserves_every_personal_row_and_cursor() {
    let mut store = guarded_world(|row| {
        row.validate()?;
        if row
            .agreements
            .values()
            .any(|view| view.source == Some(KnowledgeSource::Observed(B)))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    });
    active(&mut store, vec![delivery()]);
    let mut runtime = Runtime::default();
    let before = store.clone();
    let prepared = prepare_observations(
        &store,
        &runtime,
        SimTime(0),
        1,
        &[ObservationEvent {
            at: SimTime(0),
            kind: ObservationKind::Proposed {
                agreement: AgreementId(30),
                proposer: A,
            },
        }],
        &Facts::default(),
    )
    .unwrap();
    assert_eq!(
        prepared.commit(&mut store, &mut runtime),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    assert_eq!(runtime, Runtime::default());
    assert!(knowledge(&store, A).is_none());
    assert!(knowledge(&store, B).is_none());
}
#[test]
fn a_rejected_report_completion_retains_its_frozen_snapshot_and_recipient_memory() {
    let mut store = guarded_world(|row| {
        row.validate()?;
        if row
            .agreements
            .values()
            .any(|view| matches!(view.source, Some(KnowledgeSource::Reported(_))))
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    });
    active(&mut store, vec![delivery()]);
    notice(&mut store, SimTime(0), A, AgreementId(30)).unwrap();
    notice(&mut store, SimTime(0), B, AgreementId(30)).unwrap();
    let facts = ReportFacts {
        sender_under_care: false,
        recipient_idle_or_rest: true,
    };
    let presence = Facts::default();
    let prepared =
        prepare_report_start(&store, SimTime(1), A, B, AgreementId(30), facts, &presence).unwrap();
    let cancellation = prepare_cancel_reports(&store, &[A]);
    let mut batch = Batch::new();
    let replacement = prepared
        .stage_into_with_replacement(&store, SimTime(1), facts, &presence, &mut batch)
        .unwrap();
    let guard = cancellation
        .stage_into_composed(&store, &mut batch, Some(&replacement))
        .unwrap();
    guard.validate(&batch).unwrap();
    store.apply_batch(batch).unwrap();
    let before = store.clone();
    let completion =
        prepare_report_completion(&store, SimTime(60_001), A, true, &presence).unwrap();
    assert!(completion.completion.reported);
    assert_eq!(completion.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store, before);
    assert!(report(&store, A).is_some());
    assert_eq!(
        known_agreements(&store, B).unwrap()[&AgreementId(30)].source,
        Some(KnowledgeSource::Observed(B))
    );
}
#[test]
fn report_replacement_rejects_changed_source_foreign_batch_and_late_report_edits() {
    let mut store = world();
    active(&mut store, vec![delivery()]);
    notice(&mut store, SimTime(0), A, AgreementId(30)).unwrap();
    let facts = ReportFacts {
        sender_under_care: false,
        recipient_idle_or_rest: true,
    };
    let presence = Facts::default();
    let start =
        prepare_report_start(&store, SimTime(1), A, B, AgreementId(30), facts, &presence).unwrap();
    let before = store.clone();
    let changed = start.clone();
    knowledge_mut(&mut store, A)
        .unwrap()
        .agreements
        .get_mut(&AgreementId(30))
        .unwrap()
        .learned_at = SimTime(1);
    assert!(
        changed
            .stage_into_with_replacement(&store, SimTime(1), facts, &presence, &mut Batch::new())
            .is_err()
    );
    store = before;
    let cancellation = prepare_cancel_reports(&store, &[A]);
    let mut batch = Batch::new();
    let replacement = start
        .stage_into_with_replacement(&store, SimTime(1), facts, &presence, &mut batch)
        .unwrap();
    let mut foreign = Batch::new();
    foreign.insert(
        StableId(A.0),
        prepare_report(&store, SimTime(1), A, B, AgreementId(30), facts, &presence).unwrap(),
    );
    assert!(
        cancellation
            .clone()
            .stage_into_composed(&store, &mut foreign, Some(&replacement))
            .is_err()
    );
    let guard = cancellation
        .stage_into_composed(&store, &mut batch, Some(&replacement))
        .unwrap();
    guard.validate(&batch).unwrap();
    let mut extra =
        prepare_report(&store, SimTime(1), A, B, AgreementId(30), facts, &presence).unwrap();
    extra.sender = B;
    extra.recipient = A;
    batch.insert(StableId(B.0), extra);
    assert_eq!(guard.validate(&batch), Err(Error::Unavailable));
    assert!(report(&store, A).is_none());
}
#[test]
fn construction_fulfillment_reads_the_first_ordered_personal_promise_without_hidden_progress() {
    let mut store = world();
    let project = ProjectId(60);
    let term = AgreementTerm::ConstructionLabor {
        debtor: A,
        creditor: B,
        project,
        millis: 100,
        deadline: SimTime(100_000),
    };
    active(&mut store, vec![delivery(), term.clone()]);
    assert_eq!(known_construction_obligation(&store, A, project), None);
    notice(&mut store, SimTime(0), A, AgreementId(30)).unwrap();
    insert_obligation(
        &mut store,
        ObligationId(41),
        Obligation {
            id: ObligationId(41),
            agreement: AgreementId(40),
            term: term.clone(),
            performed: 0,
            accrued: 0,
            share_remainder: 0,
            share_scale: 0,
            defaulted: false,
            cargo: BTreeMap::new(),
        },
    )
    .unwrap();
    insert_agreement(
        &mut store,
        AgreementId(40),
        Agreement {
            id: AgreementId(40),
            proposer: A,
            parties: BTreeSet::from([A, B]),
            accepted: BTreeSet::from([A, B]),
            terms: vec![term],
            procedure: None,
            obligations: vec![ObligationId(41)],
            status: AgreementStatus::Active,
            proposed_at: SimTime(0),
            revision: 0,
        },
    )
    .unwrap();
    notice(&mut store, SimTime(0), A, AgreementId(40)).unwrap();
    assert_eq!(
        known_construction_obligation(&store, A, project),
        Some(ObligationId(32))
    );
    assert_eq!(known_construction_obligation(&store, B, project), None);
    Ledger::new(&store)
        .prepare_labor(
            SimTime(0),
            SimTime(100),
            &[WorkAssignment {
                actor: A,
                target: WorkTarget::Project(project),
            }],
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
    assert_eq!(obligation(&store, ObligationId(32)).unwrap().performed, 100);
    assert_eq!(
        known_construction_obligation(&store, A, project),
        Some(ObligationId(32))
    );
    observe_obligation(
        &mut store,
        SimTime(100),
        ObligationId(32),
        A,
        SimTime(100),
        &Facts::default(),
    )
    .unwrap();
    assert_eq!(
        known_construction_obligation(&store, A, project),
        Some(ObligationId(41))
    );
}

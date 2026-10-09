use game_api::*;
use game_sim::{
    decisions::{Dream, Personality},
    *,
};
use std::collections::BTreeSet;
fn fixture(workers: usize) -> Simulation {
    Simulation::project_fixture(
        41,
        Execution {
            workers,
            reverse_jobs: workers > 1,
        },
    )
    .unwrap()
}
fn actor(sim: &Simulation, index: usize) -> PersonId {
    sim.canonical_state().people[index].id
}
fn quiet(sim: &mut Simulation) {
    let people = sim
        .canonical_state()
        .people
        .iter()
        .map(|person| person.id)
        .collect::<Vec<_>>();
    for person in people {
        sim.debug_set_personality(person, Personality::restful(), Dream::Comfort)
            .unwrap();
    }
}
fn maker_assets(sim: &Simulation) -> (InventoryId, InventoryId, InventoryId) {
    let maker = actor(sim, 3);
    let custodians = &sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .custodians;
    let owned = |stock: &&game_sim::Inventory| {
        stock.location == InventoryLocation::Area(AreaId(1))
            && custodians.get(&stock.id) == Some(&maker)
    };
    let materials = sim
        .canonical_state()
        .inventories
        .values()
        .filter(owned)
        .find(|stock| stock.good == Good::Timber)
        .unwrap()
        .id;
    let tool = sim
        .canonical_state()
        .inventories
        .values()
        .filter(owned)
        .find(|stock| stock.good == Good::Tool && stock.quantity.0 > 0)
        .unwrap()
        .id;
    let output = sim
        .canonical_state()
        .inventories
        .values()
        .filter(owned)
        .find(|stock| stock.good == Good::Tool && stock.quantity.0 == 0)
        .unwrap()
        .id;
    (materials, tool, output)
}
fn propose(sim: &mut Simulation, finish_by: SimTime, workers: BTreeSet<PersonId>) -> ProjectId {
    let maker = actor(sim, 3);
    let (materials, tool, output) = maker_assets(sim);
    sim.debug_project_action(
        maker,
        ProjectAction::Propose {
            area: AreaId(1),
            materials,
            tool,
            input: materials,
            output,
            beneficiaries: BTreeSet::from([maker]),
            workers,
            finish_by,
        },
    )
    .unwrap();
    sim.project_view(maker).projects[0].id
}
fn supply(sim: &mut Simulation, quantity: u64) {
    let financier = actor(sim, 5);
    let (destination, _, _) = maker_assets(sim);
    let source = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Timber
                && stock.location == InventoryLocation::Area(AreaId(1))
                && stock.id != destination
        })
        .unwrap()
        .id;
    let cargo = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Timber && stock.location == InventoryLocation::Carried(financier)
        })
        .unwrap()
        .id;
    let maker = actor(sim, 3);
    sim.debug_agreement_action(
        financier,
        AgreementAction::Propose {
            parties: BTreeSet::from([maker, financier]),
            terms: vec![AgreementTerm::Delivery {
                debtor: financier,
                creditor: maker,
                source,
                destination,
                quantity: Quantity(quantity),
                deadline: SimTime(sim.canonical_state().time.0 + 300_000),
            }],
            procedure: None,
        },
    )
    .unwrap();
    let agreement = sim.agreement_view(maker).last().unwrap().id;
    sim.debug_agreement_action(maker, AgreementAction::Accept { agreement })
        .unwrap();
    let project = sim.project_view(maker).projects[0].id;
    sim.debug_project_action(maker, ProjectAction::Support { project, agreement })
        .unwrap();
    sim.debug_actor_command(
        financier,
        Command::Load {
            source,
            destination: cargo,
            quantity: Quantity(quantity),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    sim.debug_actor_command(
        financier,
        Command::Load {
            source: cargo,
            destination,
            quantity: Quantity(quantity),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
}
#[test]
fn autonomous_citizen_negotiates_real_financing_builds_and_repays_from_actual_production() {
    let mut sim = fixture(2);
    sim.advance(SimDuration(1_500_000)).unwrap();
    let world = sim.canonical_state().projects.as_ref().unwrap();
    let plan = world
        .projects
        .values()
        .find(|project| project.sponsor == actor(&sim, 3))
        .unwrap();
    assert_eq!(plan.status, ProjectStatus::Completed);
    assert!(plan.materials_consumed);
    assert_eq!(plan.progress_ms, 300_000);
    let workshop = &world.enterprises[&plan.enterprise.unwrap()];
    assert!(workshop.tools_produced >= 3_000);
    assert_eq!(workshop.timber_consumed, 2 * workshop.tools_produced);
    let funding = &sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .agreements[plan.support.iter().next().unwrap()];
    assert_eq!(funding.status, AgreementStatus::Fulfilled);
    let obligations = &sim
        .canonical_state()
        .agreements
        .as_ref()
        .unwrap()
        .obligations;
    assert!(funding.obligations.iter().any(|id| matches!(
        obligations[id].term,
        AgreementTerm::Delivery {
            quantity: Quantity(11_000),
            ..
        }
    ) && obligations[id].performed == 11_000));
    assert!(funding.obligations.iter().any(|id| matches!(
        obligations[id].term,
        AgreementTerm::Delivery {
            quantity: Quantity(3_000),
            ..
        }
    ) && obligations[id].performed == 3_000));
    assert!(sim.canonical_state().events.iter().any(|event| matches!(
        event.kind,
        EventKind::EnterpriseInputUsed {
            quantity: Quantity(2_000),
            ..
        }
    )));
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}
#[test]
fn consumed_construction_material_and_labor_survive_interruption_without_a_premature_site() {
    let mut sim = fixture(1);
    quiet(&mut sim);
    let maker = actor(&sim, 3);
    let plan = propose(&mut sim, SimTime(1_800_000), BTreeSet::from([maker]));
    supply(&mut sim, 5_000);
    let (materials, _, _) = maker_assets(&sim);
    let sites = sim.canonical_state().physical.as_ref().unwrap().sites.len();
    sim.debug_project_action(maker, ProjectAction::Work { project: plan })
        .unwrap();
    assert_eq!(sim.canonical_state().inventories[&materials].quantity.0, 0);
    sim.advance(SimDuration(120_000)).unwrap();
    sim.debug_actor_command(maker, Command::Cancel).unwrap();
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().projects[&plan].progress_ms,
        120_000
    );
    assert_eq!(
        sim.canonical_state().physical.as_ref().unwrap().sites.len(),
        sites
    );
    sim = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    sim.debug_project_action(maker, ProjectAction::Work { project: plan })
        .unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().projects[&plan].status,
        ProjectStatus::Completed
    );
    assert_eq!(
        sim.canonical_state().physical.as_ref().unwrap().sites.len(),
        sites + 1
    );
    assert_eq!(sim.canonical_state().inventories[&materials].quantity.0, 0);
}
#[test]
fn shortages_leave_stocks_untouched_and_fail_at_exact_deadline() {
    let mut sim = fixture(1);
    quiet(&mut sim);
    let maker = actor(&sim, 3);
    let plan = propose(&mut sim, SimTime(180_123), BTreeSet::from([maker]));
    let before = sim.canonical_state().inventories.clone();
    assert_eq!(
        sim.debug_project_action(maker, ProjectAction::Work { project: plan }),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_state().inventories, before);
    sim.advance(SimDuration(180_122)).unwrap();
    assert_ne!(
        sim.canonical_state().projects.as_ref().unwrap().projects[&plan].status,
        ProjectStatus::Failed
    );
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().projects[&plan].status,
        ProjectStatus::Failed
    );
    assert_eq!(sim.canonical_state().inventories, before);
}
#[test]
fn worker_contract_records_exclusive_construction_time_and_actual_in_kind_payment() {
    let mut sim = fixture(1);
    quiet(&mut sim);
    let maker = actor(&sim, 3);
    let mut withholding = Personality::restful();
    withholding.honesty = 0;
    sim.debug_set_personality(maker, withholding, Dream::Comfort)
        .unwrap();
    let helper = actor(&sim, 6);
    let plan = propose(
        &mut sim,
        SimTime(1_800_000),
        BTreeSet::from([maker, helper]),
    );
    supply(&mut sim, 5_000);
    let (materials, tool, _) = maker_assets(&sim);
    let provisions = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Food
                && stock.location == InventoryLocation::Area(AreaId(1))
                && sim
                    .canonical_state()
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&stock.id)
                    == Some(&maker)
        })
        .unwrap()
        .id;
    let destination = sim.canonical_state().people[6].inventory;
    let deadline = SimTime(1_200_000);
    let terms = vec![
        AgreementTerm::ConstructionLabor {
            debtor: helper,
            creditor: maker,
            project: plan,
            millis: 120_000,
            deadline,
        },
        AgreementTerm::Delivery {
            debtor: maker,
            creditor: helper,
            source: provisions,
            destination,
            quantity: Quantity(3_000),
            deadline,
        },
        AgreementTerm::Access {
            grantor: maker,
            grantee: helper,
            inventory: materials,
            allowed: true,
        },
        AgreementTerm::Access {
            grantor: maker,
            grantee: helper,
            inventory: tool,
            allowed: true,
        },
    ];
    sim.debug_agreement_action(
        maker,
        AgreementAction::Propose {
            parties: BTreeSet::from([maker, helper]),
            terms,
            procedure: None,
        },
    )
    .unwrap();
    let agreement = sim.agreement_view(maker).last().unwrap().id;
    sim.debug_agreement_action(helper, AgreementAction::Accept { agreement })
        .unwrap();
    sim.debug_project_action(
        helper,
        ProjectAction::Support {
            project: plan,
            agreement,
        },
    )
    .unwrap();
    sim.debug_project_action(helper, ProjectAction::Work { project: plan })
        .unwrap();
    sim.advance(SimDuration(120_000)).unwrap();
    sim.debug_actor_command(helper, Command::Cancel).unwrap();
    assert_eq!(
        sim.agreement_view(helper)
            .last()
            .unwrap()
            .obligations
            .iter()
            .find(|o| matches!(o.term, AgreementTerm::ConstructionLabor { .. }))
            .unwrap()
            .performed,
        120_000
    );
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().projects[&plan].contributions[&helper],
        120_000
    );
    let before = sim.canonical_state().inventories[&destination].quantity.0;
    let cargo = sim.canonical_state().people[3].inventory;
    sim.debug_actor_command(
        maker,
        Command::Load {
            source: provisions,
            destination: cargo,
            quantity: Quantity(3_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    sim.debug_actor_command(
        maker,
        Command::Load {
            source: cargo,
            destination,
            quantity: Quantity(3_000),
        },
    )
    .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity.0,
        before + 3_000
    );
}
#[test]
fn factories_and_knowledge_are_equivalent_across_workers_partial_advances_and_restore() {
    let mut a = fixture(1);
    let mut b = fixture(4);
    a.advance(SimDuration(1_200_000)).unwrap();
    b.advance(SimDuration(417_333)).unwrap();
    for _ in 0..30 {
        b.observe();
        b.project_view(actor(&b, 3));
    }
    b = Simulation::load(
        &b.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    while b.canonical_state().time.0 < 1_200_000 {
        b.advance(SimDuration(
            (1_200_000 - b.canonical_state().time.0).min(17_333),
        ))
        .unwrap();
    }
    assert_eq!(a.canonical_state(), b.canonical_state());
}
#[test]
fn operating_wages_follow_real_labor_and_delivery_and_nonpayment_remains_a_debt() {
    let mut sim = fixture(1);
    quiet(&mut sim);
    let maker = actor(&sim, 3);
    let worker = actor(&sim, 6);
    let project = propose(
        &mut sim,
        SimTime(1_800_000),
        BTreeSet::from([maker, worker]),
    );
    supply(&mut sim, 7_000);
    sim.debug_project_action(maker, ProjectAction::Work { project })
        .unwrap();
    sim.advance(SimDuration(300_000)).unwrap();
    let enterprise = sim.canonical_state().projects.as_ref().unwrap().projects[&project]
        .enterprise
        .unwrap();
    let site = sim.canonical_state().projects.as_ref().unwrap().enterprises[&enterprise].site;
    let (materials, tool, _) = maker_assets(&sim);
    let provisions = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Food
                && stock.location == InventoryLocation::Area(AreaId(1))
                && sim
                    .canonical_state()
                    .agreements
                    .as_ref()
                    .unwrap()
                    .custodians
                    .get(&stock.id)
                    == Some(&maker)
        })
        .unwrap()
        .id;
    let destination = sim.canonical_state().people[6].inventory;
    let deadline = SimTime(sim.canonical_state().time.0 + 300_000);
    sim.debug_agreement_action(
        maker,
        AgreementAction::Propose {
            parties: BTreeSet::from([maker, worker]),
            terms: vec![
                AgreementTerm::Labor {
                    debtor: worker,
                    creditor: maker,
                    site,
                    millis: 60_000,
                    deadline,
                },
                AgreementTerm::Delivery {
                    debtor: maker,
                    creditor: worker,
                    source: provisions,
                    destination,
                    quantity: Quantity(1_500),
                    deadline,
                },
                AgreementTerm::Access {
                    grantor: maker,
                    grantee: worker,
                    inventory: materials,
                    allowed: true,
                },
                AgreementTerm::Access {
                    grantor: maker,
                    grantee: worker,
                    inventory: tool,
                    allowed: true,
                },
            ],
            procedure: None,
        },
    )
    .unwrap();
    let agreement = sim.agreement_view(maker).last().unwrap().id;
    sim.debug_agreement_action(worker, AgreementAction::Accept { agreement })
        .unwrap();
    sim.debug_project_action(
        maker,
        ProjectAction::Operate {
            enterprise,
            agreement,
        },
    )
    .unwrap();
    sim.debug_actor_command(worker, Command::Work { site })
        .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().enterprises[&enterprise].labor[&worker],
        60_000
    );
    assert_eq!(
        sim.canonical_state().projects.as_ref().unwrap().enterprises[&enterprise].tools_produced,
        1_000
    );
    // Changing the debtor's own willingness does not erase the wage obligation.
    let mut unwilling = Personality::restful();
    unwilling.honesty = 0;
    sim.debug_set_personality(maker, unwilling, Dream::Comfort)
        .unwrap();
    sim.advance(SimDuration(deadline.0 - sim.canonical_state().time.0))
        .unwrap();
    let account = sim
        .project_view(maker)
        .enterprises
        .iter()
        .find(|view| view.id == enterprise)
        .unwrap()
        .accounts
        .clone()
        .unwrap();
    assert_eq!(account.performed_labor_ms, 60_000);
    assert_eq!(
        account
            .performed_payments
            .get(&Good::Food)
            .copied()
            .unwrap_or(0),
        0
    );
    assert!(account.defaults >= 1);
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements[&agreement]
            .status,
        AgreementStatus::Breached
    );
}

#[test]
fn cautious_financier_refuses_startup_risk_without_any_stock_transfer() {
    let mut sim = fixture(1);
    let financier = actor(&sim, 5);
    let mut cautious = Personality::industrious();
    cautious.risk_tolerance = 0;
    sim.debug_set_personality(financier, cautious, Dream::Independence)
        .unwrap();
    let before = sim.canonical_state().inventories.clone();
    sim.advance(SimDuration(200_000)).unwrap();
    assert!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .agreements
            .values()
            .any(|agreement| agreement.status == AgreementStatus::Rejected)
    );
    assert_eq!(sim.canonical_state().inventories, before);
    assert!(
        sim.canonical_state()
            .projects
            .as_ref()
            .unwrap()
            .enterprises
            .is_empty()
    );
}

#[test]
fn procurement_uses_personal_hints_then_real_loading_travel_and_unloading() {
    let mut sim = fixture(2);
    sim.advance(SimDuration(3_600_000)).unwrap();
    let maker = actor(&sim, 3);
    let forest = sim
        .canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Timber
                && stock.location == InventoryLocation::Area(AreaId(3))
                && stock.capacity.0 == 100_000
                && stock.quantity.0 < 20_000
        })
        .unwrap()
        .id;
    assert!(sim.canonical_state().events.iter().any(|event|matches!(event.kind,EventKind::Transferred {actor,source,..} if actor==maker&&source==forest)));
    let plan = sim
        .canonical_state()
        .projects
        .as_ref()
        .unwrap()
        .projects
        .values()
        .find(|project| project.sponsor == maker)
        .unwrap();
    let enterprise =
        &sim.canonical_state().projects.as_ref().unwrap().enterprises[&plan.enterprise.unwrap()];
    assert!(enterprise.tools_produced > 3_000);
    assert!(sim.canonical_state().events.iter().any(
        |event| matches!(event.kind,EventKind::Departed {actor,to:AreaId(3),..} if actor==maker)
    ));
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

use game_api::*;
use game_world::{
    decisions::{Aspiration, Personality},
    property::Attitude,
    *,
};
use std::collections::BTreeSet;

fn fixture(workers: usize) -> Simulation {
    let mut sim = Simulation::property_fixture(
        9,
        Execution {
            workers,
            reverse_jobs: workers > 1,
        },
    )
    .unwrap();
    let people = sim
        .canonical_state()
        .people
        .iter()
        .map(|person| person.id)
        .collect::<Vec<_>>();
    for person in people {
        sim.debug_actor_command(person, Command::Cancel).unwrap();
        sim.debug_set_personality(person, Personality::restful(), Aspiration::Comfort)
            .unwrap();
        sim.debug_set_property_attitude(
            person,
            Attitude {
                exclusive_claims: 900,
                sharing: 0,
                willingness_to_defend: 0,
            },
        )
        .unwrap();
    }
    sim
}
fn actor(sim: &Simulation, index: usize) -> PersonId {
    sim.canonical_state().people[index].id
}
fn source(sim: &Simulation) -> InventoryId {
    sim.canonical_state()
        .inventories
        .values()
        .find(|stock| {
            stock.good == Good::Food && stock.location == InventoryLocation::Area(AreaId(1))
        })
        .unwrap()
        .id
}
fn cargo(sim: &Simulation, person: PersonId) -> InventoryId {
    sim.canonical_state()
        .people
        .iter()
        .find(|actor| actor.id == person)
        .unwrap()
        .inventory
}
fn action(sim: &mut Simulation, action: PropertyAction) {
    let caller_sequence = sim.canonical_state().events.len() as u64 + 100;
    sim.submit(Submission {
        caller_sequence,
        command: Command::Property { action },
    })
    .unwrap();
}
fn take(sim: &mut Simulation, quantity: u64) {
    let ruler = sim.canonical_state().ruler;
    action(
        sim,
        PropertyAction::Take {
            source: source(sim),
            destination: cargo(sim, ruler),
            quantity: Quantity(quantity),
        },
    );
}

#[test]
fn newly_created_cargo_keeps_complete_ownership_bookkeeping_after_real_loading() {
    let mut sim = fixture(1);
    let ruler = sim.canonical_state().ruler;
    let source = sim
        .debug_add_inventory(
            Good::Water,
            InventoryLocation::Area(AreaId(1)),
            Quantity(3_000),
            Quantity(3_000),
            BTreeSet::from([ruler]),
        )
        .unwrap();
    let mut replay = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 3,
            reverse_jobs: true,
        },
    )
    .unwrap();
    for world in [&mut sim, &mut replay] {
        world
            .submit(Submission {
                caller_sequence: world.next_caller_sequence().unwrap(),
                command: Command::Carry {
                    source,
                    quantity: Quantity(2_000),
                },
            })
            .unwrap();
    }
    sim.advance(MINUTE).unwrap();
    replay.advance(SimDuration(17_001)).unwrap();
    replay.advance(SimDuration(MINUTE.0 - 17_001)).unwrap();
    assert_eq!(sim.canonical_state(), replay.canonical_state());
    let state = sim.canonical_state();
    let cargo = state
        .inventories
        .values()
        .find(|i| i.good == Good::Water && i.location == InventoryLocation::Carried(ruler))
        .unwrap();
    assert_eq!(state.inventories[&source].quantity, Quantity(1_000));
    assert_eq!(cargo.quantity, Quantity(2_000));
    let portions = &state.property.as_ref().unwrap().portions;
    assert_eq!(portions.len(), state.inventories.len());
    for (id, inventory) in &state.inventories {
        assert_eq!(portions[id].values().sum::<u64>(), inventory.quantity.0);
    }
    assert_eq!(
        state,
        Simulation::load(&sim.save().unwrap(), Execution::default())
            .unwrap()
            .canonical_state()
    );
}

#[test]
fn theft_uses_loading_time_conserves_goods_and_keeps_claims_attached_to_cargo() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let origin = source(&sim);
    let destination = cargo(&sim, ruler);
    let before = sim.canonical_state().inventories[&origin].quantity.0;
    let claim = sim.property_view(ruler).claims[0].id;
    let assertion = sim.canonical_state().property.as_ref().unwrap().claims[&claim].clone();
    take(&mut sim, 1_000);
    sim.advance(SimDuration(59_999)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&origin].quantity.0,
        before
    );
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity.0,
        0
    );
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&origin].quantity.0,
        before - 1_000
    );
    assert_eq!(
        sim.canonical_state().inventories[&destination].quantity.0,
        1_000
    );
    let world = sim.canonical_state().property.as_ref().unwrap();
    assert_eq!(world.claims[&claim], assertion);
    assert_eq!(world.portions[&destination][&claim], 1_000);
    assert!(
        !sim.canonical_state().inventories[&origin]
            .access
            .contains(&ruler)
    );
    assert_eq!(
        sim.property_view(ruler).incidents[0].outcome,
        IncidentOutcome::Taken
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn competing_claims_and_recognition_do_not_grant_custody_or_access() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let owner = actor(&sim, 3);
    let origin = source(&sim);
    let original = sim.property_view(ruler).claims[0].id;
    action(
        &mut sim,
        PropertyAction::Claim {
            inventory: origin,
            rights: BTreeSet::from([ClaimRight::Consume, ClaimRight::GrantAccess]),
            evidence: Vec::new(),
        },
    );
    let own = sim
        .property_view(ruler)
        .claims
        .iter()
        .find(|claim| claim.claimant == ruler)
        .unwrap()
        .id;
    action(&mut sim, PropertyAction::Recognize { claim: original });
    sim.debug_property_action(owner, PropertyAction::Contest { claim: own })
        .unwrap();
    assert_eq!(
        sim.canonical_state()
            .property
            .as_ref()
            .unwrap()
            .claims
            .len(),
        2
    );
    assert_eq!(
        sim.canonical_state()
            .agreements
            .as_ref()
            .unwrap()
            .custodians[&origin],
        owner
    );
    assert!(
        !sim.canonical_state().inventories[&origin]
            .access
            .contains(&ruler)
    );
    assert_eq!(
        sim.debug_property_action(
            ruler,
            PropertyAction::Share {
                inventory: origin,
                recipient: actor(&sim, 5),
                allowed: true
            }
        ),
        Err(Error::Unavailable)
    );
    sim.debug_property_action(
        owner,
        PropertyAction::Share {
            inventory: origin,
            recipient: ruler,
            allowed: true,
        },
    )
    .unwrap();
    assert!(
        sim.canonical_state().inventories[&origin]
            .access
            .contains(&ruler)
    );
    assert!(
        sim.property_view(ruler)
            .claims
            .iter()
            .find(|claim| claim.id == own)
            .unwrap()
            .opposed_by
            .contains(&owner)
    );
}

#[test]
fn reports_take_time_and_transfer_remembered_incidents_without_live_global_updates() {
    let mut sim = fixture(1);
    let witness = actor(&sim, 5);
    let recipient = actor(&sim, 6);
    sim.debug_move_actor(recipient, AreaId(3)).unwrap();
    take(&mut sim, 1_000);
    let incident = sim.property_view(witness).incidents[0].id;
    sim.debug_move_actor(witness, AreaId(3)).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    assert_eq!(
        sim.canonical_state().property.as_ref().unwrap().incidents[&incident].outcome,
        IncidentOutcome::Taken
    );
    assert_eq!(
        sim.property_view(witness).incidents[0].outcome,
        IncidentOutcome::Attempt
    );
    assert!(sim.property_view(recipient).incidents.is_empty());
    sim.debug_property_action(
        witness,
        PropertyAction::ReportIncident {
            recipient,
            incident,
        },
    )
    .unwrap();
    sim.advance(SimDuration(59_999)).unwrap();
    assert!(sim.property_view(recipient).incidents.is_empty());
    for _ in 0..50 {
        sim.property_view(recipient);
        sim.observe();
    }
    sim.advance(SimDuration(1)).unwrap();
    let report = &sim.property_view(recipient).incidents[0];
    assert_eq!(report.outcome, IncidentOutcome::Attempt);
    assert_eq!(
        report.source_of_knowledge,
        IncidentSource::Reported { sender: witness }
    );
    assert_eq!(report.changed_at, SimTime(0));
    assert_eq!(report.learned_at, SimTime(240_000));
}

#[test]
fn willing_local_custodian_uses_real_combat_and_interrupts_loading_before_transfer() {
    let mut sim = fixture(2);
    let owner = actor(&sim, 3);
    sim.debug_set_property_attitude(
        owner,
        Attitude {
            exclusive_claims: 900,
            sharing: 0,
            willingness_to_defend: 900,
        },
    )
    .unwrap();
    let before = sim.canonical_state().inventories.clone();
    take(&mut sim, 1_000);
    assert_eq!(sim.canonical_state().inventories, before);
    assert!(matches!(
        sim.observe().physical.unwrap().activity,
        Activity::Combat { .. }
    ));
    assert_eq!(
        sim.property_view(actor(&sim, 0)).incidents[0].outcome,
        IncidentOutcome::Interrupted
    );
    let result = sim.advance(MINUTE).unwrap();
    assert!(matches!(result.reason, StopReason::CombatInput { .. }));
    assert!(
        sim.canonical_state()
            .property
            .as_ref()
            .unwrap()
            .pending
            .is_empty()
    );
    assert!(
        !sim.canonical_state()
            .events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Taken { .. }))
    );
}

#[test]
fn taking_can_invalidate_a_lawful_reservation_without_relocating_reserved_goods_twice() {
    let mut sim = fixture(1);
    let owner = actor(&sim, 3);
    let carrier = actor(&sim, 6);
    let ruler = actor(&sim, 0);
    let stock = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Carried(owner),
            Quantity(2_000),
            Quantity(2_000),
            BTreeSet::from([owner, carrier]),
        )
        .unwrap();
    sim.debug_actor_command(
        carrier,
        Command::Load {
            source: stock,
            destination: cargo(&sim, carrier),
            quantity: Quantity(2_000),
        },
    )
    .unwrap();
    let destination = cargo(&sim, ruler);
    action(
        &mut sim,
        PropertyAction::Take {
            source: stock,
            destination,
            quantity: Quantity(1_000),
        },
    );
    sim.advance(MINUTE).unwrap();
    assert_eq!(sim.canonical_state().inventories[&stock].quantity.0, 1_000);
    assert_eq!(
        sim.canonical_state().inventories[&cargo(&sim, ruler)]
            .quantity
            .0,
        1_000
    );
    assert_eq!(
        sim.canonical_state().inventories[&cargo(&sim, carrier)]
            .quantity
            .0,
        0
    );
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|event| matches!(event.kind,EventKind::Interrupted { actor } if actor == carrier))
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn observers_can_interpret_the_same_taking_differently() {
    let mut sim = fixture(1);
    let owner = actor(&sim, 3);
    let supportive = actor(&sim, 6);
    let ruler = actor(&sim, 0);
    sim.debug_set_property_attitude(
        supportive,
        Attitude {
            exclusive_claims: 0,
            sharing: 900,
            willingness_to_defend: 0,
        },
    )
    .unwrap();
    let before = sim.canonical_state().decisions.as_ref().unwrap().minds[&owner]
        .relationships
        .get(&ruler)
        .map(|r| r.trust)
        .unwrap_or(0);
    take(&mut sim, 1_000);
    sim.advance(MINUTE).unwrap();
    let minds = &sim.canonical_state().decisions.as_ref().unwrap().minds;
    assert_eq!(minds[&owner].relationships[&ruler].trust, before - 100);
    assert_eq!(minds[&supportive].relationships[&ruler].trust, 10);
}

#[test]
fn partial_taking_is_equivalent_across_workers_restore_and_passive_views() {
    let mut a = fixture(1);
    let mut b = fixture(4);
    take(&mut a, 1_000);
    take(&mut b, 1_000);
    a.advance(MINUTE).unwrap();
    b.advance(SimDuration(17_333)).unwrap();
    for _ in 0..40 {
        b.observe();
        b.property_view(actor(&b, 5));
    }
    b = Simulation::load(
        &b.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    b.advance(SimDuration(42_667)).unwrap();
    assert_eq!(a.canonical_state(), b.canonical_state());
}

#[test]
fn failed_taking_is_inert_and_canceling_does_not_return_or_create_cargo() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let before = sim.canonical_state().clone();
    assert_eq!(
        sim.debug_property_action(
            ruler,
            PropertyAction::Take {
                source: source(&sim),
                destination: cargo(&sim, ruler),
                quantity: Quantity(10_000)
            }
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(sim.canonical_state(), &before);
    take(&mut sim, 1_000);
    sim.advance(SimDuration(30_000)).unwrap();
    sim.debug_actor_command(ruler, Command::Cancel).unwrap();
    sim.advance(SimDuration(30_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&cargo(&sim, ruler)]
            .quantity
            .0,
        0
    );
    assert_eq!(
        sim.property_view(ruler).incidents[0].outcome,
        IncidentOutcome::Interrupted
    );
}

#[test]
fn actual_custodians_choose_to_share_or_refuse_a_local_request() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let owner = actor(&sim, 3);
    let stock = source(&sim);
    action(&mut sim, PropertyAction::RequestAccess { inventory: stock });
    assert!(
        !sim.canonical_state().inventories[&stock]
            .access
            .contains(&ruler)
    );
    assert!(
        sim.canonical_state().events.iter().any(
            |event| matches!(event.kind,EventKind::AccessRefused { actor,.. } if actor == ruler)
        )
    );
    sim.debug_set_property_attitude(
        owner,
        Attitude {
            exclusive_claims: 300,
            sharing: 900,
            willingness_to_defend: 0,
        },
    )
    .unwrap();
    action(&mut sim, PropertyAction::RequestAccess { inventory: stock });
    assert!(
        sim.canonical_state().inventories[&stock]
            .access
            .contains(&ruler)
    );
    assert_eq!(
        sim.canonical_state().inventories[&stock].quantity.0,
        100_000
    );
}

#[test]
fn consumption_removes_claimed_portions_and_new_production_stays_unclaimed() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let stock = source(&sim);
    let inventory = cargo(&sim, ruler);
    let claim = sim.property_view(ruler).claims[0].id;
    take(&mut sim, 1_000);
    sim.advance(MINUTE).unwrap();
    sim.debug_actor_command(ruler, Command::Eat { source: inventory })
        .unwrap();
    sim.advance(MINUTE).unwrap();
    assert_eq!(sim.canonical_state().inventories[&inventory].quantity.0, 0);
    assert!(sim.canonical_state().property.as_ref().unwrap().portions[&inventory].is_empty());
    let site = sim
        .canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .sites
        .values()
        .find(|site| site.area == AreaId(1))
        .unwrap()
        .id;
    sim.debug_actor_command(ruler, Command::Work { site })
        .unwrap();
    sim.advance(MINUTE).unwrap();
    let world = sim.canonical_state().property.as_ref().unwrap();
    assert_eq!(world.portions[&stock][&ClaimId(0)], 1_000);
    assert_eq!(world.portions[&stock][&claim], 99_000);
    assert_eq!(
        sim.canonical_state().inventories[&stock].quantity.0,
        100_000
    );
    let owner = actor(&sim, 3);
    let rights = world.claims[&claim].rights.clone();
    sim.debug_property_action(
        owner,
        PropertyAction::Claim {
            inventory: stock,
            rights,
            evidence: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(
        sim.canonical_state()
            .property
            .as_ref()
            .unwrap()
            .claims
            .len(),
        2
    );
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
}

#[test]
fn a_hungry_risk_taking_npc_can_steal_from_known_local_food_without_remote_awareness() {
    let mut sim = fixture(2);
    let owner = actor(&sim, 3);
    let thief = actor(&sim, 5);
    let mut personality = Personality::restful();
    personality.honesty = 0;
    personality.risk_tolerance = 900;
    sim.debug_set_personality(thief, personality, Aspiration::Comfort)
        .unwrap();
    sim.debug_move_actor(owner, AreaId(3)).unwrap();
    sim.advance(SimDuration(360_000)).unwrap();
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|event| matches!(event.kind,EventKind::Taken { actor,.. } if actor == thief))
    );
    assert_eq!(
        sim.canonical_state().inventories[&cargo(&sim, thief)]
            .quantity
            .0,
        1_000
    );
    assert!(sim.property_view(owner).incidents.is_empty());
    assert_eq!(
        sim.property_view(thief).incidents[0].outcome,
        IncidentOutcome::Taken
    );
}

#[test]
fn permission_granted_during_loading_does_not_stop_the_physical_transfer() {
    let mut sim = fixture(1);
    let ruler = actor(&sim, 0);
    let owner = actor(&sim, 3);
    let stock = source(&sim);
    take(&mut sim, 1_000);
    sim.advance(SimDuration(30_000)).unwrap();
    sim.debug_property_action(
        owner,
        PropertyAction::Share {
            inventory: stock,
            recipient: ruler,
            allowed: true,
        },
    )
    .unwrap();
    assert!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&ruler].knowledge[&stock]
            .accessible
    );
    sim.advance(SimDuration(30_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&cargo(&sim, ruler)]
            .quantity
            .0,
        1_000
    );
    assert_eq!(
        sim.property_view(ruler).incidents[0].outcome,
        IncidentOutcome::Taken
    );
    assert_eq!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&owner]
            .relationships
            .get(&ruler)
            .map(|relationship| relationship.trust)
            .unwrap_or(0),
        0
    );
}

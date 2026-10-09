use game_api::*;
use game_sim::{
    decisions::{Dream, Personality},
    *,
};
use std::collections::BTreeSet;

fn sim(workers: usize) -> Simulation {
    Simulation::autonomy_fixture(
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}
fn peaceful(sim: &mut Simulation) {
    let people: Vec<_> = sim.canonical_state().people.iter().map(|p| p.id).collect();
    for id in people {
        sim.debug_set_personality(id, Personality::restful(), Dream::Comfort)
            .unwrap();
    }
}

#[test]
fn personality_changes_autonomous_choices_with_the_same_physical_opportunities() {
    let mut sim = sim(4);
    peaceful(&mut sim);
    let worker = sim.canonical_state().people[3].id;
    let resting = sim.canonical_state().people[4].id;
    sim.debug_set_personality(worker, Personality::industrious(), Dream::Independence)
        .unwrap();
    sim.advance(MINUTE).unwrap();
    let physical = sim.canonical_state().physical.as_ref().unwrap();
    assert!(matches!(
        physical.actors[&worker].activity,
        Activity::Work { .. }
    ));
    assert_eq!(physical.actors[&resting].activity, Activity::Rest);
    let mind = &sim.canonical_state().decisions.as_ref().unwrap().minds[&worker];
    assert_eq!(mind.goal, decisions::Goal::Practice);
    assert!(mind.explanation.benefit > 0);
    sim.advance(MINUTE).unwrap();
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Produced { actor, .. } if actor == worker))
    );
    assert!(sim.canonical_state().people[3].work_units > 0);
}

#[test]
fn meaningful_experience_acquires_a_value_that_changes_a_later_choice() {
    let mut reference = sim(1);
    peaceful(&mut reference);
    let actor = reference.canonical_state().people[3].id;
    let site = reference
        .canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .sites
        .values()
        .find(|s| s.area == AreaId(1))
        .unwrap()
        .id;
    for _ in 0..3 {
        reference
            .debug_actor_command(actor, Command::Work { site })
            .unwrap();
        reference.advance(MINUTE).unwrap();
    }
    let world = reference.canonical_state().decisions.as_ref().unwrap();
    assert_eq!(
        world.minds[&actor].values[&world.work_concept].experiences,
        3
    );
    let mut bytes: serde_json::Value = serde_json::from_slice(&reference.save().unwrap()).unwrap();
    bytes["state"]["decisions"]["minds"][actor.0.to_string()]["values"] = serde_json::json!({});
    let mut without_learning =
        Simulation::load(&serde_json::to_vec(&bytes).unwrap(), Execution::default()).unwrap();
    assert_eq!(
        reference.canonical_state().physical,
        without_learning.canonical_state().physical
    );
    assert_eq!(
        reference.canonical_state().inventories,
        without_learning.canonical_state().inventories
    );
    // Let both copies recover and eat under identical physical conditions before
    // comparing the learned preference at the next nonurgent decision.
    reference.advance(SimDuration(180_000)).unwrap();
    without_learning.advance(SimDuration(180_000)).unwrap();
    assert!(matches!(
        reference
            .canonical_state()
            .physical
            .as_ref()
            .unwrap()
            .actors[&actor]
            .activity,
        Activity::Work { .. }
    ));
    assert_eq!(
        without_learning
            .canonical_state()
            .physical
            .as_ref()
            .unwrap()
            .actors[&actor]
            .activity,
        Activity::Rest
    );
}

#[test]
fn distant_facts_require_a_timed_report_and_keep_their_original_age() {
    let mut sim = sim(4);
    peaceful(&mut sim);
    let sender = sim.canonical_state().people[3].id;
    let receiver = sim.canonical_state().ruler;
    let remote = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(1_000),
            Quantity(1_000),
            BTreeSet::from([sender]),
        )
        .unwrap();
    sim.debug_set_knowledge(
        sender,
        StockKnowledge {
            inventory: remote,
            area: AreaId(3),
            good: Good::Food,
            quantity: Quantity(1_000),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(sender),
        },
    )
    .unwrap();
    sim.debug_spoil_at(remote, SimTime(30_000)).unwrap();
    sim.advance(SimDuration(30_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&remote].quantity,
        Quantity(0)
    );
    assert!(
        !sim.observe()
            .known_stocks
            .iter()
            .any(|f| f.inventory == remote)
    );
    sim.debug_actor_command(
        sender,
        Command::Tell {
            recipient: receiver,
            inventory: remote,
        },
    )
    .unwrap();
    sim.advance(SimDuration(59_999)).unwrap();
    assert!(
        !sim.observe()
            .known_stocks
            .iter()
            .any(|f| f.inventory == remote)
    );
    sim.advance(SimDuration(1)).unwrap();
    let fact = sim
        .observe()
        .known_stocks
        .into_iter()
        .find(|f| f.inventory == remote)
        .unwrap();
    assert_eq!(fact.quantity, Quantity(1_000));
    assert_eq!(fact.observed_at, SimTime(0));
    assert_eq!(fact.learned_at, SimTime(90_000));
    assert_eq!(fact.source, KnowledgeSource::Reported(sender));
    assert!(!fact.accessible);
    assert_eq!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&receiver].relationships[&sender]
            .trust,
        10
    );
}

#[test]
fn normal_views_are_passive_and_never_expose_other_peoples_reasoning() {
    let mut sim = sim(2);
    sim.advance(SimDuration(120_000)).unwrap();
    let before = sim.canonical_state().clone();
    for _ in 0..100 {
        let json = serde_json::to_string(&sim.observe()).unwrap();
        assert!(
            !json.contains("explanation")
                && !json.contains("personality")
                && !json.contains("switching_margin")
        );
    }
    assert_eq!(&before, sim.canonical_state());
}

#[test]
fn decisions_conservation_events_and_learned_values_survive_chunking_and_workers() {
    let mut reference = sim(1);
    reference.advance(SimDuration(487_123)).unwrap();
    for workers in [2, 4] {
        let mut split = sim(workers);
        for dt in [1, 17, 59_981, 4_000, 123_001, 300_123] {
            split.advance(SimDuration(dt)).unwrap();
        }
        assert_eq!(reference.canonical_state(), split.canonical_state());
        let mut loaded = Simulation::load(&split.save().unwrap(), Execution::default()).unwrap();
        split.advance(SimDuration(123_456)).unwrap();
        loaded.advance(SimDuration(123_456)).unwrap();
        assert_eq!(split.canonical_state(), loaded.canonical_state());
    }
}

#[test]
fn telling_a_current_local_fact_matches_the_passive_view_even_between_reviews() {
    let mut sim = sim(1);
    peaceful(&mut sim);
    let ruler = sim.canonical_state().ruler;
    let receiver = sim.canonical_state().people[4].id;
    let source = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id;
    sim.debug_spoil_at(source, SimTime(13_000)).unwrap();
    sim.advance(SimDuration(13_000)).unwrap();
    assert_eq!(
        sim.observe()
            .physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.id == source)
            .unwrap()
            .quantity,
        Quantity(0)
    );
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Tell {
            recipient: receiver,
            inventory: source,
        },
    })
    .unwrap();
    assert_eq!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&ruler].knowledge[&source].quantity,
        Quantity(0)
    );
    sim.advance(MINUTE).unwrap();
    assert!(sim.canonical_state().events.iter().any(|e| e.kind
        == EventKind::Reported {
            sender: ruler,
            recipient: receiver,
            inventory: source
        }));
}

fn remote_supply(spoils: bool, workers: usize) -> (Simulation, PersonId, InventoryId) {
    let mut sim = sim(workers);
    peaceful(&mut sim);
    let actor = sim.canonical_state().people[3].id;
    let village_food = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| i.good == Good::Food && i.location == InventoryLocation::Area(AreaId(1)))
        .unwrap()
        .id;
    let mut data: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    data["state"]["inventories"][village_food.0.to_string()]["quantity"] = 0.into();
    let mut sim = Simulation::load(
        &serde_json::to_vec(&data).unwrap(),
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let source = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Area(AreaId(3)),
            Quantity(1_000),
            Quantity(10_000),
            BTreeSet::from([actor]),
        )
        .unwrap();
    sim.debug_set_knowledge(
        actor,
        StockKnowledge {
            inventory: source,
            area: AreaId(3),
            good: Good::Food,
            quantity: Quantity(1_000),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Reported(sim.canonical_state().ruler),
        },
    )
    .unwrap();
    if spoils {
        sim.debug_spoil_at(source, SimTime(400_000)).unwrap();
    }
    (sim, actor, source)
}

#[test]
fn a_citizen_travels_for_known_food_without_creating_work_or_teleporting_supplies() {
    let (mut sim, actor, source) = remote_supply(false, 4);
    sim.advance(SimDuration(300_000)).unwrap();
    let index = sim
        .canonical_state()
        .people
        .iter()
        .position(|p| p.id == actor)
        .unwrap();
    assert!(matches!(
        sim.canonical_state().people[index].location,
        Location::InTransit {
            arrives: SimTime(480_000),
            ..
        }
    ));
    sim.advance(SimDuration(240_000)).unwrap();
    assert_eq!(
        sim.canonical_state().people[index].location,
        Location::AtArea(AreaId(3))
    );
    assert_eq!(sim.canonical_state().people[index].work_units, 0);
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::Ate { actor: id, source: inventory, .. } if id == actor && inventory == source)));
}

#[test]
fn stale_supply_information_survives_until_arrival_and_then_changes_the_plan() {
    let (mut sim, actor, source) = remote_supply(true, 1);
    sim.advance(SimDuration(420_000)).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&source].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.canonical_state().decisions.as_ref().unwrap().minds[&actor].knowledge[&source].quantity,
        Quantity(1_000)
    );
    let mut loaded = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    sim.advance(SimDuration(60_001)).unwrap();
    loaded.advance(SimDuration(60_001)).unwrap();
    assert_eq!(sim.canonical_state(), loaded.canonical_state());
    let mind = &sim.canonical_state().decisions.as_ref().unwrap().minds[&actor];
    assert_eq!(mind.knowledge[&source].quantity, Quantity(0));
    assert_eq!(mind.last_failure, Some((SimTime(480_000), source)));
    assert_eq!(mind.plan, Some(Command::Rest));
}

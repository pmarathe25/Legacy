use game_api::{
    AreaId, Command, EventKind, Good, HealthRules, Location, PersonId, Role, SimDuration,
};
use game_sim::{
    Execution, Simulation,
    decisions::{Dream, Personality},
};

const HOUR: u64 = 3_600_000;

// Author a dry opening once, before its clock runs. The public simulation then
// has to discover, collect, carry and consume actual remaining natural water.
fn dry_opening(private: bool, knows_routes: bool) -> (Simulation, PersonId) {
    let mut sim = Simulation::ecology_fixture(81, Execution::default()).unwrap();
    sim.initialize_health(HealthRules::human()).unwrap();
    let actor = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.role == Role::Citizen && p.location == Location::AtArea(AreaId(1)))
        .unwrap()
        .id;
    for person in sim.canonical_state().people.clone() {
        sim.debug_actor_command(person.id, Command::Cancel).unwrap();
        sim.debug_set_personality(person.id, Personality::restful(), Dream::Comfort)
            .unwrap();
    }
    let ruler = sim.canonical_state().ruler;
    let mut snapshot: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let state = &mut snapshot["state"];
    for stock in state["inventories"].as_object_mut().unwrap().values_mut() {
        if stock["good"] == "Water" {
            stock["quantity"] = 0.into();
        }
    }
    for mind in state["decisions"]["minds"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        for fact in mind["knowledge"].as_object_mut().unwrap().values_mut() {
            if fact["good"] == "Water" {
                fact["quantity"] = 0.into();
            }
        }
        mind["plan"] = serde_json::Value::Null;
    }
    for source in state["ecology"]["resources"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        if source["kind"] != "Spring" {
            continue;
        }
        source["daily_growth"] = 0.into();
        source["contamination"] = 0.into();
        source["runoff_contamination"] = 0.into();
        if source["area"] == 1 {
            source["remaining"] = 0.into();
        } else if private {
            source["area"] = 1.into();
            source["access"] = serde_json::json!([ruler.0]);
            source["custodian"] = ruler.0.into();
        }
    }
    if !knows_routes {
        state["decisions"]["minds"][actor.0.to_string()]["routes"] = serde_json::json!({"1":{}});
    }
    let sim = Simulation::load(
        &serde_json::to_vec(&snapshot).unwrap(),
        Execution::default(),
    )
    .unwrap();
    (sim, actor)
}

#[test]
fn a_thirsty_person_searches_known_routes_and_collects_real_water_after_local_stocks_run_dry() {
    let (mut sim, actor) = dry_opening(false, true);
    let initial = sim
        .canonical_state()
        .ecology
        .as_ref()
        .unwrap()
        .resources
        .values()
        .filter(|r| r.kind.good() == Good::Water)
        .map(|r| r.remaining.0)
        .sum::<u64>();
    assert!(
        sim.canonical_state().ecology.as_ref().unwrap().known[&actor]
            .values()
            .all(|s| s.area != AreaId(3))
    );
    sim.advance(SimDuration(2 * HOUR + 20 * 60_000)).unwrap();
    let state = sim.canonical_state();
    let collected = state
        .events
        .iter()
        .find_map(|e| match e.kind {
            EventKind::ResourceCollected {
                actor: a,
                destination,
                good: Good::Water,
                ..
            } if a == actor => Some(destination),
            _ => None,
        })
        .expect("a thirsty ordinary person must physically collect water");
    assert!(state.events.iter().any(|e| matches!(e.kind,
        EventKind::Drank {actor:a,source,..} if a==actor && source==collected)));
    assert_eq!(
        state
            .people
            .iter()
            .find(|p| p.id == actor)
            .unwrap()
            .location,
        Location::AtArea(AreaId(3))
    );
    assert!(state.health.as_ref().unwrap().clinical[&actor].thirst < HOUR);
    assert!(
        state.health.as_ref().unwrap().known[&actor]
            .water_searched
            .contains_key(&AreaId(2))
    );
    let remaining = state
        .ecology
        .as_ref()
        .unwrap()
        .resources
        .values()
        .filter(|r| r.kind.good() == Good::Water)
        .map(|r| r.remaining.0)
        .sum::<u64>();
    let carried = state
        .inventories
        .values()
        .filter(|s| s.good == Good::Water)
        .map(|s| s.quantity.0)
        .sum::<u64>();
    let consumed = state
        .events
        .iter()
        .map(|e| match e.kind {
            EventKind::Drank { quantity, .. } => quantity.0,
            _ => 0,
        })
        .sum::<u64>();
    assert_eq!(initial, remaining + carried + consumed);
    sim.save().unwrap();
}

#[test]
fn a_present_custodian_can_refuse_water_without_granting_access_or_forcing_a_theft() {
    let (mut sim, actor) = dry_opening(true, false);
    let private = sim
        .canonical_state()
        .ecology
        .as_ref()
        .unwrap()
        .resources
        .values()
        .find(|r| r.kind.good() == Good::Water && r.remaining.0 > 0)
        .unwrap()
        .id;
    let quantity = sim.canonical_state().ecology.as_ref().unwrap().resources[&private].remaining;
    sim.advance(SimDuration(2 * HOUR + 10 * 60_000)).unwrap();
    let state = sim.canonical_state();
    assert!(
        state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceAccessRequested {resource,actor:a,..} if resource==private && a==actor))
    );
    assert!(state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceAccessRefused {resource,actor:a,..} if resource==private && a==actor)));
    assert!(
        !state.ecology.as_ref().unwrap().resources[&private]
            .access
            .contains(&actor)
    );
    assert_eq!(
        state.ecology.as_ref().unwrap().resources[&private].remaining,
        quantity
    );
    assert!(!state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceCollected {actor:a,..} if a==actor)));
    assert!(state.health.as_ref().unwrap().clinical[&actor].thirst >= 2 * HOUR);
    sim.save().unwrap();
}

#[test]
fn unknown_remote_water_is_not_a_lead_and_procurement_survives_save_workers_and_slices() {
    let (mut unknown, actor) = dry_opening(false, false);
    unknown
        .advance(SimDuration(2 * HOUR + 10 * 60_000))
        .unwrap();
    assert_eq!(
        unknown
            .canonical_state()
            .people
            .iter()
            .find(|p| p.id == actor)
            .unwrap()
            .location,
        Location::AtArea(AreaId(1))
    );
    assert!(
        unknown.canonical_state().ecology.as_ref().unwrap().known[&actor]
            .values()
            .all(|s| s.area == AreaId(1))
    );
    assert!(
        !unknown
            .canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.kind,
        EventKind::ResourceCollected {actor:a,..} if a==actor))
    );
    let (mut whole, _) = dry_opening(false, true);
    whole.advance(SimDuration(2 * HOUR + 61_003)).unwrap();
    let mut restored = Simulation::load(
        &whole.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    whole.advance(SimDuration(18 * 60_000 + 997)).unwrap();
    for duration in [17, 72_983, 360_001, 647_996] {
        restored.advance(SimDuration(duration)).unwrap();
    }
    assert_eq!(whole.canonical_state(), restored.canonical_state());
}

#[test]
fn normal_population_uses_the_renewable_spring_after_consuming_its_authored_stocks() {
    let config = game_sim::scenarios::Config {
        genetics: None,
        health: HealthRules {
            drink_relief: 30 * 60_000,
            ..HealthRules::human()
        },
        ..game_sim::scenarios::Config::default()
    };
    let mut sim = Simulation::populated_world(
        82,
        Execution::default(),
        config,
        game_sim::population::Plan {
            people: 100,
            settlement_size: 100,
        },
    )
    .unwrap();
    sim.advance(SimDuration(12 * HOUR)).unwrap();
    let state = sim.canonical_state();
    assert!(state.events.iter().any(|e| matches!(
        e.kind,
        EventKind::ResourceCollected {
            good: Good::Water,
            ..
        }
    )));
    assert!(state.events.iter().any(|e|matches!(e.kind,
        EventKind::Drank {source,..} if state.inventories[&source].location != game_sim::InventoryLocation::Area(state.areas[0].id))));
    assert!(
        state
            .physical
            .as_ref()
            .unwrap()
            .actors
            .values()
            .filter(|b| b.vitals.alive)
            .count()
            > 90
    );
    sim.save().unwrap();
}

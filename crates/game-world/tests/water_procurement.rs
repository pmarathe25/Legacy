mod support;
use game_api::{
    AreaId, Command, EventKind, Good, HealthRules, Location, PersonId, Role, SimDuration,
};
use game_world::{
    Execution, Simulation,
    decisions::{Aspiration, Personality},
};

const HOUR: u64 = 3_600_000;

#[test]
fn a_guardian_collects_and_returns_water_for_a_locally_observed_infant() {
    let mut sim = Simulation::playable_scenario(
        "child-successor",
        81,
        Execution::default(),
        game_world::scenarios::Config::default(),
    )
    .unwrap();
    sim.advance(SimDuration(60_000)).unwrap();
    let child = sim.canonical_state().ruler;
    assert_eq!(sim.observe().life_stage, Some(game_api::LifeStage::Infant));
    let guardians: Vec<_> = sim
        .canonical_state()
        .lineage
        .as_ref()
        .unwrap()
        .guardians
        .keys()
        .copied()
        .collect();
    for person in sim.canonical_state().people.clone() {
        if person.id != child
            && sim
                .canonical_state()
                .components
                .body(person.id)
                .unwrap()
                .vitals
                .alive
        {
            sim.debug_actor_command(person.id, Command::Cancel).unwrap();
            let mut personality = Personality::restful();
            personality.empathy = 0;
            sim.debug_set_personality(person.id, personality, Aspiration::Comfort)
                .unwrap();
        }
    }
    let mut snapshot: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let state = &mut snapshot["state"];
    for stock in state["inventories"].as_object_mut().unwrap().values_mut() {
        if stock["good"] == "Water" {
            stock["quantity"] = 0.into();
        }
    }
    let water_ids: Vec<_> = state["inventories"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, stock)| stock["good"] == "Water")
        .map(|(id, _)| id.clone())
        .collect();
    for id in water_ids {
        state["property"]["portions"][&id] = serde_json::json!({});
        let entity = id.parse::<u64>().expect("inventory identity");
        let value = serde_json::json!({
            "basis_quantity": 0, "basis_load": 0,
            "removed_quantity": 0, "removed_load": 0,
        });
        let rows = state["components"]["columns"]["health/water"]
            .as_array_mut()
            .expect("actual registered water lot column");
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row["entity"].as_u64() == Some(entity))
        {
            row["value"] = value;
        } else {
            rows.push(serde_json::json!({"entity": entity, "value": value}));
            rows.sort_by_key(|row| row["entity"].as_u64().unwrap());
        }
        let entities = state["components"]["entities"].as_array_mut().unwrap();
        if !entities.iter().any(|id| id.as_u64() == Some(entity)) {
            entities.push(entity.into());
            entities.sort_by_key(|id| id.as_u64().unwrap());
        }
    }
    for mind in state["decisions"]["minds"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        mind["plan"] = serde_json::Value::Null;
        for fact in mind["knowledge"].as_object_mut().unwrap().values_mut() {
            if fact["good"] == "Water" {
                fact["quantity"] = 0.into();
            }
        }
    }
    for row in state["components"]["columns"]["ecology/resource"]
        .as_array_mut()
        .expect("actual saved resource column")
    {
        let source = row.get_mut("value").expect("saved resource payload");
        if source["kind"] == "Spring" {
            source["daily_growth"] = 0.into();
            source["contamination"] = 0.into();
            source["runoff_contamination"] = 0.into();
            if source["area"] == 1 {
                source["remaining"] = 0.into();
            }
        }
    }
    support::component(&mut snapshot, "health/clinical", child)["thirst"] = (2 * HOUR).into();
    let mut sim = Simulation::load(
        &serde_json::to_vec(&snapshot).unwrap(),
        Execution::default(),
    )
    .unwrap();
    let mut split = Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let initial_events = sim.canonical_state().events.len();
    sim.advance(SimDuration(30 * 60_000)).unwrap();
    split.advance(SimDuration(60_000)).unwrap();
    let mut split = Simulation::load(
        &split.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    split.advance(SimDuration(29 * 60_000)).unwrap();
    assert_eq!(sim.canonical_state(), split.canonical_state());
    let events = &sim.canonical_state().events[initial_events..];
    let collector = events
        .iter()
        .find_map(|e| match e.kind {
            EventKind::ResourceCollected {
                actor,
                good: Good::Water,
                ..
            } if guardians.contains(&actor) => Some(actor),
            _ => None,
        })
        .expect("an actual guardian must collect remaining distant water");
    assert!(events.iter().any(|e| matches!(e.kind,
        EventKind::Cared { caregiver, child: target, .. }
            if caregiver == collector && target == child)));
    assert!(
        sim.canonical_state()
            .components
            .body(child)
            .unwrap()
            .vitals
            .alive
    );
    assert!(
        sim.canonical_state()
            .components
            .clinical(child)
            .expect("clinical state exists")
            .thirst
            < 2 * HOUR
    );
    Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
}

// Author a dry opening once, before its clock runs. The public simulation then
// has to discover, collect, carry and consume actual remaining natural water.
fn dry_opening(private: bool, knows_routes: bool) -> (Simulation, PersonId) {
    let mut sim = Simulation::ecology_fixture(81, Execution::default()).unwrap();
    sim.initialize_health(HealthRules::human()).unwrap();
    let actor = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| {
            p.role == Role::Citizen
                && *sim
                    .canonical_state()
                    .components
                    .location(p.id)
                    .expect("location exists")
                    == Location::AtArea(AreaId(1))
        })
        .unwrap()
        .id;
    for person in sim.canonical_state().people.clone() {
        sim.debug_actor_command(person.id, Command::Cancel).unwrap();
        sim.debug_set_personality(person.id, Personality::restful(), Aspiration::Comfort)
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
    for row in state["components"]["columns"]["ecology/resource"]
        .as_array_mut()
        .expect("actual saved resource column")
    {
        let source = row.get_mut("value").expect("saved resource payload");
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
        .resources(&sim.canonical_state().components)
        .filter(|(_, resource)| resource.kind.good() == Good::Water)
        .map(|(_, resource)| resource.remaining.0)
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
        *state.components.location(actor).expect("location exists"),
        Location::AtArea(AreaId(3))
    );
    assert!(
        state
            .components
            .clinical(actor)
            .expect("clinical state exists")
            .thirst
            < HOUR
    );
    assert!(state.ecology.as_ref().unwrap().observed_areas[&actor].contains_key(&AreaId(2)));
    let remaining = state
        .ecology
        .as_ref()
        .unwrap()
        .resources(&state.components)
        .filter(|(_, resource)| resource.kind.good() == Good::Water)
        .map(|(_, resource)| resource.remaining.0)
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
        .resources(&sim.canonical_state().components)
        .find(|(_, resource)| resource.kind.good() == Good::Water && resource.remaining.0 > 0)
        .unwrap()
        .0;
    let quantity = sim
        .canonical_state()
        .ecology
        .as_ref()
        .unwrap()
        .resource(&sim.canonical_state().components, private)
        .unwrap()
        .remaining;
    sim.advance(SimDuration(2 * HOUR + 10 * 60_000)).unwrap();
    let state = sim.canonical_state();
    assert!(
        state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceAccessRequested {resource,actor:a,..} if resource==private && a==actor))
    );
    assert!(state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceAccessRefused {resource,actor:a,..} if resource==private && a==actor)));
    assert!(
        !state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&state.components, private)
            .unwrap()
            .access
            .contains(&actor)
    );
    assert_eq!(
        state
            .ecology
            .as_ref()
            .unwrap()
            .resource(&state.components, private)
            .unwrap()
            .remaining,
        quantity
    );
    assert!(!state.events.iter().any(|e| matches!(e.kind,
        EventKind::ResourceCollected {actor:a,..} if a==actor)));
    assert!(
        state
            .components
            .clinical(actor)
            .expect("clinical state exists")
            .thirst
            >= 2 * HOUR
    );
    sim.save().unwrap();
}

#[test]
fn unknown_remote_water_is_not_a_lead_and_procurement_survives_save_workers_and_slices() {
    let (mut unknown, actor) = dry_opening(false, false);
    assert!(
        unknown.canonical_state().ecology.as_ref().unwrap().known[&actor]
            .values()
            .all(|source| source.area == AreaId(1))
    );
    unknown
        .advance(SimDuration(2 * HOUR + 10 * 60_000))
        .unwrap();
    let state = unknown.canonical_state();
    // Local roads are observable even after authored route memory is cleared.
    // Exploring them can discover water, but distant facts require real arrival.
    let arrivals = state
        .events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::Arrived {
                actor: person,
                area,
            } if person == actor => Some((area, event.time)),
            _ => None,
        })
        .collect::<Vec<_>>();
    for source in state.ecology.as_ref().unwrap().known[&actor]
        .values()
        .filter(|source| source.area != AreaId(1))
    {
        assert!(
            arrivals
                .iter()
                .any(|(area, at)| *area == source.area && *at <= source.observed_at)
        );
    }
    let collected = state
        .events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::ResourceCollected {
                actor: person,
                resource,
                ..
            } if person == actor => Some((resource, event.time)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !collected.is_empty(),
        "personally observed roads permit actual exploration"
    );
    for (resource, time) in collected {
        let source = &state.ecology.as_ref().unwrap().known[&actor][&resource];
        assert!(
            arrivals
                .iter()
                .any(|(area, at)| *area == source.area && *at < time)
        );
    }
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
    let config = game_world::scenarios::Config {
        genetics: None,
        health: HealthRules {
            drink_relief: 30 * 60_000,
            ..HealthRules::human()
        },
        ..game_world::scenarios::Config::default()
    };
    let mut sim = Simulation::populated_world(
        82,
        Execution::default(),
        config,
        game_world::population::Plan {
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
        EventKind::Drank {source,..} if state.inventories[&source].location != game_world::InventoryLocation::Area(state.areas[0].id))));
    assert!(
        state
            .components
            .bodies()
            .filter(|(_, body)| body.vitals.alive)
            .count()
            > 90
    );
    sim.save().unwrap();
}

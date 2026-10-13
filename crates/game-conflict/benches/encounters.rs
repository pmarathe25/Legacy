//! Local keyed scene/contact reads and one bounded encounter initiative update.
use game_conflict::*;
use game_core::*;
use game_ecs::{Schema, StableId, Store};
use std::{collections::BTreeMap, hint::black_box, time::Instant};
fn fixture(size: usize) -> (World, Store, EncounterId) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_spatial::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let area = AreaId(size as u64 + 1);
    store.spawn(StableId(area.0)).unwrap();
    let world = World {
        rules: Rules {
            encounter_limit: 4096,
            round: SimDuration(6000),
            movement: 5000,
            reach: 2000,
            damage: 45,
            defence: 30,
            incapacitation: 20,
        },
    };
    let scene = Map {
        id: CombatMapId(size as u64 + 2),
        version: 1,
        area,
        location: Location::AtArea(area),
        width: 24000,
        height: 16000,
        obstacles: Vec::new(),
        exits: Vec::new(),
    };
    let mut fighters = BTreeMap::new();
    for i in 0..size {
        let actor = PersonId(i as u64 + 1);
        insert_profile(&mut store, actor, Profile { initiative: 100 }).unwrap();
        game_spatial::set_location(&mut store, StableId(actor.0), Location::AtArea(area)).unwrap();
        fighters.insert(
            actor,
            Fighter {
                actor,
                joins_round: 0,
                team: (i % 2) as u32,
                position: scene.deployment(i, (i % 2) as u32).unwrap(),
                surrendered: false,
                withdrawn: false,
                defending: false,
                retreat: None,
                known: BTreeMap::new(),
                search: SearchState::default(),
            },
        );
    }
    let map = scene.id;
    insert_map(&mut store, scene).unwrap();
    let id = EncounterId(size as u64 + 3);
    insert_encounter(
        &mut store,
        Encounter {
            id,
            area,
            location: Location::AtArea(area),
            map,
            round: 0,
            started: SimTime(0),
            ends: SimTime(6000),
            fighters,
            order: Vec::new(),
            cursor: 0,
            active: true,
        },
    )
    .unwrap();
    (world, store, id)
}
fn main() {
    for size in [128, 512, 2048, 4096] {
        let (world, mut store, id) = fixture(size);
        let fatigue: Vec<_> = (1..=size).map(|id| (PersonId(id as u64), 0)).collect();
        let start = Instant::now();
        for _ in 0..20 {
            world
                .prepare_begin_round(&store, id, &fatigue)
                .unwrap()
                .commit(&world, &mut store)
                .unwrap();
        }
        let round = start.elapsed().as_secs_f64() * 1e3 / 20.0;
        let start = Instant::now();
        for _ in 0..100000 {
            black_box(world.physical_contact(&store, PersonId(1), PersonId(2)));
            black_box(area_map(&store, AreaId(size as u64 + 1)));
        }
        println!(
            "fighters={size} begin_round_ms={round:.3} contact_and_scene_ns={:.1}",
            start.elapsed().as_nanos() as f64 / 100000.0
        );
    }
    let (world, mut store, id) = fixture(128);
    let alive = (1..=128).map(PersonId).collect();
    world
        .prepare_begin_round(
            &store,
            id,
            &(1..=128)
                .map(|actor| (PersonId(actor), 0))
                .collect::<Vec<_>>(),
        )
        .unwrap()
        .commit(&world, &mut store)
        .unwrap();
    let start = Instant::now();
    for actor in (1..=128).map(PersonId) {
        world
            .prepare_observation(&store, id, actor, &alive)
            .unwrap()
            .commit(&world, &mut store)
            .unwrap();
        world
            .prepare_turn(
                &store,
                id,
                None,
                Action::Wait,
                TurnFacts {
                    actor,
                    independent: true,
                    body: TargetBody {
                        alive: true,
                        health: 100,
                    },
                    force_can_act: false,
                    force_allowed: true,
                    target_body: None,
                    binding_available: false,
                    release_available: false,
                    mission_order_valid: false,
                    retreat_route_valid: false,
                    damage: 45,
                    practice_ms: 6000,
                },
            )
            .unwrap()
            .commit(&world, &mut store)
            .unwrap();
    }
    println!(
        "fighters=128 full_perception_and_wait_round_ms={:.3}",
        start.elapsed().as_secs_f64() * 1e3
    );
}

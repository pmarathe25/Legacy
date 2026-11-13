use game_mechanics::{Apparatus, Catalog, Experiment, Outcome, Part, Snapshot, World};

fn catalog() -> Catalog {
    toml::from_str(include_str!("../../../content/mechanics/basic.toml")).unwrap()
}

fn machine(id: u64) -> Apparatus {
    let base = id * 10;
    Apparatus {
        id,
        parts: std::sync::Arc::new(vec![
            Part {
                id: base + 1,
                component: 1,
                parent: None,
                share_per_mille: 1000,
                broken: false,
            },
            Part {
                id: base + 2,
                component: 2,
                parent: Some(base + 1),
                share_per_mille: 600,
                broken: false,
            },
            Part {
                id: base + 3,
                component: 3,
                parent: Some(base + 1),
                share_per_mille: 400,
                broken: false,
            },
        ]),
    }
}

fn world(count: u64) -> World {
    World::compile(Snapshot {
        catalog: catalog(),
        apparatus: (1..=count).map(machine).collect(),
        revision: 0,
        last_tick: None,
    })
    .unwrap()
}

fn request(actor: u64, apparatus: u64, output: u64) -> Experiment {
    Experiment {
        actor,
        apparatus,
        output_part: apparatus * 10 + output,
        input_force: 100,
        input_distance: 120,
        load_force: 50,
    }
}

#[test]
fn branched_transmission_never_creates_work() {
    let mut world = world(2);
    let first = request(1, 1, 2);
    let second = request(2, 2, 3);
    let draft = world.prepare(1, 7, &[first, second]).unwrap();
    let rows = draft.observations().to_vec();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0].input_force,
            rows[0].input_distance,
            rows[0].load_force
        ),
        (100, 120, 50)
    );
    assert!(rows.iter().all(|row| row.outcome == Outcome::Moved));
    let maximum: u128 = rows
        .iter()
        .map(|row| u128::from(row.available_force) * u128::from(row.travel))
        .sum();
    assert!(maximum <= u128::from(first.input_force) * u128::from(first.input_distance));
    assert!(
        rows.iter()
            .all(|row| row.delivered_work
                <= u128::from(row.available_force) * u128::from(row.travel))
    );
    assert_eq!(world.commit(draft).unwrap(), rows);
}

#[test]
fn overload_breaks_real_part_and_stale_drafts_cannot_publish() {
    let mut world = world(1);
    let opening = world.snapshot();
    let other = World::compile(opening.clone()).unwrap();
    let foreign = other.prepare(1, 3, &[request(1, 1, 2)]).unwrap();
    assert!(world.commit(foreign).is_err());
    assert_eq!(world.snapshot(), opening);
    let stale = world.prepare(1, 3, &[request(1, 1, 2)]).unwrap();
    assert!(world.validate_draft(&stale).is_ok());
    let mut overload = request(2, 1, 2);
    overload.input_force = 1001;
    let draft = world.prepare(1, 3, &[overload]).unwrap();
    assert_eq!(world.snapshot(), opening);
    let rows = world.commit(draft).unwrap();
    assert_eq!(rows[0].outcome, Outcome::Broken);
    assert_eq!(rows[0].broken_parts, vec![11]);
    assert!(!opening.apparatus[0].parts[0].broken);
    let after = world.snapshot();
    assert!(world.validate_draft(&stale).is_err());
    assert!(world.commit(stale).is_err());
    assert_eq!(world.snapshot(), after);
    let saved = serde_json::to_string(&after).unwrap();
    let resumed = World::compile(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(
        resumed
            .prepare(2, 3, &[request(1, 1, 2)])
            .unwrap()
            .observations()[0]
            .outcome,
        Outcome::Broken
    );
}

#[test]
fn contested_experiments_are_independent_of_order_and_worker_count() {
    let requests: Vec<_> = (1..=2048).map(|actor| request(actor, 1, 2)).collect();
    let run = |workers, reverse: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = world(1);
                let mut requests = requests.clone();
                if reverse {
                    requests.reverse();
                }
                let draft = world.prepare(1, 9, &requests).unwrap();
                let rows = world.commit(draft).unwrap();
                assert_eq!(
                    rows.iter()
                        .filter(|row| row.outcome == Outcome::Moved)
                        .count(),
                    1
                );
                (world.snapshot(), rows)
            })
    };
    assert_eq!(run(1, false), run(4, true));
}

#[test]
fn contention_and_invalid_requests_do_not_force_another_action() {
    let mut world = world(1);
    let before = world.snapshot();
    assert!(
        world
            .prepare(1, 5, &[request(1, 1, 2), request(1, 1, 3)])
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    let draft = world
        .prepare(1, 5, &[request(1, 1, 2), request(2, 1, 3)])
        .unwrap();
    let rows = world.commit(draft).unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row.outcome == Outcome::Contended)
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.outcome == Outcome::Moved)
            .count(),
        1
    );
    assert!(rows.windows(2).all(|rows| rows[0].actor < rows[1].actor));
}

#[test]
fn independent_experiments_match_across_worker_counts() {
    let requests: Vec<_> = (1..=2000).map(|id| request(id, id, 2)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = world(2000);
                let draft = world.prepare(1, 9, &requests).unwrap();
                let observations = world.commit(draft).unwrap();
                (world.snapshot(), observations)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn invalid_graphs_and_unrepresentable_travel_are_atomic() {
    let mut invalid = world(1).snapshot();
    std::sync::Arc::make_mut(&mut invalid.apparatus[0].parts)[1].share_per_mille = 700;
    assert!(World::compile(invalid).is_err());
    let mut state = world(1).snapshot();
    state.catalog.components[0].force_ratio_numerator = 1;
    state.catalog.components[0].force_ratio_denominator = u32::MAX;
    let world = World::compile(state).unwrap();
    let before = world.snapshot();
    let mut huge = request(1, 1, 2);
    huge.input_distance = u64::MAX;
    assert!(world.prepare(1, 9, &[huge]).is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
#[ignore = "manual independent apparatus scaling benchmark"]
fn independent_apparatus_benchmark() {
    let count: u64 = std::env::var("LEGACY_MECHANISMS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50_000);
    let requests: Vec<_> = (1..=count).map(|id| request(id, id, 2)).collect();
    let opening = world(count).snapshot();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut samples = Vec::new();
        for _ in 0..3 {
            let mut world = World::compile(opening.clone()).unwrap();
            let start = std::time::Instant::now();
            let observations = pool.install(|| {
                let draft = world.prepare(1, 9, &requests).unwrap();
                world.commit(draft).unwrap()
            });
            samples.push(start.elapsed());
            assert_eq!(observations.len(), count as usize);
            let state = (world.snapshot(), observations);
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        samples.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} apparatus",
            samples[1]
        );
    }
}

#[test]
#[ignore = "manual concentrated apparatus scaling benchmark"]
fn concentrated_apparatus_benchmark() {
    let count: u64 = std::env::var("LEGACY_MECHANISMS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(50_000);
    let requests: Vec<_> = (1..=count).map(|actor| request(actor, 1, 2)).collect();
    let opening = world(1).snapshot();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut samples = Vec::new();
        for _ in 0..3 {
            let mut world = World::compile(opening.clone()).unwrap();
            let start = std::time::Instant::now();
            let observations = pool.install(|| {
                let draft = world.prepare(1, 9, &requests).unwrap();
                world.commit(draft).unwrap()
            });
            samples.push(start.elapsed());
            assert_eq!(
                observations
                    .iter()
                    .filter(|row| row.outcome == Outcome::Moved)
                    .count(),
                1
            );
            let result = (world.snapshot(), observations);
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
        samples.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} competing experiments",
            samples[1]
        );
    }
}

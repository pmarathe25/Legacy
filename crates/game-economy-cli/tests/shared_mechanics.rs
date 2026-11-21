use game_economy::LocalEndpoint;
use game_economy::core_interval::{CoreHost, MechanicalApparatus, MechanicalExperiment};
use game_economy_player::interval::Player;
use game_lots::Place;
use game_mechanics::{Outcome, Part};
use std::path::Path;
use std::sync::Arc;

fn scene() -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let authored =
        std::fs::read_to_string(root.join("content/scenarios/economy-core-personal-design.toml"))
            .unwrap();
    let mut source = authored.replace(
        "willing_to_work = false",
        "controlled = true\nwilling_to_work = false",
    );
    source.push_str(
        r#"
[[people]]
id = 2
region = 1
health = 100
hunger = 0
controlled = true
willing_to_work = false
seek_meals = false

[[mechanics.capabilities]]
person = 2
max_force = 200
max_distance = 200

[[mechanics.apparatus]]
id = 8
parts = [{ id = 1003, component = 2, share_per_mille = 1000 }]

[[local_transport.stores]]
id = 8
region = 1
name = "Shelf"
capacity = 1

[[local_transport.stores]]
id = 9
region = 1
name = "Workshop"
capacity = 10
items = [{ id = 1003, good = "tool" }]

[local_transport.walking.elapsed]
unit_millis = 1

[[local_transport.walking.routes]]
region = 1
from = { kind = "stock" }
to = { kind = "store", id = 8 }
ticks = 120

[[local_transport.walking.routes]]
region = 1
from = { kind = "stock" }
to = { kind = "store", id = 9 }
ticks = 120

[[local_transport.walking.routes]]
region = 1
from = { kind = "store", id = 8 }
to = { kind = "store", id = 9 }
ticks = 120
"#,
    );
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "legacy-shared-mechanics-{}-{}.toml",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

#[test]
fn a_reachable_shared_machine_uses_its_real_parts_and_arbitrates_simultaneous_trials() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene();
                let apparatus = MechanicalApparatus {
                    id: 7,
                    parts: Arc::new(vec![
                        Part {
                            id: 1001,
                            component: 1,
                            parent: None,
                            share_per_mille: 1000,
                            broken: false,
                        },
                        Part {
                            id: 1002,
                            component: 2,
                            parent: Some(1001),
                            share_per_mille: 1000,
                            broken: false,
                        },
                    ]),
                };
                let trial = |actor| MechanicalExperiment {
                    actor,
                    apparatus: 7,
                    output_part: 1002,
                    input_force: 100,
                    input_distance: 100,
                    load_force: 10,
                };
                let mut builder = Player::new(&mut host, 1).unwrap();
                assert!(builder.assemble(apparatus.clone()).unwrap());
                assert!(builder.move_apparatus(7, 9, true).is_err());
                builder.walk(LocalEndpoint::Store { id: 8 }).unwrap();
                assert_eq!(host.advance_continuous(120, &[]).unwrap().failure, None);

                let mut builder = Player::new(&mut host, 1).unwrap();
                assert!(!builder.move_apparatus(7, 8, true).unwrap());
                assert_eq!(
                    builder.view().unwrap().carried_apparatus.as_slice(),
                    std::slice::from_ref(&apparatus)
                );
                builder.walk(LocalEndpoint::Store { id: 9 }).unwrap();
                assert_eq!(host.advance_continuous(120, &[]).unwrap().failure, None);
                let mut builder = Player::new(&mut host, 1).unwrap();
                assert!(builder.move_apparatus(7, 9, true).unwrap());
                assert!(builder.view().unwrap().carried_apparatus.is_empty());

                let saved = serde_json::to_vec(&host.snapshot()).unwrap();
                let mut host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                let mut visitor = Player::new(&mut host, 2).unwrap();
                assert!(visitor.view().unwrap().shared_apparatus.is_empty());
                assert!(visitor.experiment(trial(2)).is_err());
                visitor.walk(LocalEndpoint::Store { id: 9 }).unwrap();
                assert_eq!(host.advance_continuous(120, &[]).unwrap().failure, None);

                let visitor = Player::new(&mut host, 2).unwrap();
                let view = visitor.view().unwrap();
                assert!(view.possessions.is_empty());
                assert_eq!(view.shared_apparatus[0].apparatus, apparatus);
                assert_eq!(view.shared_apparatus[0].store, 9);

                let results = host
                    .experiment_mechanics(&[trial(1), trial(2)], &[])
                    .unwrap();
                assert_eq!(
                    results
                        .iter()
                        .filter(|row| row.outcome == Outcome::Moved)
                        .count(),
                    1
                );
                assert_eq!(
                    results
                        .iter()
                        .filter(|row| row.outcome == Outcome::Contended)
                        .count(),
                    1
                );
                let independent = host
                    .experiment_mechanics(
                        &[
                            trial(1),
                            MechanicalExperiment {
                                apparatus: 8,
                                output_part: 1003,
                                ..trial(2)
                            },
                        ],
                        &[],
                    )
                    .unwrap();
                assert!(independent.iter().all(|row| row.outcome == Outcome::Moved));
                let mut visitor = Player::new(&mut host, 2).unwrap();
                assert!(visitor.move_apparatus(7, 9, false).unwrap());
                assert_eq!(visitor.view().unwrap().carried_apparatus, [apparatus]);
                let state = host.snapshot();
                let holder = state
                    .core
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == Place::Person(2))
                    .unwrap();
                let mut items: Vec<_> = holder.lots.iter().filter_map(|row| row.item).collect();
                items.sort_unstable();
                assert_eq!(items, [1001, 1002]);
                (results, state)
            })
    };
    assert_eq!(run(1), run(6));
}

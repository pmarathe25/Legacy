use game_economy::core_interval::{CoreHost, CoreRunEvent};
use std::path::Path;

fn run(
    workers: usize,
    quantum: u64,
    high: bool,
    parts: &[u64],
) -> (Vec<(u128, String)>, Option<game_genetics::Genome>) {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let text = std::fs::read_to_string(
                root.join("content/scenarios/economy-named-genetic-traits.toml"),
            )
            .unwrap()
            .replace("tick_millis = 1000", &format!("tick_millis = {quantum}"));
            let text = if high {
                text
            } else {
                text.replace(
                    "{ locus = 11, alleles = [1000, 1000] }",
                    "{ locus = 11, alleles = [0, 0] }",
                )
            };
            let file = std::env::temp_dir().join(format!(
                "legacy-named-traits-{}-{quantum}-{high}.toml",
                std::process::id()
            ));
            std::fs::write(&file, text).unwrap();
            let loaded = game_economy_cli::core_scenario(&file, &root.join("content"));
            std::fs::remove_file(file).unwrap();
            let mut host = loaded.unwrap();
            let mut evidence = Vec::new();
            for &part in parts {
                let run = host.advance_continuous(part, &[]).unwrap();
                assert_eq!(run.failure, None);
                evidence.extend(
                    run.events
                        .into_iter()
                        .filter_map(|event| match event {
                            CoreRunEvent::FamilyTransitions { at_millis, events } => {
                                Some((at_millis, events))
                            }
                            _ => None,
                        })
                        .flat_map(|(at, events)| events.into_iter().map(move |row| (at, row.kind))),
                );
                host = CoreHost::resume(
                    serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap()).unwrap(),
                )
                .unwrap();
            }
            let family = host.snapshot().family.unwrap();
            let child = family.members.iter().find(|member| member.id == 3);
            if let Some(child) = child {
                let catalog = family.traits.as_ref().unwrap();
                assert_eq!(catalog.expression(&child.genome, 11), Some(750));
                assert_eq!(catalog.expression(&child.genome, 19), Some(500));
                assert_eq!(child.parents.as_slice(), &[2, 1]);
            }
            (evidence, child.map(|child| child.genome.clone()))
        })
}

#[test]
fn named_expression_influences_choices_and_gives_birth_to_inherited_variants() {
    let high = run(1, 1000, true, &[601]);
    assert_eq!(high, run(6, 173, true, &[121, 111, 369]));
    assert_eq!(high.0, [(200, "conception".into()), (600, "birth".into())]);
    assert!(high.1.is_some());
    let low = run(1, 1000, false, &[601]);
    assert!(low.0.is_empty());
    assert!(low.1.is_none());
}

use game_economy::core_interval::CoreHost;

#[test]
fn a_persons_fear_relaxes_at_elapsed_deadlines() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut scenario = std::fs::read_to_string(root.join("content/scenarios/economy-core.toml"))
        .unwrap()
        .replace("tick_millis = 3600000", "tick_millis = 1000");
    scenario.push_str(
        r#"

[opinions.rules]
minimum = -100
maximum = 100
initial = 0
fulfilled_gain = 0
default_loss = 0
fear_decay = { period_millis = 83, amount = 3 }

[[opinions.records]]
observer = 1
target = 2
trust = 4
fear = 7
obligation = 5

[[opinions.records]]
observer = 2
target = 1
trust = -2
fear = 2
obligation = 0
"#,
    );
    let path =
        std::env::temp_dir().join(format!("legacy-elapsed-fear-{}.toml", std::process::id()));
    std::fs::write(&path, scenario).unwrap();
    let load = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let opening = load.unwrap().snapshot();
    let run = |workers, advances: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                assert!(host.next_deadline(None).unwrap().fear_relaxation);
                for &advance in advances {
                    let result = host.advance_continuous(advance, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    assert_eq!(result.elapsed_millis, advance);
                    let saved = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
                }
                host.snapshot()
            })
    };
    let before = run(1, &[82]);
    assert_eq!(before.opinions.as_ref().unwrap().records[0].fear, 7);
    let whole = run(1, &[166]);
    assert_eq!(whole, run(4, &[1, 82, 33, 50]));
    let opinions = whole.opinions.unwrap();
    assert_eq!(opinions.records[0].fear, 1);
    assert_eq!(opinions.records[1].fear, 0);
    assert_eq!(opinions.records[0].trust, 4);
    assert_eq!(opinions.records[0].obligation, 5);
}

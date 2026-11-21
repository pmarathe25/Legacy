use game_economy::core_interval::CoreHost;
use game_language::Meaning;

#[test]
fn people_can_choose_words_for_needs_they_infer_from_signals() {
    let simulate = |workers, attention| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let content = root.join("content");
        let mut source =
            std::fs::read_to_string(content.join("scenarios/economy-core-language-topics.toml"))
                .unwrap();
        source = source.replace("{ object = \"grain\", enabled = true, value = 10 }", "");
        source = source.replace(
            "[[language_topic_profiles]]\nperson = 2\nidle = 0\npreferences = []",
            "[[language_topic_profiles]]\nperson = 2\nidle = 0\npreferences = [{ need = 1, enabled = true, value = 100 }, { need = 2, enabled = true, value = 20 }]",
        );
        source.push_str(&format!(
            r#"
[signals]
catalog = "signals/basic.toml"
[[signals.minds]]
person = 1
idle = 0
attention = 0
expressions = [{{ form = 1, intended = 1, recipient = 2, enabled = true, value = 10 }}]
[[signals.minds]]
person = 2
idle = 0
attention = {attention}
remember = true
capacity = 2
interpretations = [{{ form = 1, meaning = 2 }}]
"#
        ));
        let path = std::env::temp_dir().join(format!(
            "legacy-need-phonemes-{}-{workers}-{attention}.toml",
            std::process::id()
        ));
        std::fs::write(&path, source).unwrap();
        let world = game_economy_cli::core_scenario(&path, &content);
        std::fs::remove_file(path).unwrap();
        let state = world.unwrap().snapshot();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(state).unwrap();
                assert_eq!(host.advance_continuous(900_000, &[]).unwrap().failure, None);
                let checkpoint = serde_json::to_string(&host.snapshot()).unwrap();
                let mut host =
                    CoreHost::resume(serde_json::from_str(&checkpoint).unwrap()).unwrap();
                assert_eq!(host.advance_continuous(900_001, &[]).unwrap().failure, None);
                let language = host.snapshot().social.unwrap().language.unwrap();
                language
                    .minds
                    .iter()
                    .flat_map(|mind| {
                        mind.words.iter().filter_map(|word| {
                            matches!(word.meaning, Meaning::Need(_))
                                .then_some((mind.person, word.meaning))
                        })
                    })
                    .collect::<Vec<_>>()
            })
    };
    let noticed = simulate(1, 1000);
    assert_eq!(noticed, simulate(4, 1000));
    assert_eq!(noticed, [(1, Meaning::Need(2)), (2, Meaning::Need(2))]);
    assert!(simulate(4, 0).is_empty());
}

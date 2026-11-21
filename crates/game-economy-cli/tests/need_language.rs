use game_economy::core_interval::CoreHost;
use game_language::Meaning;

#[test]
fn people_can_choose_words_for_needs_they_infer_from_signals() {
    let simulate = |workers, attention, elapsed| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let content = root.join("content");
        let mut source =
            std::fs::read_to_string(content.join("scenarios/economy-core-language-topics.toml"))
                .unwrap()
                .replace("\r\n", "\n");
        if elapsed {
            source = format!("social_period_millis = 900000\n{source}");
        }
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
            "legacy-need-phonemes-{}-{workers}-{attention}-{elapsed}.toml",
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
    let noticed = simulate(1, 1000, false);
    assert_eq!(noticed, simulate(4, 1000, false));
    assert_eq!(noticed, [(1, Meaning::Need(2)), (2, Meaning::Need(2))]);
    assert!(simulate(4, 0, false).is_empty());
    assert_eq!(noticed, simulate(4, 1000, true));
    assert!(simulate(1, 0, true).is_empty());
}

#[test]
fn elapsed_encounters_survive_subdivision_and_publish_each_conversation() {
    use game_economy::core_interval::CoreRunEvent;

    let simulate = |quantum, workers, steps: &[u64]| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let content = root.join("content");
        let source = std::fs::read_to_string(
            content.join("scenarios/economy-core-language-topics.toml"),
        )
        .unwrap()
        .replace(
            "tick_millis = 900000",
            &format!(
                "tick_millis = {quantum}\nsocial_period_millis = 83\ndecision_period_millis = 166"
            ),
        );
        let path = std::env::temp_dir().join(format!(
            "legacy-elapsed-conversations-{}-{quantum}-{workers}.toml",
            std::process::id()
        ));
        std::fs::write(&path, source).unwrap();
        let world = game_economy_cli::core_scenario(&path, &content);
        std::fs::remove_file(path).unwrap();
        let snapshot = world.unwrap().snapshot();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(snapshot).unwrap();
                let mut encounters = Vec::new();
                for &step in steps {
                    let result = host.stepper(&[]).advance(step).unwrap();
                    assert_eq!(result.failure, None);
                    assert_eq!(result.elapsed_millis, step);
                    encounters.extend(result.events.into_iter().filter_map(|event| match event {
                        CoreRunEvent::ElapsedEncounters {
                            at_millis,
                            meetings,
                            words,
                            ..
                        } => Some((at_millis, meetings, words)),
                        _ => None,
                    }));
                    let checkpoint = serde_json::to_string(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_str(&checkpoint).unwrap()).unwrap();
                }
                (host.snapshot().social.unwrap(), encounters)
            })
    };
    let (social, events) = simulate(1000, 1, &[333]);
    assert_eq!(
        events.iter().map(|event| event.0).collect::<Vec<_>>(),
        [83, 166, 249, 332]
    );
    assert!(events.iter().all(|(at, meetings, words)| {
        meetings.len() == 1
            && !words.is_empty()
            && words.iter().all(|word| u128::from(word.tick) == *at)
    }));
    assert_eq!(social.elapsed.unwrap().completed, 4);
    assert_eq!(
        (social, events),
        simulate(166, 4, &[37, 46, 19, 64, 1, 165, 1])
    );
}

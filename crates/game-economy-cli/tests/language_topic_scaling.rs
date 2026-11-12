use std::fmt::Write as _;
use std::path::PathBuf;

/// Includes a physical boundary, meeting selection, personal topics, sound and learning.
#[test]
#[ignore = "host language-topic scaling measurement"]
fn hosted_personal_topics_benchmark() {
    let count: u64 = std::env::var("LEGACY_LANGUAGE_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(20_000);
    assert!(count > 1 && count.is_multiple_of(2));
    let person_topics = std::env::var_os("LEGACY_LANGUAGE_PERSON_TOPICS").is_some();
    let naming = std::env::var_os("LEGACY_LANGUAGE_NAMING_POLICY").is_some();
    assert!(!naming || person_topics);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut text = String::from(
        r#"seed = 7
tick_millis = 900000
stock_capacity = 100
food_relief_divisor = 20
[rules]
hunger_per_tick = 1
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20
[language.rules]
sounds = ["a", "k", "m"]
word_length = 2
capacity = 8
[[regions]]
id = 1
stock = {}
[dynamics.social]
[dynamics.social.rules]
interval = 1
min_health = 1
max_hunger = 100
familiarity_gain = 1
familiarity_max = 10
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\ngoods = {{ grain = 1 }}").unwrap();
        writeln!(
            text,
            "[[language.minds]]\nperson = {id}\ninitiative = 1000\nadoption = 1000"
        )
        .unwrap();
        if naming {
            let namesake = if id == 1 { 2 } else { 1 };
            writeln!(text, "words = [{{ meaning = {{ kind = \"person\", id = {namesake} }}, sounds = [0, 1, 2] }}]\nnaming = {{ idle = 0, novel_value = 5, confusion_weight = 1, namesakes = [{{ person = {namesake}, value = 100 }}] }}").unwrap();
        }
        if person_topics {
            writeln!(text, "[[language_topic_profiles]]\nperson = {id}\nidle = 0\npreferences = [{{ person = {id}, enabled = true, value = 10 }}]").unwrap();
        } else {
            writeln!(text, "[[language_topic_profiles]]\nperson = {id}\nidle = 0\npreferences = [{{ object = \"grain\", enabled = true, value = 10 }}]").unwrap();
        }
        writeln!(
            text,
            "[[dynamics.social.participants]]\nperson = {id}\nwilling = true"
        )
        .unwrap();
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-language-topics-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, text).unwrap();
    let mut initial = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        initial.advance_continuous(899_999, &[]).unwrap().failure,
        None
    );
    let snapshot = initial.snapshot();
    assert!(
        snapshot
            .social
            .as_ref()
            .unwrap()
            .language
            .as_ref()
            .unwrap()
            .events
            .is_empty()
    );
    let grain = snapshot
        .core
        .physical
        .goods
        .iter()
        .find(|good| good.name == "grain")
        .unwrap()
        .id;
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| {
                game_economy::core_interval::CoreHost::resume(snapshot.clone()).unwrap()
            });
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            assert_eq!(result.boundaries, 1);
            assert!(result.events.iter().any(|event| matches!(event, game_economy::core_interval::CoreRunEvent::Meetings(rows) if !rows.is_empty())));
            let state = pool.install(|| host.snapshot());
            let events = &state
                .social
                .as_ref()
                .unwrap()
                .language
                .as_ref()
                .unwrap()
                .events;
            assert!(!events.is_empty());
            assert!(events.iter().all(|event| event.meaning
                == if person_topics {
                    game_language::Meaning::Person(event.speaker)
                } else {
                    game_language::Meaning::Object(grain)
                }));
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} people",
            times[1]
        );
    }
}

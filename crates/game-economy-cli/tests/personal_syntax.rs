use game_economy::core_interval::{CoreHost, CoreHostSnapshot};
use game_language::{Meaning, Word, syntax::Order};
use std::path::PathBuf;

fn language(state: &CoreHostSnapshot) -> &game_language::Language {
    state.social.as_ref().unwrap().language.as_ref().unwrap()
}

fn language_mut(state: &mut CoreHostSnapshot) -> &mut game_language::Language {
    state.social.as_mut().unwrap().language.as_mut().unwrap()
}

#[test]
fn personal_grammar_emerges_from_distinct_physical_demonstrations() {
    fn run(workers: usize, inference: u16, reverse: bool, demonstrates: bool) -> serde_json::Value {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
                let mut state = game_economy_cli::core_scenario(
                    &root.join("content/scenarios/economy-core-language-topics.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                let id = |name| {
                    state
                        .core
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.name == name)
                        .unwrap()
                        .id
                };
                let (grain, paper) = (id("grain"), id("paper"));
                let talk = language_mut(&mut state);
                talk.minds[0].syntax.as_mut().unwrap().demonstrate = demonstrates;
                let listener = talk.minds[1].syntax.as_mut().unwrap();
                listener.inference = inference;
                listener.reverse_on_adoption = reverse;
                for mind in &mut talk.minds {
                    assert_eq!(mind.syntax.as_ref().unwrap().order, None);
                    mind.words = vec![
                        Word {
                            meaning: Meaning::Object(grain),
                            sounds: vec![0],
                        },
                        Word {
                            meaning: Meaning::Object(paper),
                            sounds: vec![1],
                        },
                        Word {
                            meaning: Meaning::Person(1),
                            sounds: vec![2],
                        },
                    ];
                    mind.words.sort_by_key(|word| word.meaning);
                }
                let mut host = CoreHost::resume(state).unwrap();
                assert_eq!(host.advance_continuous(900_001, &[]).unwrap().failure, None);
                let first = host.snapshot();
                let talk = language(&first);
                let first_order = talk.minds[0].syntax.as_ref().unwrap().order.unwrap();
                assert_eq!(talk.minds[1].syntax.as_ref().unwrap().order, None);
                assert_eq!(
                    talk.events[0].clause.as_ref().unwrap().intended.object,
                    grain
                );
                assert_eq!(host.advance_continuous(900_000, &[]).unwrap().failure, None);
                let repeat = host.snapshot();
                assert_eq!(
                    language(&repeat).minds[1].syntax.as_ref().unwrap().order,
                    None
                );

                // Change what the speaker wants to show. Both objects were in
                // their custody at the start, and the words were already known.
                let mut next: CoreHostSnapshot =
                    serde_json::from_slice(&serde_json::to_vec(&repeat).unwrap()).unwrap();
                language_mut(&mut next).minds[0]
                    .topic_policy
                    .as_mut()
                    .unwrap()
                    .preferences[0]
                    .meaning = Meaning::Object(paper);
                let mut host = CoreHost::resume(next).unwrap();
                assert_eq!(host.advance_continuous(900_000, &[]).unwrap().failure, None);
                let learned = host.snapshot();
                let talk = language(&learned);
                let clause = talk.events[0].clause.as_ref().unwrap();
                assert_eq!(clause.intended.object, paper);
                let expected =
                    (inference == 1000 && demonstrates).then_some(match (first_order, reverse) {
                        (Order::HolderFirst, true) => Order::ObjectFirst,
                        (Order::ObjectFirst, true) => Order::HolderFirst,
                        (order, false) => order,
                    });
                assert_eq!(clause.adopted_order, expected);
                assert_eq!(talk.minds[1].syntax.as_ref().unwrap().order, expected);
                assert_eq!(host.advance_continuous(900_000, &[]).unwrap().failure, None);
                let final_state = host.snapshot();
                let clause = language(&final_state).events[0].clause.as_ref().unwrap();
                assert_eq!(
                    clause.interpreted,
                    (expected == Some(first_order)).then_some(clause.intended)
                );
                serde_json::to_value(final_state).unwrap()
            })
    }
    for (inference, reverse, demonstrates) in [
        (0, false, true),
        (1000, false, false),
        (1000, false, true),
        (1000, true, true),
    ] {
        assert_eq!(
            run(1, inference, reverse, demonstrates),
            run(4, inference, reverse, demonstrates)
        );
    }
}

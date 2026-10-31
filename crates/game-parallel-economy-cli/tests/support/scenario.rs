use super::{load, save, scenario};
use std::path::PathBuf;

#[test]
fn generated_population_is_seeded_individual_and_worker_invariant() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-generated.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let before = world.snapshot();
                assert_eq!(before.people.len(), 129);
                assert!(before.people.iter().any(|person| person.region == Some(1)));
                assert!(before.people.iter().any(|person| person.region == Some(2)));
                assert!(before.people.iter().any(|person| !person.willing_to_work));
                assert!(before.people.iter().any(|person| person.willing_to_work));
                assert!(before.people.iter().any(|person| !person.seek_meals));
                let events = world.advance().unwrap();
                assert!(!events.started.is_empty());
                let after = world.snapshot();
                assert!(
                    after
                        .physical
                        .work
                        .iter()
                        .all(|work| before.people.iter().any(|person| {
                            person.id == work.job.worker
                                && person.willing_to_work
                                && !person.controlled
                        }))
                );
                let saved = serde_json::to_string(&after).unwrap();
                let restored =
                    game_parallel_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                        .unwrap();
                assert_eq!(after, restored.snapshot());
                after
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn seed_override_controls_both_population_and_simulation_seed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("content/parallel/economy-generated.toml");
    let load_seed = |seed| {
        super::scenario_with_seed(&path, &root.join("content"), seed)
            .unwrap()
            .snapshot()
    };
    let authored = load_seed(None);
    assert_eq!(authored, load_seed(Some(41)));
    let other = load_seed(Some(42));
    assert_eq!(other.seed, 42);
    assert_ne!(authored.people, other.people);
    assert_eq!(other, load_seed(Some(42)));
}

#[test]
fn generated_dispositions_and_individual_overrides_interpret_experiences() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-generated.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut opinions = world.snapshot().opinions.unwrap();
    assert_eq!(opinions.sensitivities.len(), 128);
    opinions.learn_harm(&[(2, 1, 1), (3, 1, 1), (66, 1, 1)]);
    opinions.learn_help(&[(2, 1), (66, 1)]);
    opinions.learn_promises(&[(2, 1), (66, 1)]);
    let index = opinions.index();
    assert_eq!(index.affinity(2, 1), 2);
    assert_eq!(index.affinity(3, 1), -5);
    assert_eq!(index.affinity(66, 1), -2);
    assert_eq!(index.obligation(2, 1), 0);
    assert_eq!(index.obligation(66, 1), 4);
}

#[test]
fn generated_founders_have_individual_lives_and_actual_births_survive_reload() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-generated.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let family = initial.family.as_ref().unwrap();
                assert_eq!(family.members.len(), 129);
                assert_eq!(family.next_id, 130);
                assert!(family.members.iter().all(|member| member.partner.is_none()));
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| !member.reproduction_allowed)
                );
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| member.reproduction_allowed)
                );
                assert!(family.members.iter().any(|member| member.age == 18));
                assert!(family.members.iter().any(|member| member.age == 25));
                for _ in 0..2 {
                    world.advance().unwrap();
                }
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_parallel_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                        .unwrap();
                for _ in 0..5 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let family = state.family.as_ref().unwrap();
                assert!(family.totals.births > 0);
                assert!(
                    family
                        .members
                        .iter()
                        .any(|member| member.id >= 130 && member.parents.len() == 2)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_population_joins_the_ruler_campaign_without_revealing_remote_people() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = super::scenario_with_options(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                    Some(42),
                    Some(&root.join("content/parallel/campaign-population.toml")),
                )
                .unwrap();
                let initial = world.snapshot();
                assert_eq!(initial.people.len(), 130);
                assert_eq!(initial.family.as_ref().unwrap().next_id, 1128);
                let work = initial
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert_eq!(work.preferences.len(), 129);
                assert!(work.askers.iter().any(|row| row.person >= 1000));
                assert!(
                    initial
                        .dreams
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|row| row.person >= 1000)
                );
                let giving = initial.giving.as_ref().unwrap();
                assert_eq!(giving.responses.len(), 129);
                assert!(
                    giving
                        .responses
                        .iter()
                        .any(|p| p.person >= 1000 && p.willing)
                );
                assert!(
                    giving
                        .responses
                        .iter()
                        .any(|p| p.person >= 1000 && !p.willing)
                );
                assert!(giving.learning.iter().any(|p| p.person >= 1000));
                assert!(
                    giving
                        .learning
                        .iter()
                        .filter(|p| p.person >= 1000)
                        .all(|p| p.experiences == 0 && p.last_helper.is_none())
                );
                let farming = &initial.agriculture.as_ref().unwrap().policies;
                let search = &initial.food_search.as_ref().unwrap().people;
                assert_eq!(farming.len(), 129);
                assert_eq!(search.len(), 129);
                assert!(farming.iter().any(|p| p.person >= 1000 && p.willing));
                assert!(farming.iter().any(|p| p.person >= 1000 && !p.willing));
                assert!(search.iter().any(|p| p.person >= 1000 && p.enabled));
                assert!(search.iter().any(|p| p.person >= 1000 && !p.enabled));
                assert!(
                    search
                        .iter()
                        .filter(|p| p.person >= 1000)
                        .all(|p| p.target.is_none())
                );
                let dynamics = initial.dynamics.as_ref().unwrap();
                let social = dynamics.social.as_ref().unwrap();
                let claims = dynamics.claims.as_ref().unwrap();
                assert_eq!(claims.preferences.len(), 130);
                assert!(claims.claims.is_empty() && claims.site_claims.is_empty());
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && p.assert_outputs)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && !p.assert_outputs)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && p.disregard_others)
                );
                assert!(
                    claims
                        .preferences
                        .iter()
                        .any(|p| p.person >= 1000 && !p.disregard_others)
                );
                assert_eq!(social.participants.len(), 130);
                assert!(
                    social
                        .participants
                        .iter()
                        .any(|p| p.person >= 1000 && p.willing)
                );
                assert!(
                    social
                        .participants
                        .iter()
                        .any(|p| p.person >= 1000 && !p.willing)
                );
                assert!(social.pressures.iter().any(|p| p.person >= 1000));
                assert!(social.relationships.is_empty());
                assert_eq!(dynamics.personality.as_ref().unwrap().people.len(), 130);
                let remote: Vec<_> = initial
                    .people
                    .iter()
                    .filter(|p| p.region == Some(2))
                    .map(|p| p.id)
                    .collect();
                assert!(!remote.is_empty());
                world.set_farming_choice(2, false).unwrap();
                {
                    let mut player = game_parallel_economy_player::Player::new(&mut world);
                    let view = player.view().unwrap();
                    assert_eq!(view.person.unwrap().id, 1);
                    assert!(view.nearby.iter().all(|id| !remote.contains(id)));
                    player.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    game_parallel_economy::CropPhase::Growing
                );
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a >= 1000 || event.b >= 1000)
                );
                let social = state.dynamics.as_ref().unwrap().social.as_ref().unwrap();
                assert!(state.physical.work.iter().all(|job| {
                    social
                        .events
                        .iter()
                        .all(|event| ![event.a, event.b].contains(&job.job.worker))
                }));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn generated_campaign(count: u64) -> game_parallel_economy::Economy {
    static NEXT_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = std::fs::read_to_string(root.join("content/parallel/campaign-population.toml"))
        .unwrap()
        .replace("count = 128", &format!("count = {count}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-campaign-population-{}-{count}-{}.toml",
        std::process::id(),
        NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, profile).unwrap();
    let world = super::scenario_with_options(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
        Some(42),
        Some(&path),
    );
    std::fs::remove_file(path).unwrap();
    world.unwrap()
}

#[test]
fn generated_campaign_replays_at_scale_across_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = generated_campaign(2048);
                for _ in 0..7 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                assert!(state.family.as_ref().unwrap().totals.births > 0);
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some()
                        && state
                            .agriculture
                            .as_ref()
                            .unwrap()
                            .policies
                            .iter()
                            .any(|policy| policy.person == member.id)
                }));
                let claims = state.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some()
                        && claims.preferences.iter().any(|p| p.person == member.id)
                }));
                let giving = state.giving.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && giving.responses.iter().any(|p| p.person == member.id)
                }));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_unwilling_person_can_respond_to_fear_without_a_felt_obligation() {
    let mut initial = generated_campaign(128).snapshot();
    let dynamics = initial.dynamics.as_mut().unwrap();
    let social = dynamics.social.as_ref().unwrap();
    let target = social
        .participants
        .iter()
        .find(|p| {
            p.person >= 1000
                && !p.willing
                && social.pressures.iter().any(|rule| rule.person == p.person)
        })
        .unwrap()
        .person;
    let region = initial
        .people
        .iter()
        .find(|p| p.id == target)
        .unwrap()
        .region;
    let requester = social
        .participants
        .iter()
        .find(|p| {
            p.person >= 1000
                && p.willing
                && p.person != target
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == p.person && person.region == region)
        })
        .unwrap()
        .person;
    dynamics
        .personality
        .as_mut()
        .unwrap()
        .people
        .iter_mut()
        .find(|p| p.person == requester)
        .unwrap()
        .sociability = 1000;
    for person in &mut initial.people {
        person.willing_to_work = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: target,
            target: requester,
            trust: 0,
            fear: 10,
            obligation: 0,
            affinity: 0,
        });
    let mut world = game_parallel_economy::Economy::compile(initial).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let social = state.dynamics.as_ref().unwrap().social.as_ref().unwrap();
    assert!(social.events.iter().any(|event| event.pressured == Some(target)
        && [event.a, event.b].contains(&requester)), "target={target} requester={requester} events={:?}", social.events);
    assert_eq!(
        state
            .opinions
            .as_ref()
            .unwrap()
            .index()
            .obligation(target, requester),
        0
    );
}

#[test]
fn generated_person_asserts_a_claim_only_after_their_real_output_arrives() {
    let mut initial = generated_campaign(128).snapshot();
    let claims = initial.dynamics.as_ref().unwrap().claims.as_ref().unwrap();
    let worker = initial
        .people
        .iter()
        .find(|person| {
            person.id >= 1000
                && person.region == Some(1)
                && person.willing_to_work
                && claims
                    .preferences
                    .iter()
                    .any(|p| p.person == person.id && p.assert_outputs)
        })
        .unwrap()
        .id;
    for person in &mut initial.people {
        person.willing_to_work = person.id == worker;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .dynamics
        .as_mut()
        .unwrap()
        .personality
        .as_mut()
        .unwrap()
        .people
        .iter_mut()
        .find(|p| p.person == worker)
        .unwrap()
        .diligence = 1000;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                assert!(
                    world
                        .snapshot()
                        .dynamics
                        .unwrap()
                        .claims
                        .unwrap()
                        .claims
                        .is_empty()
                );
                for _ in 0..5 {
                    world.advance().unwrap();
                }
                let state = world.snapshot();
                let flour = state
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "flour")
                    .unwrap()
                    .id;
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|container| {
                        container.place
                            == game_parallel_lots::Place::Stock {
                                region: 1,
                                good: flour,
                            }
                    })
                    .unwrap();
                assert!(stock.used > 0);
                assert!(
                    state
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .claims
                        .as_ref()
                        .unwrap()
                        .claims
                        .iter()
                        .any(|claim| claim.person == worker
                            && claim.region == 1
                            && claim.good == flour)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_can_give_real_food_from_fear_without_feeling_obligated() {
    let mut initial = generated_campaign(128).snapshot();
    let giving = initial.giving.as_ref().unwrap();
    let target = giving
        .responses
        .iter()
        .find(|response| {
            response.person >= 1000
                && !response.willing
                && response.pressure_threshold == Some(3)
                && initial
                    .people
                    .iter()
                    .any(|p| p.id == response.person && p.region == Some(1))
        })
        .unwrap()
        .person;
    let person = initial.people.iter_mut().find(|p| p.id == target).unwrap();
    person.willing_to_work = false;
    person.seek_meals = false;
    let requester = initial
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_parallel_lots::Place::Person(1))
        .unwrap();
    *requester =
        game_parallel_lots::Container::new(requester.place, requester.capacity, vec![]).unwrap();
    initial
        .agriculture
        .as_mut()
        .unwrap()
        .policies
        .iter_mut()
        .find(|p| p.person == target)
        .unwrap()
        .willing = false;
    initial
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: target,
            target: 1,
            trust: 0,
            fear: 10,
            obligation: 0,
            affinity: 0,
        });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                let bread = before
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                let amount = |state: &game_parallel_economy::State, person| -> u64 {
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == game_parallel_lots::Place::Person(person))
                        .unwrap()
                        .lots
                        .iter()
                        .filter(|lot| lot.good == bread)
                        .map(|lot| lot.quantity)
                        .sum()
                };
                let before_sender = amount(&before, target);
                let before_recipient = amount(&before, 1);
                world.ask_food(1, target, 4).unwrap();
                world.advance().unwrap();
                let after = world.snapshot();
                assert_eq!(amount(&after, target), before_sender - 1);
                assert_eq!(amount(&after, 1), before_recipient + 1);
                assert_eq!(
                    after
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .obligation(target, 1),
                    0
                );
                after
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_can_refuse_requested_work_or_do_it_from_fear() {
    let mut initial = generated_campaign(128).snapshot();
    let requests = initial
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap();
    let worker = requests
        .preferences
        .iter()
        .find(|preference| {
            preference.person >= 1000
                && preference.help_value < preference.effort
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == preference.person && person.region == Some(1))
        })
        .unwrap()
        .person;
    for person in &mut initial.people {
        person.willing_to_work = false;
        person.seek_meals = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    initial
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .clear();
    initial.goals = None;
    initial.dreams = None;
    let run = |workers, fear| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                state
                    .opinions
                    .as_mut()
                    .unwrap()
                    .records
                    .push(game_parallel_economy::Opinion {
                        observer: worker,
                        target: 1,
                        trust: 0,
                        fear,
                        obligation: 0,
                        affinity: 0,
                    });
                let recipe = state
                    .physical
                    .recipes
                    .iter()
                    .find(|recipe| recipe.name == "mill-grain")
                    .unwrap()
                    .id;
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                let request = world.ask_work(1, worker, recipe, 4).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let event = state
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap()
                    .events
                    .iter()
                    .find(|event| event.request == request)
                    .unwrap();
                if fear == 0 {
                    assert!(matches!(
                        event.outcome,
                        game_parallel_autowork::requests::Outcome::Refused
                    ));
                    assert!(
                        state
                            .physical
                            .work
                            .iter()
                            .all(|work| work.job.worker != worker)
                    );
                } else {
                    assert!(matches!(
                        event.outcome,
                        game_parallel_autowork::requests::Outcome::Started { .. }
                    ));
                    assert!(
                        state
                            .physical
                            .work
                            .iter()
                            .any(|work| work.job.worker == worker)
                    );
                }
                assert_eq!(
                    state
                        .opinions
                        .as_ref()
                        .unwrap()
                        .index()
                        .obligation(worker, 1),
                    0
                );
                state
            })
    };
    assert_eq!(run(1, 0), run(4, 0));
    assert_eq!(run(1, 10), run(4, 10));
}

#[test]
fn generated_aspiration_can_lead_to_an_actual_work_request() {
    let mut initial = generated_campaign(128).snapshot();
    let requests = initial
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap();
    let asker = requests
        .askers
        .iter()
        .find(|asker| {
            asker.person >= 1000
                && initial
                    .dreams
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .any(|dream| dream.person == asker.person)
        })
        .unwrap()
        .person;
    initial
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .retain(|row| row.person == asker);
    initial
        .dreams
        .as_mut()
        .unwrap()
        .people
        .retain(|row| row.person == asker);
    for person in &mut initial.people {
        person.willing_to_work = false;
        person.seek_meals = false;
    }
    for policy in &mut initial.agriculture.as_mut().unwrap().policies {
        policy.willing = false;
    }
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let mut observed = false;
                for _ in 0..8 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    observed |= state
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .next_id
                        > 1;
                }
                assert!(observed);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_person_learns_help_only_from_an_actual_food_transfer() {
    let initial = generated_campaign(128).snapshot();
    let target = initial
        .giving
        .as_ref()
        .unwrap()
        .learning
        .iter()
        .find(|p| {
            p.person >= 1000
                && initial
                    .people
                    .iter()
                    .any(|person| person.id == p.person && person.region == Some(1))
        })
        .unwrap()
        .person;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let bread = world
                    .snapshot()
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                assert!(world.give(1, target, bread, 1).unwrap());
                let learned = world.sharing_learning(target).unwrap();
                assert_eq!((learned.experiences, learned.last_helper), (1, Some(1)));
                assert!(!world.give(1, target, bread, 999).unwrap());
                let learned = world.sharing_learning(target).unwrap();
                assert_eq!((learned.experiences, learned.last_helper), (1, Some(1)));
                assert!(world.give(1, target, bread, 1).unwrap());
                assert_eq!(world.sharing_learning(target).unwrap().experiences, 2);
                let mut state = world.snapshot();
                let person = state.people.iter_mut().find(|p| p.id == target).unwrap();
                person.hunger = 15;
                person.seek_meals = false;
                person.willing_to_work = false;
                state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .policies
                    .iter_mut()
                    .find(|p| p.person == target)
                    .unwrap()
                    .willing = false;
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|c| c.place == game_parallel_lots::Place::Person(target))
                    .unwrap();
                *holder = game_parallel_lots::Container::new(holder.place, holder.capacity, vec![])
                    .unwrap();
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                assert!(
                    state
                        .giving
                        .as_ref()
                        .unwrap()
                        .requests
                        .iter()
                        .any(|request| request.requester == target && request.target == 1)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn generated_hungry_person_can_ask_a_previously_met_acquaintance() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = generated_campaign(128);
                world.advance().unwrap();
                let mut state = world.snapshot();
                let relationships = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .social
                    .as_ref()
                    .unwrap()
                    .relationships;
                let preferences = &state.giving.as_ref().unwrap().acquaintance_asking;
                let actor = relationships
                    .iter()
                    .flat_map(|r| [r.a, r.b])
                    .find(|id| {
                        *id >= 1000 && preferences.iter().any(|p| p.person == *id && p.enabled)
                    })
                    .unwrap();
                let known: Vec<_> = relationships
                    .iter()
                    .filter_map(|r| {
                        if r.a == actor {
                            Some(r.b)
                        } else if r.b == actor {
                            Some(r.a)
                        } else {
                            None
                        }
                    })
                    .collect();
                let person = state.people.iter_mut().find(|p| p.id == actor).unwrap();
                person.hunger = 15;
                person.seek_meals = false;
                person.willing_to_work = false;
                state
                    .agriculture
                    .as_mut()
                    .unwrap()
                    .policies
                    .iter_mut()
                    .find(|p| p.person == actor)
                    .unwrap()
                    .willing = false;
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|c| c.place == game_parallel_lots::Place::Person(actor))
                    .unwrap();
                *holder = game_parallel_lots::Container::new(holder.place, holder.capacity, vec![])
                    .unwrap();
                let mut world = game_parallel_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let request = state
                    .giving
                    .as_ref()
                    .unwrap()
                    .requests
                    .iter()
                    .find(|r| r.requester == actor)
                    .unwrap();
                assert!(known.contains(&request.target));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

fn hungry_generated_campaign(count: u64) -> game_parallel_economy::State {
    let mut world = generated_campaign(count);
    world.advance().unwrap();
    let mut state = world.snapshot();
    for person in &mut state.people {
        if person.id >= 1000 {
            person.hunger = 15;
            person.seek_meals = false;
            person.willing_to_work = false;
        }
    }
    for policy in &mut state.agriculture.as_mut().unwrap().policies {
        if policy.person >= 1000 {
            policy.willing = false;
        }
    }
    for holder in &mut state.physical.custody.containers {
        if matches!(holder.place, game_parallel_lots::Place::Person(id) if id >= 1000) {
            *holder =
                game_parallel_lots::Container::new(holder.place, holder.capacity, vec![]).unwrap();
        }
    }
    state
}

#[test]
fn many_hungry_people_ask_known_acquaintances_deterministically() {
    let initial = hungry_generated_campaign(2048);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let requests = &state.giving.as_ref().unwrap().requests;
                assert!(requests.len() > 100);
                let mut actors: Vec<_> = requests.iter().map(|request| request.requester).collect();
                actors.sort_unstable();
                actors.dedup();
                assert_eq!(actors.len(), requests.len());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
#[ignore = "performance measurement"]
fn generated_acquaintance_requests_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000u64);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let initial = hungry_generated_campaign(count);
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = game_parallel_economy::Economy::compile(initial.clone()).unwrap();
                let start = std::time::Instant::now();
                world.advance().unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert!(world.snapshot().giving.unwrap().requests.len() > (count / 20) as usize);
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} generated={count} hungry_tick_seconds={:.6}",
                samples[1]
            );
        });
}

#[test]
#[ignore = "performance measurement"]
fn generated_campaign_benchmark() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000u64);
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = generated_campaign(count);
                let start = std::time::Instant::now();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let elapsed = start.elapsed().as_secs_f64();
                let snapshot = world.snapshot();
                assert!(
                    snapshot
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .social
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.a >= 1000 || event.b >= 1000)
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} generated={count} ticks=3 elapsed={:.6}",
                samples[1]
            );
        });
}

#[test]
fn authored_scenario_advances_and_resumes_identically_across_worker_counts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let first = world.advance().unwrap();
                assert_eq!(first.started.len(), 1);
                assert_eq!(world.snapshot().goals.as_ref().unwrap().people.len(), 1);
                let state = world.snapshot();
                let field = &state.agriculture.as_ref().unwrap().fields[0];
                assert_eq!(field.phase, game_parallel_economy::CropPhase::Growing);
                assert_eq!(field.remaining, 24);
                let seed = state
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "grain-seed")
                    .unwrap()
                    .id;
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| {
                        row.place
                            == game_parallel_lots::Place::Stock {
                                region: 1,
                                good: seed,
                            }
                    })
                    .unwrap();
                assert_eq!(stock.used, 20);
                let path = std::env::temp_dir().join(format!(
                    "legacy-economy-{}-{workers}.json",
                    std::process::id()
                ));
                save(&world, &path).unwrap();
                let mut restored = load(&path).unwrap();
                std::fs::remove_file(path).unwrap();
                for _ in 0..4 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_campaign_starts_a_controlled_ruler_and_a_real_family() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let state = world.snapshot();
                assert!(state.people.iter().find(|p| p.id == 1).unwrap().controlled);
                assert_eq!(state.campaign.as_ref().unwrap().lineage.current, Some(1));
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let saved = world.snapshot();
                assert_eq!(saved.family.as_ref().unwrap().totals.births, 1);
                assert!(
                    saved
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3 && p.enabled)
                );
                assert!(
                    saved
                        .gathering
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .any(|p| p.person == 3 && p.pressured_by.is_none())
                );
                let newborn_purpose = saved
                    .purpose
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|p| p.person == 3)
                    .unwrap();
                assert_eq!(newborn_purpose.deprivation, 0);
                assert!(newborn_purpose.eligible_at > saved.physical.custody.tick);
                assert!(
                    saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .personality
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3)
                );
                assert!(
                    saved
                        .knowledge
                        .as_ref()
                        .unwrap()
                        .minds
                        .iter()
                        .any(|mind| mind.person == 3)
                );
                assert!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == game_parallel_lots::Place::Person(3))
                );
                game_parallel_economy::Economy::compile(saved.clone()).unwrap();
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ruler_view_excludes_remote_people_stock_and_other_minds() {
    use game_parallel_economy_knowledge::Record;
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut baseline = world.snapshot();
    baseline
        .people
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .region = Some(2);
    let mut hidden = baseline.clone();
    hidden.giving.as_mut().unwrap().learning[0].experiences = 2;
    hidden
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 8,
        });
    hidden
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|m| m.person == 2)
        .unwrap()
        .records
        .push(Record {
            region: 2,
            food: 999,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        });
    let stock = hidden
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| matches!(c.place, Place::Stock { region: 2, .. }))
        .unwrap();
    let good = match stock.place {
        Place::Stock { good, .. } => good,
        _ => unreachable!(),
    };
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good,
            quantity: 7,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut first = game_parallel_economy::Economy::compile(baseline).unwrap();
    let mut second = game_parallel_economy::Economy::compile(hidden).unwrap();
    let mut player = Player::new(&mut first);
    assert_eq!(
        player.view().unwrap(),
        Player::new(&mut second).view().unwrap()
    );
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
    assert!(player.view().unwrap().nearby.is_empty());
    let conditions = player.view().unwrap().conditions.unwrap();
    assert_eq!(conditions.season.unwrap().name, "growing season");
    assert_eq!(conditions.local_hazard.unwrap().region, 1);
    assert!(player.past_knowledge(2).is_err());
    assert!(player.meet(2).is_err());
    assert_eq!(player.view().unwrap().tick, 0);
}

#[test]
fn suspended_control_hides_the_rulers_view_and_rejects_actions() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .campaign
        .as_mut()
        .unwrap()
        .lineage
        .invocation
        .as_mut()
        .unwrap()
        .control_until = Some(2);
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert!(view.person.is_none());
    assert!(
        view.nearby.is_empty()
            && view.possessions.is_empty()
            && view.local_stock.is_empty()
            && view.food_reports.is_empty()
    );
    assert!(player.eat(31).is_err());
    assert!(player.start_expedition(2, 1).is_err());
    player.advance().unwrap();
    player.advance().unwrap();
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
}

#[test]
fn player_can_make_a_promise_and_later_choose_real_delivery() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let good = player.view().unwrap().possessions[0].good;
    let id = player.promise(2, good, 1, 8).unwrap();
    player.advance().unwrap();
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Active
    );
    assert!(player.fulfill_promise(id).unwrap());
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Fulfilled
    );
}

#[test]
fn player_fights_on_the_authored_generated_map() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let id = player.attack(2).unwrap();
    assert_eq!(player.view().unwrap().battle.unwrap().encounter, id);
    assert!(player.rest().is_err());
    player.advance().unwrap();
    assert!(player.view().unwrap().person.unwrap().health < 100);
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    assert!(player.view().unwrap().battle.is_none());
}

#[test]
fn player_request_can_be_refused_or_followed_from_personal_fear() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = world.snapshot();
    for member in &mut initial.family.as_mut().unwrap().members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    let mut world = game_parallel_economy::Economy::compile(initial).unwrap();
    let mut player = Player::new(&mut world);
    let recipe = player
        .catalog()
        .unwrap()
        .1
        .into_iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    let first = player.ask_work(2, recipe, 15).unwrap();
    assert_eq!(player.view().unwrap().pending_work_requests[0].id, first);
    for _ in 0..8 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1)
        {
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(
        view.work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1),
        "{view:?}"
    );
    player.attack(2).unwrap();
    player.advance().unwrap();
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    let second = player.ask_work(2, recipe, 15).unwrap();
    let mut accepted = false;
    for _ in 0..10 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .observed_work
            .iter()
            .any(|work| work.request == second && work.worker == 2)
        {
            accepted = true;
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(accepted, "{view:?}");
}

#[test]
fn player_treats_infection_with_authored_medicine_taken_from_stock() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let (medicine, quantity) = player.view().unwrap().infection.unwrap().medicine.unwrap();
    assert!(player.transfer_stock(medicine, quantity, true).unwrap());
    assert!(player.treat().unwrap());
    assert!(player.view().unwrap().infection.is_none());
}

#[test]
fn player_can_treat_a_nearby_person_with_carried_medicine() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 2,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let medicine = state
        .physical
        .goods
        .iter()
        .find(|g| g.name == "medicine")
        .unwrap()
        .id;
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(player.transfer_stock(medicine, 1, true).unwrap());
    assert!(player.treat_other(2, medicine, 1).unwrap());
    assert!(
        !world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
}

#[test]
fn player_can_cultivate_a_visible_field() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let field = player.view().unwrap().fields[0].id;
    assert!(player.farm(field).unwrap());
    assert_eq!(
        player.view().unwrap().fields[0].phase,
        game_parallel_economy::CropPhase::Growing
    );
}

#[test]
fn player_rest_advances_time_and_recovers_personal_fatigue() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .fatigue
        .as_mut()
        .unwrap()
        .people
        .push(game_parallel_fatigue::Person {
            id: 1,
            fatigue: 10,
            resting: false,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    player.rest().unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.fatigue, Some((6, true)));
}

#[test]
fn player_observes_and_recovers_local_deceased_possessions() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.people.iter_mut().find(|p| p.id == 2).unwrap().health = 0;
    state
        .family
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .alive = false;
    let good = state.physical.goods[0].id;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        Place::Person(2),
        holder.capacity,
        vec![Lot {
            good,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(
        player
            .view()
            .unwrap()
            .bodies
            .iter()
            .any(|body| body.person == 2 && body.possessions.iter().any(|lot| lot.good == good))
    );
    assert!(player.recover_possessions(2, good, 1).unwrap());
    assert!(
        player
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == good)
    );
}

#[test]
fn player_gathers_from_an_authored_local_resource_site() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let site = player
        .view()
        .unwrap()
        .resource_sites
        .into_iter()
        .find(|site| site.id == 1)
        .unwrap();
    assert_eq!(site.available, 20);
    assert!(player.gather(1).unwrap());
    assert_eq!(
        player
            .view()
            .unwrap()
            .resource_sites
            .into_iter()
            .find(|site| site.id == 1)
            .unwrap()
            .available,
        18
    );
    assert!(player.gather(2).is_err());
}

#[test]
fn player_sees_only_their_own_actual_journey() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.depart(2).unwrap();
    let view = player.view().unwrap();
    let journey = view.journey.unwrap();
    assert_eq!(
        (
            journey.person,
            journey.source,
            journey.destination,
            journey.remaining
        ),
        (1, 1, 2, 3)
    );
    assert!(view.nearby.is_empty());
    assert!(view.resource_sites.is_empty());
}

#[test]
fn player_can_give_carried_goods_without_creating_a_promise() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let before = player.view().unwrap();
    let good = before.possessions[0].good;
    assert!(player.give(2, good, 1).unwrap());
    let after = player.view().unwrap();
    assert_eq!(
        after.possessions[0].quantity,
        before.possessions[0].quantity - 1
    );
    assert!(after.promises.is_empty());
}

#[test]
fn player_can_ask_a_local_person_for_food_and_see_the_request() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.ask_food(2, 4).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.food_requests.len(), 1);
    assert_eq!(
        (
            view.food_requests[0].requester,
            view.food_requests[0].target
        ),
        (1, 2)
    );
    assert!(player.ask_food(99, 4).is_err());
    assert_eq!(player.view().unwrap().tick, 1);
}

#[test]
fn player_can_request_a_personal_hauling_effort() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 8).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.hauling_requests.len(), 1);
    assert_eq!(
        (
            view.hauling_requests[0].requester,
            view.hauling_requests[0].carrier
        ),
        (1, 2)
    );
    assert_eq!(view.tick, 1);
}

#[test]
fn player_request_can_lead_to_a_real_cross_region_delivery() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = original.snapshot();
    state.autowork = None;
    state.goals = None;
    state.dreams = None;
    state.agriculture = None;
    state.gathering = None;
    state.people[1].seek_meals = false;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 20).unwrap();
    for _ in 0..15 {
        player.advance().unwrap();
    }
    let view = player.view().unwrap();
    assert!(
        view.local_stock
            .iter()
            .any(|lot| lot.good == bread && lot.quantity >= 2)
    );
}

#[test]
fn player_can_see_and_give_to_a_companion_on_the_same_journey() {
    use game_parallel_economy::Departure;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.nearby, vec![2]);
    assert!(player.give(2, view.possessions[0].good, 1).unwrap());
    assert_eq!(player.view().unwrap().journey.unwrap().remaining, 2);
}

#[test]
fn scenario_shelter_goods_resolve_from_content_names() {
    use crate::{EnvironmentConfig, resolve_environment};
    use std::collections::BTreeMap;
    let parse = |good: &str| {
        toml::from_str::<EnvironmentConfig>(&format!("regions = [{{ region = 1, damage = 2, shelter = 0, shelter_reduction = 1, shelter_good = \"{good}\" }}]")).unwrap()
    };
    let goods = BTreeMap::from([("reed-shelter-kit", 7)]);
    assert_eq!(
        resolve_environment(parse("reed-shelter-kit"), &goods)
            .unwrap()
            .regions[0]
            .shelter_good,
        Some(7)
    );
    assert!(resolve_environment(parse("missing"), &goods).is_err());
}

#[test]
fn authored_personal_promise_offers_resolve_named_goods() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let offer = &state.agreements.as_ref().unwrap().offering[0];
    let good = state
        .physical
        .goods
        .iter()
        .find(|good| good.id == offer.good)
        .unwrap();
    assert_eq!(
        (offer.person, offer.recipient, good.name.as_str()),
        (2, 1, "bread")
    );
    assert!(!offer.willing);
}

#[test]
fn authored_personal_offers_resolve_observed_work_by_recipe_name() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = source.replacen(
        "[[promise_offering]]",
        "[[promise_offering]]\nafter_work_recipe = \"fixture-tool\"",
        1,
    );
    let path =
        std::env::temp_dir().join(format!("legacy-observed-work-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let id = state.agreements.unwrap().offering[0]
        .after_work_recipe
        .unwrap();
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .name,
        "fixture-tool"
    );
}

#[test]
fn authored_work_tools_resolve_recipe_and_good_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = format!(
        "{}\n[[work_tools]]\nrecipe = \"fixture-tool\"\ngood = \"tool\"\nquantity = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!("legacy-work-tools-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let tool = &state.work_tools[0];
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == tool.recipe)
            .unwrap()
            .name,
        "fixture-tool"
    );
    assert_eq!(
        state
            .physical
            .goods
            .iter()
            .find(|g| g.id == tool.good)
            .unwrap()
            .name,
        "tool"
    );
}

#[test]
fn authored_work_sites_resolve_recipes_and_preserve_kind_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\n[[work_sites.sites]]\nid = 7\nregion = 1\nkind = \"forge\"\ncapacity = 2\n[[work_sites.requirements]]\nrecipe = \"fixture-tool\"\nkind = \"forge\"\nslots = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!("legacy-work-sites-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let sites = state.work_sites.unwrap();
    assert_eq!(sites.sites[0].kind, "forge");
    assert_eq!(sites.requirements[0].kind, "forge");
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == sites.requirements[0].recipe)
            .unwrap()
            .name,
        "fixture-tool"
    );
}

#[test]
fn ruler_observes_only_local_physical_work_sites() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\n[[work_sites.sites]]\nid = 7\nregion = 1\nkind = \"forge\"\ncapacity = 2\n[[work_sites.sites]]\nid = 8\nregion = 2\nkind = \"mill\"\ncapacity = 1\n[[work_sites.requirements]]\nrecipe = \"fixture-tool\"\nkind = \"forge\"\nslots = 1\n",
        source
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-work-sites-view-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let player = game_parallel_economy_player::Player::new(&mut world);
    let (_, requirements) = player.work_requirements().unwrap();
    assert_eq!(requirements[0].kind, "forge");
    let view = player.view().unwrap();
    assert_eq!(view.work_sites.len(), 1);
    assert_eq!(
        (view.work_sites[0].site.id, view.work_sites[0].occupied),
        (7, 0)
    );
}

#[test]
fn ruler_can_build_an_authored_site_from_an_actual_kit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\nsites = []\nrequirements = []\n[construction]\n[[construction.blueprints]]\nid = 1\nname = \"shelter workshop\"\nkind = \"workshop\"\ncapacity = 1\nkit = \"reed-shelter-kit\"\nquantity = 1\n[[construction.demolition]]\nperson = 2\nsite = 10\nwilling = false\npressured_by = 1\npressure_threshold = 3\n[[construction.goal_builders]]\nperson = 2\nenabled = false\n",
        source
    );
    let path =
        std::env::temp_dir().join(format!("legacy-construction-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let preference = &state.construction.as_ref().unwrap().demolition[0];
    assert_eq!(
        (preference.person, preference.site, preference.pressured_by),
        (2, 10, Some(1))
    );
    assert_eq!(
        state.construction.as_ref().unwrap().goal_builders[0].person,
        2
    );
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    assert_eq!(
        player.building_blueprints().unwrap()[0].name,
        "shelter workshop"
    );
    assert_eq!(player.view().unwrap().claim_built_sites, Some(false));
    player.set_built_site_claim_choice(true).unwrap();
    assert!(player.build_site(1).unwrap());
    let view = player.view().unwrap();
    assert_eq!(view.work_sites.len(), 1);
    assert_eq!(view.work_sites[0].site.kind, "workshop");
    let site = view.work_sites[0].site.id;
    assert_eq!(player.view().unwrap().site_claims, vec![site]);
    assert!(player.claim_site(site).is_err());
    assert!(player.destroy_site(site).unwrap());
    assert_eq!(player.view().unwrap().site_claims, vec![site]);
    player.withdraw_site_claim(site).unwrap();
    assert!(player.view().unwrap().site_claims.is_empty());
}

#[test]
fn ruler_can_assert_and_withdraw_personal_claims_without_seeing_other_minds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let good = world
        .snapshot()
        .physical
        .goods
        .iter()
        .find(|g| g.name == "grain")
        .unwrap()
        .id;
    world.claim_stock(2, 1, good).unwrap();
    {
        let mut player = game_parallel_economy_player::Player::new(&mut world);
        let before = player.view().unwrap();
        assert!(before.claims.is_empty());
        player.claim_stock(good).unwrap();
        let after = player.view().unwrap();
        assert_eq!(after.tick, before.tick);
        assert_eq!(
            after.claims,
            vec![game_parallel_economy_player::ClaimView { region: 1, good }]
        );
        assert!(player.claim_stock(good).is_err());
    }
    let path =
        std::env::temp_dir().join(format!("legacy-personal-claim-{}.json", std::process::id()));
    save(&world, &path).unwrap();
    let mut restored = load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut player = game_parallel_economy_player::Player::new(&mut restored);
    assert_eq!(player.view().unwrap().claims.len(), 1);
    player.withdraw_claim(1, good).unwrap();
    assert!(player.view().unwrap().claims.is_empty());
    assert!(player.withdraw_claim(1, good).is_err());
    let state = restored.snapshot();
    let claims = &state
        .dynamics
        .as_ref()
        .unwrap()
        .claims
        .as_ref()
        .unwrap()
        .claims;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].person, 2);
}

#[test]
fn authored_site_claims_remain_personal_and_private_to_the_ruler() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/parallel/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[work_sites]\nsites = [{{ id = 7, region = 1, kind = \"workshop\", capacity = 1 }}]\nrequirements = []\n[[dynamics.claims.site_claims]]\nperson = 2\nsite = 7\n[[opinions.sensitivities]]\nperson = 1\naffinity_site_use = 3\naffinity_site_destruction = -2\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nsite_use = -4\n"
    );
    let path = std::env::temp_dir().join(format!("legacy-site-claims-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let snapshot = world.snapshot();
    let sensitivity = snapshot
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|s| s.person == 1)
        .unwrap();
    assert_eq!(
        (
            sensitivity.affinity_site_use,
            sensitivity.affinity_site_destruction
        ),
        (Some(3), Some(-2))
    );
    assert_eq!(sensitivity.relationship_cases[0].site_use, Some(-4));
    let mut player = game_parallel_economy_player::Player::new(&mut world);
    assert!(player.view().unwrap().site_claims.is_empty());
    player.claim_site(7).unwrap();
    assert_eq!(player.view().unwrap().site_claims, vec![7]);
    player.withdraw_site_claim(7).unwrap();
    assert!(player.view().unwrap().site_claims.is_empty());
    let snapshot = world.snapshot();
    assert_eq!(
        snapshot
            .dynamics
            .as_ref()
            .unwrap()
            .claims
            .as_ref()
            .unwrap()
            .site_claims[0]
            .person,
        2
    );
}

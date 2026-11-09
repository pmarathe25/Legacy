use super::{load, save, scenario};
use std::path::PathBuf;

#[test]
fn player_boundary_advancement_yields_only_times() {
    use std::ops::ControlFlow;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-local-walking.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let mut observed = Vec::new();
    let progress = player.advance_boundaries(3, |time| {
        observed.push(time);
        if observed.len() == 2 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(progress.stop, game_economy::BoundaryStop::Yielded);
    assert_eq!(progress.completed, 2);
    assert_eq!(
        observed,
        vec![
            game_economy::BoundaryTime {
                tick: 1,
                elapsed_millis: u128::from(progress.end.quantum_millis),
                quantum_millis: progress.end.quantum_millis
            },
            progress.end
        ]
    );
}

#[test]
fn local_walking_limits_store_observation_and_physical_pickup() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-local-walking.toml"),
        &root.join("content"),
    )
    .unwrap();
    world.set_local_carrying_choice(2, false).unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let view = player.view().unwrap();
    assert!(view.local_stock_observed);
    assert!(!view.local_stores[0].observed);
    assert!(view.local_stores[0].lots.is_empty());
    assert_eq!(
        player.walk_local(Some(1)).unwrap(),
        game_economy::LocalWalkOutcome::Progressed
    );
    assert!(!player.view().unwrap().local_stock_observed);
    assert_eq!(
        player.walk_local(Some(1)).unwrap(),
        game_economy::LocalWalkOutcome::Arrived
    );
    let view = player.view().unwrap();
    assert!(view.local_stores[0].observed);
    assert!(!view.local_stores[1].observed);
    let good = view.local_stores[0].lots[0].good;
    assert!(player.move_local_store(1, good, 1, false).unwrap());
    assert_eq!(
        player.walk_local(None).unwrap(),
        game_economy::LocalWalkOutcome::Progressed
    );
    assert_eq!(
        player.walk_local(None).unwrap(),
        game_economy::LocalWalkOutcome::Arrived
    );
    assert!(player.transfer_stock(good, 1, false).unwrap());
}

#[test]
fn player_sees_and_gives_only_to_people_at_the_same_local_position() {
    use game_economy::{Economy, LocalEndpoint, LocalLocation, LocalPosition};
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut initial = scenario(
        &root.join("content/scenarios/economy-local-walking.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let sites = initial.work_sites.as_mut().unwrap();
    sites.next_id = 2;
    sites.sites.push(game_economy::WorkSite {
        id: 1,
        region: 1,
        kind: "workshop".into(),
        capacity: 1,
    });
    let stock = LocalEndpoint::Stock;
    let store1 = LocalEndpoint::Store { id: 1 };
    let store2 = LocalEndpoint::Store { id: 2 };
    let routes = &mut initial
        .local_transport
        .as_mut()
        .unwrap()
        .walking
        .as_mut()
        .unwrap()
        .routes;
    std::sync::Arc::make_mut(routes)
        .iter_mut()
        .find(|route| route.from == stock && route.to == store1)
        .unwrap()
        .ticks = 3;
    let walking = |to, remaining| LocalLocation::Walking {
        region: 1,
        from: stock,
        to,
        remaining,
    };
    let make = |a, b| {
        let mut state = initial.clone();
        state
            .local_transport
            .as_mut()
            .unwrap()
            .walking
            .as_mut()
            .unwrap()
            .positions = vec![
            LocalPosition {
                person: 1,
                location: a,
            },
            LocalPosition {
                person: 2,
                location: b,
            },
        ];
        Economy::compile(state).unwrap()
    };
    let mut separated = make(
        LocalLocation::At {
            region: 1,
            endpoint: store1,
        },
        LocalLocation::At {
            region: 1,
            endpoint: store2,
        },
    );
    let mut player = Player::new(&mut separated);
    let view = player.view().unwrap();
    assert!(view.nearby.is_empty());
    assert!(view.work_sites.is_empty());
    assert!(player.give(2, view.possessions[0].good, 1).is_err());

    let mut at_stock = make(
        LocalLocation::At {
            region: 1,
            endpoint: stock,
        },
        LocalLocation::At {
            region: 1,
            endpoint: store2,
        },
    );
    assert_eq!(
        Player::new(&mut at_stock).view().unwrap().work_sites.len(),
        1
    );

    let mut body_state = initial.clone();
    body_state.people[1].health = 0;
    body_state
        .family
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 2)
        .unwrap()
        .alive = false;
    body_state
        .local_transport
        .as_mut()
        .unwrap()
        .walking
        .as_mut()
        .unwrap()
        .positions = vec![
        LocalPosition {
            person: 1,
            location: LocalLocation::At {
                region: 1,
                endpoint: store1,
            },
        },
        LocalPosition {
            person: 2,
            location: LocalLocation::At {
                region: 1,
                endpoint: store2,
            },
        },
    ];
    let mut body_world = Economy::compile(body_state.clone()).unwrap();
    assert!(
        Player::new(&mut body_world)
            .view()
            .unwrap()
            .bodies
            .is_empty()
    );
    body_state
        .local_transport
        .as_mut()
        .unwrap()
        .walking
        .as_mut()
        .unwrap()
        .positions[1]
        .location = LocalLocation::At {
        region: 1,
        endpoint: store1,
    };
    let mut body_world = Economy::compile(body_state).unwrap();
    assert_eq!(Player::new(&mut body_world).view().unwrap().bodies.len(), 1);

    let mut different_legs = make(walking(store1, 1), walking(store2, 1));
    assert!(
        Player::new(&mut different_legs)
            .view()
            .unwrap()
            .nearby
            .is_empty()
    );
    let mut different_progress = make(walking(store1, 1), walking(store1, 2));
    assert!(
        Player::new(&mut different_progress)
            .view()
            .unwrap()
            .nearby
            .is_empty()
    );
    let mut together = make(walking(store1, 1), walking(store1, 1));
    let mut player = Player::new(&mut together);
    let view = player.view().unwrap();
    assert_eq!(view.nearby, vec![2]);
    assert!(player.give(2, view.possessions[0].good, 1).unwrap());
}

#[test]
fn authored_local_visit_is_an_independent_personal_choice() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-local-walking.toml"),
        &root.join("content"),
    )
    .unwrap();
    world.set_local_carrying_choice(2, false).unwrap();
    world.set_local_visit_choice(2, true).unwrap();
    world.advance().unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let walking = state.local_transport.unwrap().walking.unwrap();
    assert_eq!(
        walking.location(2, 1),
        game_economy::LocalLocation::At {
            region: 1,
            endpoint: game_economy::LocalEndpoint::Store { id: 1 },
        }
    );
}

#[test]
fn authored_local_food_search_leads_to_a_real_meal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-local-food-search.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut visited = false;
                let mut acquired = false;
                for _ in 0..5 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let local = state.local_transport.as_ref().unwrap();
                    visited |= local.walking.as_ref().unwrap().events.iter().any(|event| {
                        event.person == 2
                            && event.outcome == game_economy::LocalWalkOutcome::Arrived
                    });
                    acquired |= state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|container| container.place == game_lots::Place::Person(2))
                        .is_some_and(|container| !container.lots.is_empty());
                }
                assert!(visited && acquired);
                assert!(world.snapshot().people[1].hunger < 9);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_goal_search_reaches_a_store_and_returns_with_material() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, fixture: &str, source| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join(format!("content/scenarios/{fixture}.toml")),
                    &root.join("content"),
                )
                .unwrap();
                let mut visited = false;
                let mut visited_source = false;
                let mut carried = false;
                for _ in 0..20 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    let local = state.local_transport.as_ref().unwrap();
                    visited |= local.walking.as_ref().unwrap().events.iter().any(|event| {
                        event.person == 2
                            && event.outcome == game_economy::LocalWalkOutcome::Arrived
                            && event.destination == game_economy::LocalEndpoint::Store { id: 1 }
                    });
                    visited_source |= local.walking.as_ref().unwrap().events.iter().any(|event| {
                        event.person == 2
                            && event.outcome == game_economy::LocalWalkOutcome::Arrived
                            && event.destination
                                == game_economy::LocalEndpoint::Store { id: source }
                    });
                    carried |= local.events.iter().any(|event| {
                        event.person == 2
                            && event.moved
                            && event.from == game_lots::Place::Inventory(source)
                    });
                }
                assert!(visited && visited_source && carried);
                let state = world.snapshot();
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress > 0)
                );
                state
            })
    };
    for (fixture, source) in [("economy-goal-search", 1), ("economy-goal-multistore", 2)] {
        assert_eq!(run(1, fixture, source), run(4, fixture, source));
    }
}

#[test]
fn authored_text_is_written_transferred_and_read_without_automatic_agreement() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-written-text.toml"),
        &root.join("content"),
    )
    .unwrap();
    world.set_work_choice(2, false).unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    assert_eq!(player.text_choice().unwrap().unwrap().1, 1);
    let item = player.write_text(1, 1).unwrap().unwrap();
    assert_eq!(
        player.known_page_text(item).unwrap().as_deref(),
        Some("Please meet me tomorrow.")
    );
    assert!(player.give_page(2, item).unwrap());
    assert!(world.known_page_text(2, item).is_err());
    world.advance().unwrap();
    assert_eq!(
        world.known_page_text(2, item).unwrap().as_deref(),
        Some("Please meet me tomorrow.")
    );
    assert_eq!(world.remembered_texts(1).unwrap().len(), 1);
    assert_eq!(world.remembered_texts(2).unwrap().len(), 1);
    let state = world.snapshot();
    assert!(
        state
            .writing
            .unwrap()
            .pages
            .iter()
            .find(|p| p.item == item)
            .unwrap()
            .claim
            .is_none()
    );
}

#[test]
fn remembered_text_survives_physical_page_consumption_and_reload() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-written-text.toml"),
        &root.join("content"),
    )
    .unwrap();
    world.set_work_choice(2, false).unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let item = player.write_text(1, 1).unwrap().unwrap();
    assert!(player.read_page(item).unwrap());
    let memory = &player.remembered_texts().unwrap()[0];
    assert_eq!((memory.first_read_at, memory.read_at), (1, 2));
    let mut state = world.snapshot();
    state.goals = None;
    state.dreams = None;
    state.skills = None;
    let writing = state.writing.as_ref().unwrap();
    let page_good = writing.page;
    let paper = writing.paper;
    let recipe = state.physical.recipes.first_mut().unwrap();
    recipe.inputs = vec![game_production::Amount {
        good: page_good,
        quantity: 1,
    }];
    recipe.outputs = vec![game_production::Amount {
        good: paper,
        quantity: 1,
    }];
    recipe.ticks = 1;
    let recipe_id = recipe.id;
    let mut world = game_economy::Economy::compile(state).unwrap();
    assert!(
        game_economy_player::Player::new(&mut world)
            .transfer_stock(page_good, 1, false)
            .unwrap()
    );
    assert!(
        game_economy_player::Player::new(&mut world)
            .start_work(recipe_id, 1)
            .unwrap()
    );
    for _ in 0..3 {
        if world.snapshot().writing.as_ref().unwrap().pages.is_empty() {
            break;
        }
        game_economy_player::Player::new(&mut world)
            .continue_work()
            .unwrap();
    }
    let state = world.snapshot();
    assert!(state.writing.as_ref().unwrap().pages.is_empty());
    let mut restored = game_economy::Economy::compile(
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
    )
    .unwrap();
    let player = game_economy_player::Player::new(&mut restored);
    assert_eq!(
        player.known_page_text(item).unwrap().as_deref(),
        Some("Please meet me tomorrow.")
    );
    assert_eq!(player.remembered_texts().unwrap()[0].first_read_at, 1);
}

#[test]
fn player_observes_and_takes_over_authored_local_work() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-work-succession.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .shared_work
        .as_mut()
        .unwrap()
        .succession
        .as_mut()
        .unwrap()
        .preferences
        .clear();
    let mut world = game_economy::Economy::compile(state).unwrap();
    for _ in 0..12 {
        if !world.advance().unwrap().started.is_empty() {
            break;
        }
    }
    let job = world.snapshot().physical.work[0].job.id;
    world.set_work_choice(2, false).unwrap();
    assert_eq!(world.advance().unwrap().suspended, vec![job]);
    let mut player = game_economy_player::Player::new(&mut world);
    let view = player.view().unwrap();
    assert!(
        view.vacant_work
            .iter()
            .any(|row| row.job == job && row.site == 1)
    );
    assert!(player.take_over_work(job, false).unwrap());
    assert!(player.view().unwrap().vacant_work.is_empty());
}

#[test]
fn playable_care_uses_real_medicine_and_can_transmit_illness_during_a_project() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, willing: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let world = scenario(
                    &root.join("content/scenarios/economy-care-project.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut initial = world.snapshot();
                initial.medical.as_mut().unwrap().policies[0].willing = willing;
                let medicine = initial
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == "medicine")
                    .unwrap()
                    .id;
                let mut world = game_economy::Economy::compile(initial).unwrap();
                world.advance().unwrap();
                let state = world.snapshot();
                let cases = &state
                    .dynamics
                    .as_ref()
                    .unwrap()
                    .disease
                    .as_ref()
                    .unwrap()
                    .cases;
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| row.place == game_lots::Place::Person(3))
                    .unwrap();
                let held = holder
                    .lots
                    .iter()
                    .filter(|lot| lot.good == medicine)
                    .map(|lot| lot.quantity)
                    .sum::<u64>();
                if !willing {
                    assert!(cases.iter().any(|case| case.person == 2));
                    assert!(!cases.iter().any(|case| case.person == 3));
                    assert_eq!(held, 1);
                    return state;
                }
                assert!(!cases.iter().any(|case| case.person == 2));
                assert!(
                    cases
                        .iter()
                        .any(|case| case.person == 3 && case.source == Some(2))
                );
                assert_eq!(held, 0);
                assert!(state.physical.work.iter().all(|work| work.job.worker != 2));
                let saved = serde_json::to_string(&state).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..20 {
                    if world
                        .snapshot()
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress == 1)
                    {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress == 1)
                );
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn playable_community_forms_personal_proposals_without_compelling_acceptance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, approve: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let world = scenario(
                    &root.join("content/scenarios/economy-community.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut initial = world.snapshot();
                if !approve {
                    let mind = &mut initial
                        .giving
                        .as_mut()
                        .unwrap()
                        .practices
                        .as_mut()
                        .unwrap()
                        .minds[1];
                    mind.approve = false;
                    mind.willing = false;
                }
                let mut world = game_economy::Economy::compile(initial).unwrap();
                assert!(
                    game_economy_player::Player::new(&mut world)
                        .view()
                        .unwrap()
                        .practices
                        .is_none()
                );
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                let mut result = None;
                for _ in 0..30 {
                    let state = world.snapshot();
                    let minds = &state
                        .giving
                        .as_ref()
                        .unwrap()
                        .practices
                        .as_ref()
                        .unwrap()
                        .minds;
                    if let Some(item) = minds[1]
                        .inbox
                        .iter()
                        .find(|item| item.decision != game_economy::PracticeDecision::Pending)
                    {
                        assert!(!minds[0].authored.is_empty());
                        result = Some(item.decision);
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    result,
                    Some(if approve {
                        game_economy::PracticeDecision::Adopted
                    } else {
                        game_economy::PracticeDecision::Declined
                    }),
                    "minds={:?}",
                    state.giving.as_ref().unwrap().practices
                );
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn authored_recovery_scenario_has_a_real_observer_and_physical_transfer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-recovery.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let before = world.snapshot();
                assert!(before.taking.as_ref().unwrap().policies[0].recover_incapacitated);
                world.advance().unwrap();
                let saved = world.snapshot();
                let taking = saved.taking.as_ref().unwrap();
                let event = taking
                    .events
                    .iter()
                    .find(|event| event.actor == 1 && event.target == 2)
                    .unwrap();
                assert_eq!(event.outcome, game_economy::TakingOutcome::Transferred);
                assert!(
                    taking
                        .witnesses
                        .iter()
                        .any(|w| w.person == 3 && w.last_seen.is_some())
                );
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_victim_reports_a_taking_without_deciding_what_the_listener_believes() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap();
                assert!(Player::new(&mut world).taking_reports().unwrap().is_empty());
                world.advance().unwrap();
                world.advance().unwrap();
                let after_taking = world.snapshot();
                let taking = after_taking.taking.as_ref().unwrap();
                let seen = taking
                    .witnesses
                    .iter()
                    .find(|w| w.person == 2 && w.actor == 1)
                    .unwrap()
                    .last_seen
                    .expect("victim noticed the taking");
                assert_eq!(seen.outcome, game_economy::TakingOutcome::Transferred);
                assert!(Player::new(&mut world).taking_reports().unwrap().is_empty());
                let saved = serde_json::to_string(&after_taking).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert!(Player::new(&mut restored).meet(2).unwrap());
                let reports = Player::new(&mut world)
                    .taking_reports()
                    .unwrap()
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>();
                let account = reports
                    .iter()
                    .find(|report| report.speaker == 2 && report.actor == 1)
                    .expect("the victim shared their account");
                assert_eq!(account.observed_at, seen.tick);
                assert_eq!(
                    account.claimed_outcome,
                    game_economy::TakingOutcome::Resisted
                );
                assert_eq!(account.belief, game_economy::TakingReportBelief::Doubts);
                let now = world.snapshot();
                assert_eq!(now, restored.snapshot());
                assert_eq!(
                    now.taking
                        .as_ref()
                        .unwrap()
                        .witnesses
                        .iter()
                        .find(|w| w.person == 2 && w.actor == 1)
                        .unwrap()
                        .last_seen,
                    Some(seen)
                );
                now
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn personal_inquiry_preserves_the_account_and_requires_contact_and_choice() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, willing| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap();
                assert!(
                    Player::new(&mut world)
                        .request_taking_inquiry(2, 1, 1, 10)
                        .is_err()
                );
                world.advance().unwrap();
                world.advance().unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                world
                    .set_taking_inquiry_preference(
                        2,
                        Some(game_economy::TakingComplaintPreference {
                            person: 2,
                            willing,
                            pressure_threshold: None,
                        }),
                    )
                    .unwrap();
                Player::new(&mut world)
                    .request_taking_inquiry(2, 1, 1, 10)
                    .unwrap();
                let pending = Player::new(&mut world).taking_inquiries().unwrap()[0].clone();
                assert_eq!(pending.account.source, Some(2));
                assert_eq!(
                    pending.account.outcome,
                    game_economy::TakingOutcome::Resisted
                );
                assert!(world.taking_inquiries(2).is_empty());
                assert!(
                    Player::new(&mut world)
                        .request_taking_inquiry(2, 1, 1, 10)
                        .is_err()
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                // The next taking may alter the victim's memory; it must not rewrite the request.
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(
                    Player::new(&mut world).taking_inquiries().unwrap()[0].account,
                    pending.account
                );
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert!(Player::new(&mut restored).meet(2).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let inquiry = Player::new(&mut world).taking_inquiries().unwrap()[0].clone();
                assert_eq!(inquiry.account, pending.account);
                assert_eq!(
                    inquiry.decision,
                    if willing {
                        game_economy::TakingComplaintDecision::Accepted
                    } else {
                        game_economy::TakingComplaintDecision::Declined
                    }
                );
                assert_eq!(world.taking_inquiries(2).len(), 1);
                state
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn earlier_heard_case_can_personally_influence_a_later_inquiry() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-judgment.toml"),
                    &root.join("content"),
                )
                .unwrap();
                Player::new(&mut world)
                    .set_taking_attention(1, true)
                    .unwrap();
                world.advance().unwrap();
                world.set_taking_choice(1, false).unwrap();
                Player::new(&mut world)
                    .set_taking_report(2, 1, None)
                    .unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                let first = world
                    .taking_reports(2)
                    .iter()
                    .find(|r| r.actor == 1)
                    .unwrap()
                    .observed_at;
                Player::new(&mut world)
                    .set_taking_report_policy(2, 1, None)
                    .unwrap();
                world.set_taking_choice(1, true).unwrap();
                world.advance().unwrap();
                world.set_taking_choice(1, false).unwrap();
                world
                    .set_taking_assessment_preference(
                        2,
                        Some(game_economy::TakingAssessmentPreference {
                            person: 2,
                            matching_unassessed: Some(3),
                            prior_actor_adjustment: Some(-8),
                            ..Default::default()
                        }),
                    )
                    .unwrap();
                Player::new(&mut world)
                    .request_taking_inquiry(2, 3, 1, 12)
                    .unwrap();
                let account = Player::new(&mut world).taking_inquiries().unwrap()[0].account;
                assert!(account.observed_at > first);
                assert!(Player::new(&mut world).meet(2).unwrap());
                Player::new(&mut world)
                    .set_taking_report(2, 1, None)
                    .unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for owner in [&mut world, &mut restored] {
                    assert!(Player::new(owner).meet(2).unwrap());
                }
                let answer = world.taking_inquiries(2).remove(0).answer.unwrap();
                assert_eq!(answer.assessment, Some(-5));
                assert_eq!(answer.precedent.as_ref().unwrap().observed_at, first);
                assert!(world.taking_inquiries(3)[0].answer.is_none());
                assert_eq!(world.snapshot(), restored.snapshot());
                for owner in [&mut world, &mut restored] {
                    owner.set_taking_choice(2, false).unwrap();
                    owner.advance().unwrap();
                    assert!(
                        owner
                            .snapshot()
                            .taking
                            .as_ref()
                            .unwrap()
                            .events
                            .iter()
                            .all(|e| e.actor != 2)
                    );
                    owner.set_taking_choice(2, true).unwrap();
                    owner.advance().unwrap();
                    assert!(owner.snapshot().taking.as_ref().unwrap().events.iter().any(
                        |e| e.actor == 2
                            && e.target == 1
                            && e.outcome == game_economy::TakingOutcome::Transferred
                    ));
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                for owner in [&mut world, &mut restored] {
                    owner.set_taking_choice(2, false).unwrap();
                    owner.set_taking_choice(1, true).unwrap();
                    owner.advance().unwrap();
                    owner.set_taking_choice(1, false).unwrap();
                    Player::new(owner)
                        .set_taking_report_policy(2, 1, None)
                        .unwrap();
                    owner
                        .set_taking_assessment_preference(
                            2,
                            Some(game_economy::TakingAssessmentPreference {
                                person: 2,
                                matching_unassessed: Some(3),
                                prior_assessment_scale: Some(-2),
                                ..Default::default()
                            }),
                        )
                        .unwrap();
                    Player::new(owner)
                        .request_taking_inquiry(2, 3, 1, 12)
                        .unwrap();
                    assert!(owner.taking_inquiries(3)[0].previous_case.is_none());
                    assert_eq!(owner.taking_case_memory(2).unwrap().assessment, -5);
                    assert!(owner.taking_case_memory(3).is_none());
                    assert!(Player::new(owner).meet(2).unwrap());
                    assert_eq!(
                        owner.taking_inquiries(2)[0]
                            .previous_case
                            .as_ref()
                            .unwrap()
                            .assessment,
                        -5
                    );
                    Player::new(owner).set_taking_report(2, 1, None).unwrap();
                    assert!(Player::new(owner).meet(2).unwrap());
                    let new_answer = owner.taking_inquiries(2).remove(0).answer.unwrap();
                    assert_eq!(new_answer.assessment, Some(13));
                    assert_eq!(new_answer.prior_case.unwrap().assessment, -5);
                    assert!(owner.taking_inquiries(3)[0].answer.is_none());
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                let saved_again = serde_json::to_string(&world.snapshot()).unwrap();
                let loaded =
                    game_economy::Economy::compile(serde_json::from_str(&saved_again).unwrap())
                        .unwrap();
                assert_eq!(loaded.snapshot(), world.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn accepted_inquiry_records_a_later_witness_statement_privately() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap();
                Player::new(&mut world)
                    .set_taking_attention(1, true)
                    .unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                world.set_taking_choice(1, false).unwrap();
                Player::new(&mut world)
                    .set_taking_report(2, 1, Some(game_economy::TakingOutcome::Resisted))
                    .unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert_eq!(world.taking_reports(2).len(), 1);
                Player::new(&mut world)
                    .request_taking_inquiry(2, 3, 1, 12)
                    .unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                let accepted = Player::new(&mut world)
                    .taking_inquiries()
                    .unwrap()
                    .remove(0);
                assert_eq!(
                    accepted.decision,
                    game_economy::TakingComplaintDecision::Accepted
                );
                assert!(accepted.answer.is_none());
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for owner in [&mut world, &mut restored] {
                    assert!(Player::new(owner).meet(2).unwrap());
                }
                let investigator = world.taking_inquiries(2).remove(0);
                let answer = investigator
                    .answer
                    .expect("a later statement reaches the investigator");
                assert_eq!(answer.report.speaker, 3);
                assert_eq!(answer.report.observed_at, accepted.account.observed_at);
                assert_eq!(
                    answer.report.claimed_outcome,
                    game_economy::TakingOutcome::Resisted
                );
                assert!(!answer.matches_account);
                assert_eq!(answer.assessment, Some(-7));
                let before_invalid = world.snapshot();
                assert!(world.revise_taking_assessment(3, Some(12)).is_err());
                assert_eq!(world.snapshot(), before_invalid);
                for owner in [&mut world, &mut restored] {
                    owner.revise_taking_assessment(2, Some(12)).unwrap();
                    assert_eq!(
                        owner.taking_inquiries(2)[0]
                            .answer
                            .as_ref()
                            .unwrap()
                            .assessment,
                        Some(12)
                    );
                    owner.revise_taking_assessment(2, None).unwrap();
                    assert_eq!(
                        owner.taking_inquiries(2)[0]
                            .answer
                            .as_ref()
                            .unwrap()
                            .assessment,
                        None
                    );
                    owner.revise_taking_assessment(2, Some(-4)).unwrap();
                }
                assert_eq!(
                    before_invalid.taking.as_ref().unwrap().complaints[0]
                        .answer
                        .as_ref()
                        .unwrap()
                        .assessment,
                    Some(-7)
                );
                let revised_save = serde_json::to_string(&world.snapshot()).unwrap();
                let revised =
                    game_economy::Economy::compile(serde_json::from_str(&revised_save).unwrap())
                        .unwrap();
                assert_eq!(revised.snapshot(), world.snapshot());
                assert!(
                    Player::new(&mut world).taking_inquiries().unwrap()[0]
                        .answer
                        .is_none()
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn autonomous_people_pursue_inquiries_without_forcing_testimony() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, testify| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                world.set_taking_choice(1, false).unwrap();
                world
                    .set_taking_inquiry_preference(
                        1,
                        Some(game_economy::TakingComplaintPreference {
                            person: 1,
                            willing: true,
                            pressure_threshold: None,
                        }),
                    )
                    .unwrap();
                world.set_taking_inquiry_pursuit(2, true).unwrap();
                world.set_taking_inquiry_pursuit(1, true).unwrap();
                world
                    .set_taking_report_policy(
                        2,
                        1,
                        1,
                        Some(game_economy::TakingReportPolicy {
                            person: 2,
                            recipient: 1,
                            actor: 1,
                            willing: testify,
                            pressure_threshold: None,
                            claimed_outcome: None,
                            source: Default::default(),
                            heard_from: None,
                            details: None,
                        }),
                    )
                    .unwrap();
                world.request_taking_inquiry(2, 1, 2, 1, 10).unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..6 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                    if !world.taking_inquiries(1).is_empty() {
                        break;
                    }
                }
                assert_eq!(
                    world.taking_inquiries(1)[0].decision,
                    game_economy::TakingComplaintDecision::Accepted
                );
                assert!(world.taking_inquiries(1)[0].answer.is_none());
                for _ in 0..4 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                    if world.taking_inquiries(1)[0].answer.is_some() {
                        break;
                    }
                }
                let inquiry = world.taking_inquiries(1).remove(0);
                assert_eq!(inquiry.answer.is_some(), testify);
                if let Some(answer) = inquiry.answer {
                    assert!(answer.matches_account);
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1, true), run(4, true));
    assert_eq!(run(1, false), run(4, false));
}

#[test]
fn playable_succession_waits_for_an_actual_birth_and_world_keeps_advancing() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-succession.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut no_birth = world.snapshot();
                no_birth
                    .family
                    .as_mut()
                    .unwrap()
                    .members
                    .iter_mut()
                    .find(|member| member.id == 2)
                    .unwrap()
                    .pregnancy = None;
                let mut no_birth = game_economy::Economy::compile(no_birth).unwrap();
                world.advance().unwrap();
                no_birth.advance().unwrap();
                assert_eq!(
                    world.snapshot().campaign.as_ref().unwrap().lineage.current,
                    None
                );
                assert_eq!(
                    world
                        .snapshot()
                        .people
                        .iter()
                        .find(|person| person.id == 1)
                        .unwrap()
                        .health,
                    0
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..8 {
                    if world
                        .snapshot()
                        .campaign
                        .as_ref()
                        .unwrap()
                        .lineage
                        .current
                        .is_some()
                    {
                        break;
                    }
                    game_economy_player::Player::new(&mut world)
                        .advance()
                        .unwrap();
                    game_economy_player::Player::new(&mut restored)
                        .advance()
                        .unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let child = state
                    .campaign
                    .as_ref()
                    .unwrap()
                    .lineage
                    .current
                    .expect("actual birth should allow succession");
                assert_ne!(child, 1);
                assert!(
                    state
                        .family
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .any(|member| member.id == child && member.parents.contains(&2))
                );
                assert!(
                    state
                        .people
                        .iter()
                        .any(|person| person.id == child && person.controlled)
                );
                assert!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|container| container.place == game_lots::Place::Person(child))
                );
                for _ in 0..10 {
                    let guardians = game_economy_player::Player::new(&mut world)
                        .guardians()
                        .unwrap();
                    if guardians
                        .iter()
                        .all(|guardian| guardian.region == Some(1) && guardian.journey.is_none())
                    {
                        break;
                    }
                    game_economy_player::Player::new(&mut world)
                        .advance()
                        .unwrap();
                    game_economy_player::Player::new(&mut restored)
                        .advance()
                        .unwrap();
                }
                assert!(
                    game_economy_player::Player::new(&mut world)
                        .guardians()
                        .unwrap()
                        .iter()
                        .all(|guardian| guardian.region == Some(1) && guardian.journey.is_none())
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                for _ in 0..6 {
                    game_economy_player::Player::new(&mut no_birth)
                        .advance()
                        .unwrap();
                }
                assert!(no_birth.tick() > 1);
                assert_eq!(
                    no_birth
                        .snapshot()
                        .campaign
                        .as_ref()
                        .unwrap()
                        .lineage
                        .current,
                    None
                );
                assert_eq!(no_birth.snapshot().people.len(), 4);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_successor_invokes_their_actual_previous_life() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-succession.toml"),
                    &root.join("content"),
                )
                .unwrap();
                for _ in 0..10 {
                    world.advance().unwrap();
                    if world
                        .snapshot()
                        .campaign
                        .as_ref()
                        .unwrap()
                        .lineage
                        .current
                        .is_some_and(|id| id != 1)
                    {
                        break;
                    }
                }
                let state = world.snapshot();
                let lineage = &state.campaign.as_ref().unwrap().lineage;
                assert_ne!(lineage.current, None);
                assert_eq!(lineage.history.len(), 1);
                assert_eq!(lineage.history[0].person, 1);
                assert!(Player::new(&mut world).past_knowledge(1).is_err());
                Player::new(&mut world).invoke(&[1]).unwrap();
                let remembered = Player::new(&mut world).past_knowledge(1).unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(
                    Player::new(&mut restored).past_knowledge(1).unwrap(),
                    remembered
                );
                let mut warned = false;
                for _ in 0..10 {
                    let before = world.tick();
                    let result = Player::new(&mut world).advance();
                    let restored_result = Player::new(&mut restored).advance();
                    assert_eq!(result.is_ok(), restored_result.is_ok());
                    if result.is_err() {
                        assert_eq!(world.tick(), before);
                        warned = world
                            .snapshot()
                            .campaign
                            .unwrap()
                            .lineage
                            .invocation
                            .unwrap()
                            .pending
                            .is_some();
                        break;
                    }
                }
                assert!(warned, "the ongoing invocation should warn before its cost");
                assert_eq!(world.snapshot(), restored.snapshot());
                Player::new(&mut world).dismiss_invocation().unwrap();
                Player::new(&mut restored).dismiss_invocation().unwrap();
                assert!(Player::new(&mut world).past_knowledge(1).is_err());
                Player::new(&mut world).advance().unwrap();
                Player::new(&mut restored).advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_conflict_does_not_stop_other_peoples_project() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-conflict-project.toml"),
                    &root.join("content"),
                )
                .unwrap();
                game_economy_player::Player::new(&mut world)
                    .attack(3)
                    .unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..20 {
                    let state = world.snapshot();
                    assert!(
                        state
                            .combat
                            .as_ref()
                            .unwrap()
                            .conflict
                            .encounters
                            .iter()
                            .any(|encounter| encounter
                                .fighters
                                .iter()
                                .any(|fighter| fighter.person == 3))
                    );
                    assert!(state.physical.work.iter().all(|work| work.job.worker != 3));
                    if state.goals.as_ref().is_some_and(|goals| {
                        goals
                            .people
                            .iter()
                            .any(|goal| goal.person == 2 && goal.progress > 0)
                    }) {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress == 1)
                );
                assert!(
                    !state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 3 && goal.progress > 0)
                );
                assert!(
                    state
                        .people
                        .iter()
                        .find(|person| person.id == 3)
                        .unwrap()
                        .health
                        < 100
                );
                let injury = state
                    .people
                    .iter()
                    .find(|person| person.id == 3)
                    .unwrap()
                    .health;
                game_economy_player::Player::new(&mut world)
                    .combat_order(game_economy_combat::Order::Retreat)
                    .unwrap();
                game_economy_player::Player::new(&mut restored)
                    .combat_order(game_economy_combat::Order::Retreat)
                    .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert!(
                    game_economy_player::Player::new(&mut world)
                        .view()
                        .unwrap()
                        .battle
                        .is_none()
                );
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .people
                        .iter()
                        .find(|person| person.id == 3)
                        .unwrap()
                        .health
                        <= injury
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_regional_project_requires_requested_physical_delivery() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-regional-project.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let good = |name: &str| {
                    initial
                        .physical
                        .goods
                        .iter()
                        .find(|row| row.name == name)
                        .unwrap()
                        .id
                };
                let timber = good("timber");
                let tool = good("tool");
                let mut refused = initial.clone();
                refused.hauling.as_mut().unwrap().policies[0].willing = false;
                let mut refused = game_economy::Economy::compile(refused).unwrap();
                game_economy_player::Player::new(&mut refused)
                    .ask_haul(2, 2, 1, timber, 7)
                    .unwrap();
                for _ in 0..12 {
                    refused.advance().unwrap();
                }
                let refused = refused.snapshot();
                assert_eq!(
                    refused
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place
                            == (game_lots::Place::Stock {
                                region: 2,
                                good: timber
                            }))
                        .unwrap()
                        .used,
                    5
                );
                assert!(
                    !refused
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress > 0)
                );
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                assert!(
                    !world
                        .snapshot()
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress > 0)
                );
                game_economy_player::Player::new(&mut world)
                    .ask_haul(2, 2, 1, timber, 40)
                    .unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..30 {
                    if world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|row| {
                            row.place
                                == (game_lots::Place::Stock {
                                    region: 1,
                                    good: tool,
                                })
                                && row.used == 1
                        })
                    {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .hauling
                        .as_ref()
                        .unwrap()
                        .requests
                        .iter()
                        .any(|request| request.carrier == 2)
                );
                assert!(
                    state.hauling.as_ref().unwrap().policies[0].retry_at
                        > state.physical.custody.tick
                );
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress == 1),
                    "people={:?} goals={:?} hauling={:?} travel={:?} sites={:?} timber={:?}",
                    state.people,
                    state.goals,
                    state.hauling,
                    state.travel,
                    state.work_sites,
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .filter(|row| row.place
                            == (game_lots::Place::Stock {
                                region: 1,
                                good: timber
                            })
                            || row.place
                                == (game_lots::Place::Stock {
                                    region: 2,
                                    good: timber
                                })
                            || row.place == game_lots::Place::Person(2))
                        .collect::<Vec<_>>()
                );
                assert!(state.physical.custody.containers.iter().any(|row| row.place
                    == (game_lots::Place::Stock {
                        region: 1,
                        good: tool
                    })
                    && row.used == 1));
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place
                            == (game_lots::Place::Stock {
                                region: 2,
                                good: timber
                            }))
                        .unwrap()
                        .used,
                    0
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_project_waits_for_missing_material_then_resumes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let world = scenario(
                    &root.join("content/scenarios/economy-project.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let mut initial = world.snapshot();
                let good = |name: &str| {
                    initial
                        .physical
                        .goods
                        .iter()
                        .find(|row| row.name == name)
                        .unwrap()
                        .id
                };
                let timber = good("timber");
                let tool = good("tool");
                let store = initial
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == game_lots::Place::Inventory(1))
                    .unwrap();
                *store = game_lots::Container::new(
                    store.place,
                    store.capacity,
                    vec![game_lots::Lot {
                        item: None,
                        good: timber,
                        quantity: 4,
                        expires_at: None,
                    }],
                )
                .unwrap();
                let mut world = game_economy::Economy::compile(initial).unwrap();
                for _ in 0..15 {
                    world.advance().unwrap();
                }
                let before = world.snapshot();
                assert!(
                    !before
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress > 0)
                );
                assert!(
                    game_economy_player::Player::new(&mut world)
                        .transfer_stock(timber, 1, false)
                        .unwrap()
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..30 {
                    if world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|row| {
                            row.place
                                == (game_lots::Place::Stock {
                                    region: 1,
                                    good: tool,
                                })
                                && row.used == 1
                        })
                    {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress == 1)
                );
                assert!(state.physical.custody.containers.iter().any(|row| row.place
                    == (game_lots::Place::Stock {
                        region: 1,
                        good: tool
                    })
                    && row.used == 1));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_project_builds_and_produces_from_real_materials() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-project.toml"),
                    &root.join("content"),
                )
                .unwrap();
                world.advance().unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..30 {
                    if world.snapshot().goals.as_ref().is_some_and(|goals| {
                        goals
                            .people
                            .iter()
                            .any(|goal| goal.person == 2 && goal.progress >= 1)
                    }) {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                // Finished work is credited before its physical output is delivered.
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(
                    state
                        .goals
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|goal| goal.person == 2 && goal.progress >= 1)
                );
                assert!(
                    state
                        .work_sites
                        .as_ref()
                        .unwrap()
                        .sites
                        .iter()
                        .any(|site| site.kind == "workshop")
                );
                let good = |name: &str| {
                    state
                        .physical
                        .goods
                        .iter()
                        .find(|row| row.name == name)
                        .unwrap()
                        .id
                };
                let stock = |good| {
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == game_lots::Place::Stock { region: 1, good })
                        .unwrap()
                        .used
                };
                assert_eq!(stock(good("reed-shelter-kit")), 0);
                assert_eq!(stock(good("timber")), 0);
                assert_eq!(stock(good("tool")), 1);
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|row| row.place == game_lots::Place::Inventory(1))
                        .unwrap()
                        .used,
                    0
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_campaign_supports_an_observation_expedition() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let id = game_economy_player::Player::new(&mut world)
                    .start_expedition(2, 1)
                    .unwrap();
                game_economy_cli_commands::player::handle(
                    &mut world,
                    &["observe-expedition", &id.to_string()],
                )
                .unwrap();
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..16 {
                    if world.snapshot().missions.as_ref().unwrap().entries[0].objective_met {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert!(world.snapshot().missions.as_ref().unwrap().entries[0].objective_met);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_campaign_exposes_only_the_rulers_encounter_memories() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    assert!(
        game_economy_player::Player::new(&mut world)
            .known_people()
            .unwrap()
            .is_empty()
    );
    assert!(
        game_economy_player::Player::new(&mut world)
            .meet(2)
            .unwrap()
    );
    let seen = game_economy_player::Player::new(&mut world)
        .known_people()
        .unwrap();
    assert_eq!(
        seen,
        vec![game_economy::SeenPerson {
            person: 2,
            region: 1,
            observed_at: 1
        }]
    );
    assert!(game_economy_cli_commands::player::handle(&mut world, &["known-people", "2"]).is_err());
    game_economy_cli_commands::player::handle(&mut world, &["remember-people", "off"]).unwrap();
    assert_eq!(
        game_economy_player::Player::new(&mut world)
            .known_people()
            .unwrap(),
        seen
    );
}

#[test]
fn playable_encounter_report_retains_the_witness_and_time() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut initial = scenario(
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                initial
                    .people
                    .iter_mut()
                    .find(|person| person.id == 99)
                    .unwrap()
                    .region = Some(1);
                let mind = initial
                    .knowledge
                    .as_mut()
                    .unwrap()
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 2)
                    .unwrap();
                mind.remember_people = true;
                mind.people_reporting = Some(game_economy::Reporting {
                    willing: true,
                    pressure_threshold: None,
                });
                let mut world = game_economy::Economy::compile(initial).unwrap();
                assert!(world.meet(2, 99).unwrap());
                let seen = world.known_people(2).unwrap();
                let memory = seen.iter().find(|seen| seen.person == 99).unwrap();
                let timestamp = memory.observed_at;
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert!(Player::new(&mut restored).meet(2).unwrap());
                let reports = Player::new(&mut world).people_reports().unwrap();
                assert!(reports.iter().any(|report| report.person == 99
                    && report.region == 1
                    && report.observed_at == timestamp
                    && report.speaker == 2));
                assert!(
                    !Player::new(&mut world)
                        .known_people()
                        .unwrap()
                        .iter()
                        .any(|seen| seen.person == 99)
                );
                assert!(
                    game_economy_cli_commands::player::handle(&mut world, &["people-reports"])
                        .unwrap()
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn playable_campaign_advances_consistently_across_workers_and_reload() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                for _ in 0..4 {
                    world.advance().unwrap();
                }
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
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
fn authored_taking_preferences_resolve_goods_and_keep_subjective_reactions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[[taking.policies]]\nperson = 2\ntarget = 1\ngood = \"bread\"\nquantity = 1\nwilling = false\npressured_by = 1\npressure_threshold = 2\n\n[[taking.responses]]\nperson = 1\nnotices = true\nresist = false\n\n[[taking.reactions]]\nperson = 1\nother = 2\n[taking.reactions.as_target]\ntransferred = {{ affinity = 3 }}\n\n[[taking.witnesses]]\nperson = 99\nactor = 1\ntarget = 2\nnotices = true\n[taking.witnesses.toward_actor]\ntransferred = {{ affinity = -2 }}\n\n[[taking.report_policies]]\nperson = 99\nrecipient = 2\nactor = 1\nwilling = true\nclaimed_outcome = \"resisted\"\ndetails = {{ target = 2, good = \"bread\", quantity = 17 }}\n\n[[taking.receptions]]\nperson = 2\nstance = \"doubt\"\ndifferent_toward_speaker = {{ affinity = 4 }}\n"
    );
    let path = std::env::temp_dir().join(format!("legacy-taking-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let taking = state.taking.unwrap();
    let bread = taking.policies[0].good;
    assert_eq!(
        state
            .physical
            .goods
            .iter()
            .find(|g| g.id == taking.policies[0].good)
            .unwrap()
            .name,
        "bread"
    );
    assert!(!taking.policies[0].willing);
    assert_eq!(taking.policies[0].pressure_threshold, Some(2));
    assert_eq!(
        taking.reactions[0].as_target.transferred.unwrap().affinity,
        3
    );
    assert_eq!(
        taking.witnesses[0]
            .toward_actor
            .transferred
            .unwrap()
            .affinity,
        -2
    );
    assert_eq!(
        taking.report_policies[0].claimed_outcome,
        Some(game_economy::TakingOutcome::Resisted)
    );
    assert_eq!(
        taking.receptions[0]
            .different_toward_speaker
            .unwrap()
            .affinity,
        4
    );
    let details = taking.report_policies[0].details.as_ref().unwrap();
    assert_eq!(
        (details.target, details.good, details.quantity),
        (Some(2), Some(bread), Some(17))
    );
    game_economy_cli_commands::player::handle(
        &mut world,
        &["taking-compare", "different", "-1", "3", "0", "0"],
    )
    .unwrap();
    assert_eq!(
        world
            .taking_reception(1)
            .unwrap()
            .different_toward_speaker
            .unwrap()
            .affinity,
        3
    );
    game_economy_cli_commands::player::handle(&mut world, &["taking-compare", "different", "off"])
        .unwrap();
    assert!(
        world
            .taking_reception(1)
            .unwrap()
            .different_toward_speaker
            .is_none()
    );
    game_economy_cli_commands::player::handle(&mut world, &["taking-belief", "believe"]).unwrap();
    game_economy_cli_commands::player::handle(
        &mut world,
        &["taking-report", "2", "99", "truthful"],
    )
    .unwrap();
    game_economy_cli_commands::player::handle(&mut world, &["taking-relay", "2", "99", "truthful"])
        .unwrap();
    game_economy_cli_commands::player::handle(
        &mut world,
        &["taking-details", "2", "99", "2", &bread.to_string(), "12"],
    )
    .unwrap();
    assert_eq!(
        world
            .taking_report_policy(1, 2, 99)
            .unwrap()
            .details
            .as_ref()
            .unwrap()
            .quantity,
        Some(12)
    );
    assert!(
        game_economy_cli_commands::player::handle(
            &mut world,
            &["taking-details", "2", "99", "2", &bread.to_string(), "0"],
        )
        .is_err()
    );
    assert_eq!(
        world
            .taking_report_policy(1, 2, 99)
            .unwrap()
            .details
            .as_ref()
            .unwrap()
            .quantity,
        Some(12)
    );
    assert!(
        world
            .snapshot()
            .taking
            .unwrap()
            .report_policies
            .iter()
            .any(|p| p.person == 1
                && p.actor == 99
                && p.source == game_economy::TakingReportSource::Heard)
    );
    assert_eq!(
        world.taking_reception(1).unwrap().stance,
        game_economy::TakingReportStance::Believe
    );
    game_economy_cli_commands::player::handle(&mut world, &["taking-watch", "2", "on"]).unwrap();
    assert!(world.taking_witness(1, 2).unwrap().notices);
    game_economy_cli_commands::player::handle(&mut world, &["taking-watch", "2", "off"]).unwrap();
    assert!(!world.taking_witness(1, 2).unwrap().notices);
    game_economy_cli_commands::player::handle(&mut world, &["taking-response", "notice", "resist"])
        .unwrap();
    assert!(
        world
            .snapshot()
            .taking
            .unwrap()
            .responses
            .iter()
            .any(|r| r.person == 1 && r.resist)
    );
    game_economy_cli_commands::player::handle(
        &mut world,
        &["take-from", "2", &bread.to_string(), "1", "withdraw"],
    )
    .unwrap();
    assert!(world.snapshot().taking.unwrap().events.iter().any(|event| {
        event.actor == 1
            && event.target == 2
            && event.outcome == game_economy::TakingOutcome::Transferred
    }));
}

#[test]
fn wildlife_receives_the_current_seasonal_exposure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.fauna.as_mut().unwrap().species[0].climate = Some(game_fauna::Climate {
        tolerance: 0,
        inherited_tolerance: 0,
        hardiness_locus: 1,
        damage_per_excess: 1,
    });
    let environment = state.environment.as_mut().unwrap();
    environment.seasons[0].duration = 1;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_economy::Economy::compile(state.clone()).unwrap();
                world.advance().unwrap();
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].health,
                    9
                );
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].health,
                    7
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn animal_care_content_resolves_named_goods() {
    let config: game_economy::AnimalCareConfig = toml::from_str(
        r#"
        [[policies]]
        person = 4
        animal = 1
        good = "grain"
        quantity = 2
        willing = false
        health_at_most = 8
        pressured_by = 1
        pressure_threshold = 3
        [[policies]]
        person = 5
        good = "grain"
        quantity = 1
        willing = true
        health_at_most = 8
    "#,
    )
    .unwrap();
    let care = config.build(|name| (name == "grain").then_some(7)).unwrap();
    assert_eq!(care.policies[0].good, 7);
    assert_eq!(care.policies[0].pressure_threshold, Some(3));
    assert_eq!(care.policies[0].animal, Some(1));
    assert_eq!(care.policies[1].animal, None);
}

#[test]
fn personal_animal_feelings_load_as_signed_content() {
    let feelings: game_economy::AnimalFeelings = toml::from_str(
        r#"
        [[minds]]
        person = 1
        minimum = -10
        maximum = 10
        capacity = 4
        on_feed = -2
        on_hunt = 3
        opinions = [{ animal = 1, affinity = -1 }]
    "#,
    )
    .unwrap();
    assert_eq!(feelings.minds[0].on_feed, -2);
    assert_eq!(feelings.minds[0].on_hunt, 3);
    assert_eq!(feelings.minds[0].opinions[0].affinity, -1);
}

#[test]
fn hunting_preferences_accept_optional_individual_targets() {
    let hunting: game_economy::Hunting = toml::from_str(
        r#"
        [[policies]]
        person = 1
        willing = true
        [[policies]]
        person = 2
        animal = 9
        willing = false
    "#,
    )
    .unwrap();
    assert_eq!(hunting.policies[0].animal, None);
    assert_eq!(hunting.policies[1].animal, Some(9));
}

#[test]
fn hunting_preferences_resolve_named_reserves() {
    let config: game_economy::HuntingConfig = toml::from_str(
        r#"
        [[policies]]
        person = 1
        willing = true
        hunger_at_least = 5
        reserve_good = "meat"
        reserve_below = 3
        request_response = { willing = false, minimum_trust = -2, pressure_threshold = 3 }
    "#,
    )
    .unwrap();
    let hunting = config.build(|name| (name == "meat").then_some(7)).unwrap();
    assert_eq!(hunting.policies[0].reserve_good, Some(7));
    assert_eq!(hunting.policies[0].reserve_below, Some(3));
    assert_eq!(
        hunting.policies[0].request_response.unwrap().minimum_trust,
        Some(-2)
    );
}

#[test]
fn wildlife_climate_settings_are_loaded_from_content() {
    let text = r#"
        next_id = 1
        animals = []
        [[species]]
        id = 1
        name = "grazer"
        food_good = "grain"
        intake = 1
        hunger_per_tick = 1
        relief_per_unit = 1
        starvation_threshold = 10
        starvation_damage = 1
        max_health = 10
        adult_age = 1
        fertile_until = 10
        lifespan = 20
        gestation = 2
        conception_one_in = 4
        breeding_max_hunger = 3
        newborn_health = 10
        [species.climate]
        tolerance = 1
        inherited_tolerance = 4
        hardiness_locus = 1
        damage_per_excess = 2
        [species.hunting]
        good = "grain"
        quantity = 2
        success_per_thousand = 1000
    "#;
    let config: game_fauna::content::Config = toml::from_str(text).unwrap();
    let fauna = config.build(|name| (name == "grain").then_some(1)).unwrap();
    assert_eq!(
        fauna.species[0]
            .climate
            .as_ref()
            .unwrap()
            .inherited_tolerance,
        4
    );
    assert_eq!(fauna.species[0].hunting.as_ref().unwrap().good, 1);
}

#[test]
fn player_hunts_a_local_animal_into_actual_possessions() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let meat = state
        .physical
        .goods
        .iter()
        .find(|good| good.name == "raw-meat")
        .unwrap()
        .id;
    assert_eq!(
        state.giving.as_ref().unwrap().responses[0].alternatives,
        vec![meat]
    );
    assert!(
        state.hunting.as_ref().unwrap().policies[0]
            .request_response
            .as_ref()
            .unwrap()
            .willing
    );
    assert_eq!(
        state.fauna.as_ref().unwrap().species[0]
            .hunting
            .as_ref()
            .unwrap()
            .good,
        meat
    );
    let mut world = game_economy::Economy::compile(state).unwrap();
    assert!(Player::new(&mut world).hunt_animal(1).unwrap());
    let view = Player::new(&mut world).view().unwrap();
    assert!(
        view.possessions
            .iter()
            .any(|lot| lot.good == meat && lot.quantity == 2 && lot.expires_at == Some(49))
    );
    assert_eq!(
        view.animals
            .iter()
            .find(|animal| animal.id == 1)
            .unwrap()
            .health,
        0
    );
    assert!(Player::new(&mut world).hunt_animal(1).is_err());
}

#[test]
fn player_can_handle_an_animal_after_its_own_experience_changes_its_response() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap();
    let bread = world
        .snapshot()
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    assert!(!Player::new(&mut world).handle_animal(1).unwrap());
    assert!(Player::new(&mut world).feed_animal(1, bread, 1).unwrap());
    assert!(Player::new(&mut world).handle_animal(1).unwrap());
    assert!(Player::new(&mut world).handle_animal(3).is_err());
    assert!(world.snapshot().fauna.unwrap().animals[0].health > 0);
}

#[test]
fn player_can_capture_and_release_a_live_animal() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let reaction = state
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|row| row.person == 2)
        .unwrap()
        .opposed_action_reaction
        .as_ref()
        .unwrap();
    assert_eq!(reaction.blocked.unwrap().affinity, 1);
    assert_eq!(reaction.overcame.unwrap().affinity, -1);
    assert!(Player::new(&mut world).capture_animal(1).unwrap());
    let animal = Player::new(&mut world)
        .view()
        .unwrap()
        .animals
        .into_iter()
        .find(|animal| animal.id == 1)
        .unwrap();
    assert!(animal.health > 0);
    assert_eq!(animal.held_by, Some(1));
    assert!(!Player::new(&mut world).defend_animal(1).unwrap());
    assert!(Player::new(&mut world).defend_animal(2).is_err());
    assert!(Player::new(&mut world).release_animal(1).unwrap());
    assert_eq!(
        Player::new(&mut world)
            .view()
            .unwrap()
            .animals
            .into_iter()
            .find(|animal| animal.id == 1)
            .unwrap()
            .held_by,
        None
    );
    assert!(Player::new(&mut world).capture_animal(3).is_err());
}

#[test]
fn wildlife_scenario_hungry_person_can_hunt_for_their_own_reserve() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let meat = state.fauna.as_ref().unwrap().species[0]
        .hunting
        .as_ref()
        .unwrap()
        .good;
    state.people[1].hunger = 12;
    state.people[1].seek_meals = false;
    if let Some(social) = state.dynamics.as_mut().and_then(|d| d.social.as_mut()) {
        for participant in &mut social.participants {
            participant.willing = false;
        }
    }
    let mut world = game_economy::Economy::compile(state).unwrap();
    world.advance().unwrap();
    let state = world.snapshot();
    let carried = state
        .physical
        .custody
        .containers
        .iter()
        .find(|c| c.place == game_lots::Place::Person(2))
        .unwrap();
    assert!(
        carried
            .lots
            .iter()
            .any(|lot| lot.good == meat && lot.quantity == 2)
    );
    assert_eq!(
        state
            .fauna
            .unwrap()
            .animals
            .iter()
            .filter(|animal| animal.health == 0)
            .count(),
        1
    );
}

#[test]
fn player_feeds_local_animals_only_with_real_carried_food() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let base = scenario(
        &root.join("content/scenarios/economy-wildlife.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = base.snapshot();
    let patch = initial
        .gathering
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == 3)
        .unwrap();
    patch.available = 0;
    patch.regrowth = 0;
    initial.fauna.as_mut().unwrap().animals[0].hunger = 4;
    initial.fauna.as_mut().unwrap().animals[0].social_response = Some(game_fauna::SocialResponse {
        feed_affinity: -2,
        minimum: -10,
        maximum: 10,
        capacity: 4,
    });
    initial.animal_feelings = Some(game_economy::AnimalFeelings {
        minds: vec![game_economy::AnimalFeelingMind {
            person: 1,
            minimum: -10,
            maximum: 10,
            capacity: 4,
            on_feed: 3,
            on_hunt: -1,
            on_handle_accepted: 0,
            on_handle_refused: 0,
            on_capture_yielded: 0,
            on_capture_forced: 0,
            on_capture_resisted: 0,
            on_animal_released: 0,
            on_animal_release_blocked: 0,
            on_defense_succeeded: 0,
            on_defense_overcome: 0,
            opinions: vec![],
        }],
    });
    let grain = initial.fauna.as_ref().unwrap().species[0].food_good;
    let bread = initial
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                assert!(Player::new(&mut world).feed_animal(3, grain, 1).is_err());
                assert!(Player::new(&mut world).feed_animal(1, u32::MAX, 1).is_err());
                assert_eq!(world.snapshot(), before);
                assert!(Player::new(&mut world).feed_animal(1, bread, 1).unwrap());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -2
                );
                assert_eq!(
                    Player::new(&mut world)
                        .view()
                        .unwrap()
                        .animals
                        .iter()
                        .find(|a| a.id == 1)
                        .unwrap()
                        .affinity,
                    Some(3)
                );
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].hunger,
                    5
                );
                assert!(!Player::new(&mut world).feed_animal(1, grain, 2).unwrap());
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -2
                );
                assert_eq!(
                    world.snapshot().fauna.as_ref().unwrap().animals[0].hunger,
                    6
                );
                assert!(
                    Player::new(&mut world)
                        .transfer_stock(grain, 2, true)
                        .unwrap()
                );
                let saved = world.snapshot();
                let hunger = saved.fauna.as_ref().unwrap().animals[0].hunger;
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(Player::new(&mut world).feed_animal(1, grain, 2).unwrap());
                assert!(Player::new(&mut restored).feed_animal(1, grain, 2).unwrap());
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(
                    state.fauna.as_ref().unwrap().animals[0].opinions[0].affinity,
                    -4
                );
                assert_eq!(
                    Player::new(&mut world)
                        .view()
                        .unwrap()
                        .animals
                        .iter()
                        .find(|a| a.id == 1)
                        .unwrap()
                        .affinity,
                    Some(6)
                );
                assert_eq!(state.fauna.as_ref().unwrap().animals[0].hunger, hunger - 1);
                assert!(
                    !Player::new(&mut world)
                        .view()
                        .unwrap()
                        .possessions
                        .iter()
                        .any(|lot| lot.good == grain)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_wildlife_uses_named_food_and_exposes_only_local_observations() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-wildlife.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let species = &initial.fauna.as_ref().unwrap().species[0];
                assert_eq!(
                    initial
                        .physical
                        .goods
                        .iter()
                        .find(|g| g.id == species.food_good)
                        .unwrap()
                        .name,
                    "grain"
                );
                let view = Player::new(&mut world).view().unwrap();
                assert_eq!(
                    view.animals
                        .iter()
                        .map(|animal| animal.id)
                        .collect::<Vec<_>>(),
                    vec![1, 2]
                );
                let mut altered = initial.clone();
                let fauna = altered.fauna.as_mut().unwrap();
                fauna.animals[0].genome.alleles[0] = [999, 999];
                let father = fauna.animals[1].genome.clone();
                fauna.animals[0].pregnancy = Some(game_economy::AnimalPregnancy {
                    father: 2,
                    father_genome: father,
                    due: 3,
                });
                let mut other = game_economy::Economy::compile(altered).unwrap();
                assert_eq!(
                    Player::new(&mut other).view().unwrap().animals,
                    view.animals
                );
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state
                        .gathering
                        .as_ref()
                        .unwrap()
                        .nodes
                        .iter()
                        .find(|n| n.id == 3)
                        .unwrap()
                        .available,
                    9
                );
                assert_eq!(state.fauna.as_ref().unwrap().animals[2].hunger, 2);
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                state
            })
    };
    assert_eq!(run(1), run(4));
}

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
                    &root.join("content/scenarios/economy-generated.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let before = world.snapshot();
                assert_eq!(before.people.len(), 129);
                let reaction = before
                    .opinions
                    .as_ref()
                    .unwrap()
                    .sensitivities
                    .iter()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .opposed_action_reaction
                    .as_ref()
                    .unwrap();
                assert_eq!(reaction.blocked.unwrap().affinity, 1);
                let other = before
                    .opinions
                    .as_ref()
                    .unwrap()
                    .sensitivities
                    .iter()
                    .find(|row| row.person == 4)
                    .unwrap()
                    .opposed_action_reaction
                    .as_ref()
                    .unwrap();
                assert!(std::sync::Arc::ptr_eq(reaction, other));
                assert!(
                    before
                        .opinions
                        .as_ref()
                        .unwrap()
                        .sensitivities
                        .iter()
                        .find(|row| row.person == 3)
                        .unwrap()
                        .opposed_action_reaction
                        .is_none()
                );
                let mut individual = reaction.clone();
                std::sync::Arc::make_mut(&mut individual)
                    .blocked
                    .as_mut()
                    .unwrap()
                    .affinity = -4;
                assert_eq!(other.blocked.unwrap().affinity, 1);
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
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert_eq!(after, restored.snapshot());
                after
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn campaign_loads_its_per_strain_combat_transmission_rule() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    assert_eq!(
        state.dynamics.unwrap().disease.unwrap().strains[0].combat_transmission_per_million,
        500_000
    );
}

#[test]
fn seed_override_controls_both_population_and_simulation_seed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("content/scenarios/economy-generated.toml");
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
        &root.join("content/scenarios/economy-generated.toml"),
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
                    &root.join("content/scenarios/economy-generated.toml"),
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
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
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
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                    Some(42),
                    Some(&root.join("content/scenarios/campaign-population.toml")),
                )
                .unwrap();
                let initial = world.snapshot();
                assert_eq!(initial.people.len(), 132);
                assert_eq!(initial.family.as_ref().unwrap().next_id, 1128);
                let work = initial
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert_eq!(work.preferences.len(), 129);
                assert!(
                    work.preferences
                        .iter()
                        .filter(|row| row.person >= 1000)
                        .all(|row| row.trust_weight == 1)
                );
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
                assert_eq!(social.participants.len(), 132);
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
                    let mut player = game_economy_player::Player::new(&mut world);
                    let view = player.view().unwrap();
                    assert_eq!(view.person.unwrap().id, 1);
                    assert!(view.nearby.iter().all(|id| !remote.contains(id)));
                    player.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(
                    state.agriculture.as_ref().unwrap().fields[0].phase,
                    game_economy::CropPhase::Growing
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

fn generated_campaign(count: u64) -> game_economy::Economy {
    static NEXT_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = std::fs::read_to_string(root.join("content/scenarios/campaign-population.toml"))
        .unwrap()
        .replace("count = 128", &format!("count = {count}"));
    let path = std::env::temp_dir().join(format!(
        "legacy-campaign-population-{}-{count}-{}.toml",
        std::process::id(),
        NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, profile).unwrap();
    let world = super::scenario_with_options(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
        Some(42),
        Some(&path),
    );
    std::fs::remove_file(path).unwrap();
    world.unwrap()
}

#[test]
fn large_campaign_commits_and_reloads_identically_across_workers() {
    let initial = generated_campaign(20_000).snapshot();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
                let stale = world.prepare().unwrap();
                world.advance().unwrap();
                let committed = world.snapshot();
                assert!(world.commit(stale).is_err());
                assert_eq!(world.snapshot(), committed);
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&world.snapshot()).unwrap())
                        .unwrap(),
                )
                .unwrap();
                for _ in 0..2 {
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
                let work = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && work.preferences.iter().any(|p| p.person == member.id)
                }));
                let dreams = state.dreams.as_ref().unwrap();
                assert!(state.family.as_ref().unwrap().members.iter().any(|member| {
                    member.born.is_some() && dreams.learning.iter().any(|p| p.person == member.id)
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
        .push(game_economy::Opinion {
            observer: target,
            target: requester,
            trust: 0,
            fear: 10,
            obligation: 0,
            affinity: 0,
        });
    let mut world = game_economy::Economy::compile(initial).unwrap();
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
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
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
                            == game_lots::Place::Stock {
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
        .find(|c| c.place == game_lots::Place::Person(1))
        .unwrap();
    *requester = game_lots::Container::new(requester.place, requester.capacity, vec![]).unwrap();
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
        .push(game_economy::Opinion {
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
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
                let before = world.snapshot();
                let bread = before
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                let amount = |state: &game_economy::State, person| -> u64 {
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == game_lots::Place::Person(person))
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
                    .push(game_economy::Opinion {
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
                let mut world = game_economy::Economy::compile(state).unwrap();
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
                        game_autowork::requests::Outcome::Refused
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
                        game_autowork::requests::Outcome::Started { .. }
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
fn generated_work_learning_is_an_individual_inclination_not_an_aspiration() {
    let initial = generated_campaign(128).snapshot();
    let dreams = initial.dreams.as_ref().unwrap();
    assert!(dreams.inherit_births);
    assert!(dreams.learning.iter().any(|row| row.person == 2));
    assert!(dreams.learning.iter().any(|row| row.person >= 1000));
    assert!(dreams.learning.iter().all(|row| row.max_aspirations == 3));
    let learner = dreams
        .learning
        .iter()
        .find(|row| {
            row.person >= 1000 && !dreams.people.iter().any(|dream| dream.person == row.person)
        })
        .unwrap()
        .person;
    let restored: game_economy::State =
        serde_json::from_str(&serde_json::to_string(&initial).unwrap()).unwrap();
    assert!(
        restored
            .dreams
            .unwrap()
            .learning
            .iter()
            .any(|row| row.person == learner)
    );
    assert_eq!(generated_campaign(128).snapshot(), initial);
}

#[test]
fn authored_aspiration_learning_overrides_a_generated_disposition() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let contents =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let path = std::env::temp_dir().join(format!(
        "legacy-aspiration-override-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, format!("{contents}\n[[aspiration_learning]]\nperson = 1000\nmax_aspirations = 1\npriority = 17\nbatch = 2\nreview_interval = 5\n")).unwrap();
    let world = super::scenario_with_options(
        &path,
        &root.join("content"),
        Some(42),
        Some(&root.join("content/scenarios/campaign-population.toml")),
    );
    std::fs::remove_file(path).unwrap();
    let state = world.unwrap().snapshot();
    let learning: Vec<_> = state
        .dreams
        .as_ref()
        .unwrap()
        .learning
        .iter()
        .filter(|row| row.person == 1000)
        .collect();
    assert_eq!(learning.len(), 1);
    assert_eq!(
        (
            learning[0].max_aspirations,
            learning[0].priority,
            learning[0].batch
        ),
        (1, 17, 2)
    );
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
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
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
    let meat = initial
        .physical
        .goods
        .iter()
        .find(|g| g.name == "raw-meat")
        .unwrap()
        .id;
    assert!(
        initial
            .giving
            .as_ref()
            .unwrap()
            .responses
            .iter()
            .filter(|response| response.person >= 1000)
            .all(|response| response.alternatives == [meat])
    );
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
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
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
                    .find(|c| c.place == game_lots::Place::Person(target))
                    .unwrap();
                *holder = game_lots::Container::new(holder.place, holder.capacity, vec![]).unwrap();
                let mut world = game_economy::Economy::compile(state).unwrap();
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
                    .find(|c| c.place == game_lots::Place::Person(actor))
                    .unwrap();
                *holder = game_lots::Container::new(holder.place, holder.capacity, vec![]).unwrap();
                let mut world = game_economy::Economy::compile(state).unwrap();
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

#[test]
fn population_profiles_seed_personal_acquaintance_preferences() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = std::fs::read_to_string(root.join("content/scenarios/campaign-population.toml"))
        .unwrap()
        .replace(
            "prefer_affinity = true",
            "prefer_affinity = true\nmax_affinity = -1\naffinity_weight = -2\navoid_last = false",
        );
    let path = std::env::temp_dir().join(format!(
        "legacy-personal-acquaintance-profile-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, profile).unwrap();
    let world = super::scenario_with_options(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
        Some(42),
        Some(&path),
    )
    .unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let rows = &state.giving.as_ref().unwrap().acquaintance_asking;
    assert!(!rows.is_empty());
    assert!(rows.iter().all(
        |row| (row.max_affinity, row.affinity_weight, row.avoid_last)
            == (Some(-1), Some(-2), Some(false))
    ));
}

fn hungry_generated_campaign(count: u64) -> game_economy::State {
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
        if matches!(holder.place, game_lots::Place::Person(id) if id >= 1000) {
            *holder = game_lots::Container::new(holder.place, holder.capacity, vec![]).unwrap();
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
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
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
            #[cfg(feature = "profile")]
            let before = game_diagnostics::snapshot();
            let mut samples = Vec::new();
            for round in 0..4 {
                let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
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
            #[cfg(feature = "profile")]
            if std::env::var_os("LEGACY_PROFILE").is_some() {
                for (name, measurement) in game_diagnostics::snapshot() {
                    let prior = before.get(name);
                    let calls = measurement.calls - prior.map_or(0, |m| m.calls);
                    let nanos = measurement.nanoseconds - prior.map_or(0, |m| m.nanoseconds);
                    if calls > 0 {
                        println!(
                            "phase={name} calls={calls} mean_ms={:.3}",
                            nanos as f64 / calls as f64 / 1_000_000.0
                        );
                    }
                }
            }
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
    #[cfg(feature = "profile")]
    if std::env::var_os("LEGACY_PROFILE").is_some() {
        for (name, measurement) in game_diagnostics::snapshot() {
            if (name.starts_with("economy.") || name.starts_with("family."))
                && measurement.calls > 0
            {
                println!(
                    "phase={name} calls={} mean_ms={:.3}",
                    measurement.calls,
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        }
    }
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
                    &root.join("content/scenarios/economy.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let first = world.advance().unwrap();
                assert_eq!(first.started.len(), 1);
                assert_eq!(world.snapshot().goals.as_ref().unwrap().people.len(), 1);
                let state = world.snapshot();
                let field = &state.agriculture.as_ref().unwrap().fields[0];
                assert_eq!(field.phase, game_economy::CropPhase::Growing);
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
                            == game_lots::Place::Stock {
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
                    &root.join("content/scenarios/economy-campaign.toml"),
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
                let newborn = saved
                    .family
                    .as_ref()
                    .unwrap()
                    .members
                    .iter()
                    .find(|member| member.born == Some(3))
                    .unwrap()
                    .id;
                assert!(
                    saved
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == newborn && p.enabled)
                );
                assert!(
                    saved
                        .gathering
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .any(|p| p.person == newborn && p.pressured_by.is_none())
                );
                let newborn_purpose = saved
                    .purpose
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|p| p.person == newborn)
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
                        .any(|p| p.person == newborn)
                );
                assert!(
                    saved
                        .knowledge
                        .as_ref()
                        .unwrap()
                        .minds
                        .iter()
                        .any(|mind| mind.person == newborn)
                );
                assert!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == game_lots::Place::Person(newborn))
                );
                game_economy::Economy::compile(saved.clone()).unwrap();
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn campaign_guardians_share_people_travel_and_player_encounters() {
    use game_economy::ReleaseDecision;
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let initial = Player::new(&mut world).guardians().unwrap();
    assert_eq!(
        initial.iter().map(|g| g.person).collect::<Vec<_>>(),
        vec![99, 100]
    );
    assert!(initial.iter().all(|g| g.region == Some(2)));
    for _ in 0..4 {
        Player::new(&mut world).advance().unwrap();
    }
    let arrived = Player::new(&mut world).guardians().unwrap();
    assert!(
        arrived
            .iter()
            .all(|g| g.region == Some(1) && g.journey.is_none())
    );
    assert!(Player::new(&mut world).meet(99).unwrap());
    for _ in 0..3 {
        Player::new(&mut world).advance().unwrap();
    }
    assert_eq!(
        Player::new(&mut world).release_guardian(99).unwrap(),
        ReleaseDecision::Accepted
    );
    assert_eq!(
        Player::new(&mut world).release_guardian(100).unwrap(),
        ReleaseDecision::NotReady
    );
    let state = world.snapshot();
    let guardians = Player::new(&mut world).guardians().unwrap();
    for guardian in guardians {
        let body = state
            .people
            .iter()
            .find(|p| p.id == guardian.person)
            .unwrap();
        assert_eq!(
            (guardian.region, guardian.health),
            (body.region, body.health)
        );
    }
}

#[test]
fn player_can_set_a_delivery_objective_through_the_cli() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let id = Player::new(&mut world).start_expedition(2, 5).unwrap();
    let good = world.snapshot().physical.goods[0].id;
    assert!(
        game_economy_cli_commands::player::handle(
            &mut world,
            &[
                "deliver-expedition",
                &id.to_string(),
                "2",
                &good.to_string(),
                "3"
            ]
        )
        .unwrap()
    );
    let objective =
        serde_json::to_value(world.snapshot().missions.unwrap().entries[0].objective).unwrap();
    assert_eq!(objective["kind"], "deliver");
    assert_eq!(objective["person"], 2);
    assert_eq!(objective["good"], good);
    assert_eq!(objective["quantity"], 3);
    assert_eq!(objective["delivered"], 0);
    assert!(
        game_economy_cli_commands::player::handle(
            &mut world,
            &["delivery-choice", &id.to_string(), "off", "2", "3"]
        )
        .unwrap()
    );
    let preference = world.snapshot().missions.unwrap().entries[0]
        .delivery_preference
        .unwrap();
    assert!(!preference.willing);
    assert_eq!(preference.reserve, 2);
    assert_eq!(preference.pressure_threshold, Some(3));
    assert!(
        game_economy_cli_commands::player::handle(
            &mut world,
            &[
                "delivery-acquisition",
                &id.to_string(),
                "off",
                "ignore",
                "4"
            ]
        )
        .unwrap()
    );
    assert!(
        game_economy_cli_commands::handle(
            &mut world,
            &[
                "expedition-delivery-choice",
                "1",
                &id.to_string(),
                "on",
                "0",
                "none"
            ]
        )
        .unwrap()
    );
    let preference = world.snapshot().missions.unwrap().entries[0]
        .delivery_preference
        .unwrap();
    assert!(preference.willing);
    assert_eq!(preference.pressure_threshold, None);
    let acquisition = preference.acquisition.unwrap();
    assert!(!acquisition.willing);
    assert!(acquisition.disregard_claims);
    assert_eq!(acquisition.pressure_threshold, Some(4));
    let error = game_economy_cli_commands::player::handle(
        &mut world,
        &[
            "delivery-acquisition",
            &id.to_string(),
            "on",
            "respect",
            "none",
            "999",
        ],
    )
    .unwrap_err();
    assert!(error.contains("known local store"));
}

#[test]
fn player_can_offer_and_request_lessons() {
    use game_economy_player::Player;
    use game_economy_skills::{Record, TeacherPolicy};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    for (command, teacher, student) in [("teach", 1, 2), ("learn", 2, 1)] {
        let mut state = original.clone();
        let skills = state.skills.as_mut().unwrap();
        let rule = &skills.rules[0];
        let recipe = rule.recipe;
        let cap = rule.threshold(rule.max_level);
        skills
            .records
            .retain(|record| !(record.person == teacher && record.recipe == recipe));
        skills.records.push(Record {
            person: teacher,
            recipe,
            completions: cap,
            credited_jobs: vec![],
        });
        skills
            .teachers
            .retain(|policy| !(policy.person == teacher && policy.recipe == recipe));
        skills.teachers.push(TeacherPolicy {
            person: teacher,
            recipe,
            enabled: true,
            eligible_at: 0,
        });
        let mut world = game_economy::Economy::compile(state).unwrap();
        let before = world.snapshot();
        let own = Player::new(&mut world).skills().unwrap();
        let row = own.iter().find(|row| row.recipe == recipe).unwrap();
        assert_eq!(row.work_credits, if teacher == 1 { cap } else { 0 });
        assert_eq!(row.lesson_credits, 0);
        assert!(game_economy_cli_commands::player::handle(&mut world, &["skills"]).unwrap());
        assert_eq!(world.snapshot(), before);
        assert!(
            game_economy_cli_commands::player::handle(
                &mut world,
                &["teaching-feeling", "received", "-1", "2", "0", "0"]
            )
            .unwrap()
        );
        let reaction = Player::new(&mut world)
            .teaching_reaction()
            .unwrap()
            .unwrap();
        assert_eq!(reaction.received.unwrap().affinity, 2);
        assert!(
            game_economy_cli_commands::player::handle(
                &mut world,
                &["teaching-feeling", "received", "off"]
            )
            .unwrap()
        );
        assert!(
            Player::new(&mut world)
                .teaching_reaction()
                .unwrap()
                .is_none()
        );
        let recipe_text = recipe.to_string();
        assert!(
            game_economy_cli_commands::player::handle(&mut world, &[command, "2", &recipe_text])
                .unwrap()
        );
        let state = world.snapshot();
        assert!(
            state
                .skills
                .unwrap()
                .lessons
                .iter()
                .any(|lesson| lesson.teacher == teacher
                    && lesson.student == student
                    && lesson.recipe == recipe)
        );
        if student == 1 {
            assert_eq!(
                Player::new(&mut world)
                    .skills()
                    .unwrap()
                    .iter()
                    .find(|row| row.recipe == recipe)
                    .unwrap()
                    .lesson_credits,
                1
            );
        }
    }
}

#[test]
fn player_changes_only_their_own_social_affinity_preference() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    assert!(
        game_economy_cli_commands::player::handle(&mut world, &["social-affinity", "-2"]).unwrap()
    );
    assert!(
        game_economy_cli_commands::player::handle(&mut world, &["social-affinity", "2", "0"])
            .unwrap()
    );
    assert_eq!(
        Player::new(&mut world)
            .social_affinity_weight(None)
            .unwrap(),
        Some(-2)
    );
    assert_eq!(
        Player::new(&mut world)
            .social_affinity_weight(Some(2))
            .unwrap(),
        Some(0)
    );
    let before = world.snapshot();
    assert!(
        game_economy_cli_commands::player::handle(&mut world, &["social-affinity", "999", "1"])
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    assert!(
        game_economy_cli_commands::player::handle(&mut world, &["social-affinity", "99", "1"])
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    assert!(
        game_economy_cli_commands::player::handle(&mut world, &["social-affinity", "2", "default"])
            .unwrap()
    );
    assert_eq!(
        Player::new(&mut world)
            .social_affinity_weight(Some(2))
            .unwrap(),
        None
    );
    assert_eq!(
        Player::new(&mut world)
            .social_affinity_weight(None)
            .unwrap(),
        Some(-2)
    );
}

#[test]
fn authored_teaching_reactions_remain_individual_and_directional() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[[opinions.sensitivities]]\nperson = 1\nteaching_reaction = {{ received = {{ affinity = 3, obligation = 0 }} }}\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nteaching_reaction = {{ received = {{ affinity = -4 }} }}\n"
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-teaching-reaction-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let sensitivity = state
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|row| row.person == 1)
        .unwrap();
    assert_eq!(
        sensitivity
            .teaching_reaction
            .as_ref()
            .unwrap()
            .received
            .unwrap()
            .affinity,
        3
    );
    assert_eq!(
        sensitivity
            .teaching_reaction
            .as_ref()
            .unwrap()
            .received
            .unwrap()
            .obligation,
        0
    );
    assert_eq!(
        sensitivity.relationship_cases[0]
            .teaching_reaction
            .as_ref()
            .unwrap()
            .received
            .unwrap()
            .affinity,
        -4
    );
}

#[test]
fn player_can_take_from_an_incapacitated_guardian_who_later_recovers() {
    use game_economy_player::Player;
    use game_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let bread = state
        .physical
        .goods
        .iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    let guardian = state
        .people
        .iter_mut()
        .find(|person| person.id == 99)
        .unwrap();
    guardian.region = Some(1);
    guardian.health = 0;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|container| container.place == Place::Person(99))
        .unwrap();
    *holder = Container::new(
        holder.place,
        holder.capacity,
        vec![Lot {
            item: None,
            good: bread,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = game_economy::Economy::compile(state).unwrap();
    assert!(
        Player::new(&mut world)
            .view()
            .unwrap()
            .bodies
            .iter()
            .any(|body| body.person == 99)
    );
    assert!(
        Player::new(&mut world)
            .recover_possessions(99, bread, 1)
            .unwrap()
    );
    let state = world.snapshot();
    assert_eq!(
        state
            .people
            .iter()
            .find(|person| person.id == 99)
            .unwrap()
            .health,
        5
    );
    assert_eq!(
        state
            .physical
            .custody
            .containers
            .iter()
            .find(|container| container.place == Place::Person(99))
            .unwrap()
            .used,
        0
    );
}

#[test]
fn player_can_confine_and_release_a_guardian_using_the_authored_cell() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/scenarios/economy-confinement.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let guardian = state
        .people
        .iter_mut()
        .find(|person| person.id == 99)
        .unwrap();
    guardian.region = Some(1);
    guardian.health = 0;
    let mut world = game_economy::Economy::compile(state).unwrap();
    assert!(Player::new(&mut world).confine_person(99, 1).unwrap());
    let guardian = Player::new(&mut world)
        .guardians()
        .unwrap()
        .into_iter()
        .find(|g| g.person == 99)
        .unwrap();
    assert_eq!((guardian.health, guardian.confined), (2, Some(true)));
    assert!(Player::new(&mut world).guard_cell(1).unwrap());
    assert_eq!(
        world.snapshot().confinement.as_ref().unwrap().cells[0].resistance,
        6
    );
    assert!(Player::new(&mut world).release_held_person(99).unwrap());
    assert_eq!(
        Player::new(&mut world)
            .guardians()
            .unwrap()
            .into_iter()
            .find(|g| g.person == 99)
            .unwrap()
            .confined,
        Some(false)
    );
}

#[test]
fn a_real_construction_can_create_a_new_physical_barrier() {
    use game_economy_player::Player;
    use game_lots::{Container, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let initial = scenario(
        &root.join("content/scenarios/economy-confinement.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let kit = initial
        .physical
        .goods
        .iter()
        .find(|good| good.name == "reed-shelter-kit")
        .unwrap()
        .id;
    let mut missing = initial.clone();
    let stock = missing
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| {
            c.place
                == Place::Stock {
                    region: 1,
                    good: kit,
                }
        })
        .unwrap();
    *stock = Container::new(stock.place, stock.capacity, vec![]).unwrap();
    let mut world = game_economy::Economy::compile(missing).unwrap();
    let before = world.snapshot();
    assert!(Player::new(&mut world).build_site(1).is_err());
    assert_eq!(world.snapshot(), before);
    let mut world = game_economy::Economy::compile(initial).unwrap();
    assert!(Player::new(&mut world).build_site(1).unwrap());
    let state = world.snapshot();
    assert!(
        state
            .work_sites
            .as_ref()
            .unwrap()
            .sites
            .iter()
            .any(|site| site.id == 2 && site.kind == "cell")
    );
    assert!(
        state
            .confinement
            .as_ref()
            .unwrap()
            .cells
            .iter()
            .any(|cell| cell.site == 2 && cell.resistance == 6 && cell.allow_supplies)
    );
    assert!(
        state
            .physical
            .custody
            .containers
            .iter()
            .any(|container| container.place == Place::Site(2) && container.used == 1)
    );
    game_economy::Economy::compile(state).unwrap();
}

#[test]
fn player_can_be_captured_by_an_individual_and_choose_to_escape() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-confinement.toml"),
        &root.join("content"),
    )
    .unwrap();
    Player::new(&mut world).set_capture_yield(false).unwrap();
    Player::new(&mut world)
        .set_confinement_reaction(
            game_economy::ConfinementExperienceKind::Captured,
            Some(game_economy::SubjectiveReaction {
                trust: -1,
                affinity: 3,
                fear: 0,
                obligation: 0,
            }),
        )
        .unwrap();
    Player::new(&mut world).advance().unwrap();
    let view = Player::new(&mut world).view().unwrap();
    assert!(view.held);
    let opinion = view
        .relationships
        .iter()
        .find(|row| row.target == 2)
        .unwrap();
    assert_eq!(
        (
            opinion.trust,
            opinion.affinity,
            opinion.fear,
            opinion.obligation
        ),
        (-1, 3, 0, 0)
    );
    assert!(Player::new(&mut world).depart(2).is_err());
    assert!(!Player::new(&mut world).escape_confinement().unwrap());
    assert!(Player::new(&mut world).escape_confinement().unwrap());
    assert!(!Player::new(&mut world).view().unwrap().held);
    Player::new(&mut world).depart(2).unwrap();
}

#[test]
fn ruler_view_excludes_remote_people_stock_and_other_minds() {
    use game_economy_knowledge::Record;
    use game_economy_player::Player;
    use game_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
        .push(game_economy::Opinion {
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
            item: None,
            good,
            quantity: 7,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut first = game_economy::Economy::compile(baseline).unwrap();
    let mut second = game_economy::Economy::compile(hidden).unwrap();
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
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    let mut world = game_economy::Economy::compile(state).unwrap();
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
fn player_can_offer_more_than_they_own_and_make_a_separate_work_request() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-work-practices.toml"),
        &root.join("content"),
    )
    .unwrap();
    game_economy_cli_commands::player::handle(
        &mut world,
        &["work-outcome", "2", "-1", "2", "4", "-5", "6", "default"],
    )
    .unwrap();
    game_economy_cli_commands::player::handle(
        &mut world,
        &["work-outcome-feelings", "received", "3", "4"],
    )
    .unwrap();
    game_economy_cli_commands::player::handle(
        &mut world,
        &["work-outcome-feelings", "partial", "default", "-2"],
    )
    .unwrap();
    assert!(
        game_economy_cli_commands::player::handle(
            &mut world,
            &["work-outcome-feelings", "missing", "default", "3"]
        )
        .is_err()
    );
    let mut player = Player::new(&mut world);
    let configured = player
        .view()
        .unwrap()
        .practices
        .unwrap()
        .work
        .unwrap()
        .outcome_policy
        .unwrap();
    assert_eq!(
        (
            configured.received_fear,
            configured.received_obligation,
            configured.partial_fear,
            configured.partial_obligation,
            configured.missing_obligation
        ),
        (3, 4, None, Some(-2), 0)
    );
    assert_eq!(
        (
            configured.missing_trust,
            configured.received_affinity,
            configured.partial_trust,
            configured.partial_affinity
        ),
        (-1, -5, Some(6), None)
    );
    player
        .set_work_outcome_policy(Some(game_economy::WorkOutcomePolicy {
            wait_ticks: 2,
            missing_trust: 1,
            missing_affinity: -2,
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .outcome_policy
            .unwrap()
            .missing_affinity,
        -2
    );
    player.set_work_outcome_policy(None).unwrap();
    player
        .set_work_practice_evaluation(Some(game_economy::WorkPracticeEvaluation {
            base: 1,
            affinity_weight: -2,
            trust_weight: 3,
            threshold: 4,
        }))
        .unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .evaluation
            .unwrap()
            .affinity_weight,
        -2
    );
    player.set_work_practice_choice("approve", false).unwrap();
    assert!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .evaluation
            .is_none()
    );
    let view = player.view().unwrap();
    let bread = view.possessions[0].good;
    let offered = view.possessions[0].quantity + 100;
    let recipe = player
        .catalog()
        .unwrap()
        .1
        .into_iter()
        .find(|recipe| recipe.name == "fixture-tool")
        .unwrap()
        .id;
    assert!(
        player
            .propose_work_practice(2, recipe, bread, offered, 12)
            .unwrap()
    );
    let view = player.view().unwrap();
    assert_eq!(
        view.practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .authored
            .as_ref()
            .unwrap()
            .quantity,
        offered
    );
    player.advance().unwrap();
    let request = player.ask_work(2, recipe, 8).unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .as_ref()
            .unwrap()
            .work
            .as_ref()
            .unwrap()
            .engagement
            .as_ref()
            .unwrap()
            .request,
        request
    );
}

#[test]
fn player_can_choose_signed_promise_trust_reactions() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    assert_eq!(player.promise_trust_reactions().unwrap(), (None, None));
    player.set_promise_trust_reactions(Some((-2, 3))).unwrap();
    assert_eq!(
        player.promise_trust_reactions().unwrap(),
        (Some(-2), Some(3))
    );
    let state = world.snapshot();
    let mut restored = game_economy::Economy::compile(
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
    )
    .unwrap();
    let mut player = Player::new(&mut restored);
    assert_eq!(
        player.promise_trust_reactions().unwrap(),
        (Some(-2), Some(3))
    );
    player.set_promise_trust_reactions(None).unwrap();
    assert_eq!(player.promise_trust_reactions().unwrap(), (None, None));
}

#[test]
fn player_can_make_a_promise_and_later_choose_real_delivery() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let good = player.view().unwrap().possessions[0].good;
    let id = player.promise(2, good, 1, 8).unwrap();
    player.advance().unwrap();
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_agreements::Status::Active
    );
    assert!(player.fulfill_promise(id).unwrap());
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_agreements::Status::Fulfilled
    );
}

#[test]
fn distrust_can_initiate_tactical_conflict_with_a_controlled_person() {
    use game_economy_combat::{Order, Reaction, Response};
    use game_economy_player::Player;
    use game_opinions::{Confrontation, Opinion};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let initial = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = initial.clone();
                for person in &mut state.people {
                    person.willing_to_work = false;
                    person.seek_meals = false;
                }
                for member in &mut state.family.as_mut().unwrap().members {
                    member.pregnancy = None;
                    member.reproduction_allowed = false;
                }
                let opinions = state.opinions.as_mut().unwrap();
                opinions
                    .records
                    .retain(|row| (row.observer, row.target) != (2, 1));
                opinions.records.push(Opinion {
                    observer: 2,
                    target: 1,
                    trust: -3,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                });
                let sensitivity = opinions
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|p| p.person == 2)
                    .unwrap();
                sensitivity.confrontation = Some(Confrontation {
                    trust_at_most: -2,
                    fear_at_most: 0,
                    interval: 1,
                    enabled: true,
                });
                state.combat.as_mut().unwrap().reactions.push(Reaction {
                    person: 1,
                    response: Response::Fight,
                    retreat_at_or_below: None,
                    retreat_fear_at_least: None,
                    pursue: false,
                });
                let mut world = game_economy::Economy::compile(state).unwrap();
                Player::new(&mut world).rest().unwrap();
                assert!(
                    world
                        .snapshot()
                        .combat
                        .as_ref()
                        .unwrap()
                        .conflict
                        .encounters
                        .is_empty()
                );
                let before = world.snapshot().people[0].health;
                world.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(
                    state.people[0].health, before,
                    "new fights begin after the existing round"
                );
                let combat = state.combat.as_ref().unwrap();
                assert_eq!(combat.conflict.encounters.len(), 1);
                let fight = &combat.conflict.encounters[0];
                assert_eq!(
                    fight.fighters.iter().find(|f| f.person == 1).unwrap().order,
                    Order::Defend
                );
                assert!(matches!(
                    fight.fighters.iter().find(|f| f.person == 2).unwrap().order,
                    Order::Attack { target: 1 }
                ));
                assert_eq!(combat.tactics.battles[0].encounter, fight.id);
                assert!(Player::new(&mut world).view().unwrap().battle.is_some());
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert!(world.snapshot().people[0].health < before);
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn player_fights_on_the_authored_generated_map() {
    use game_economy_combat::Order;
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
fn controlled_worker_can_receive_and_independently_respond_to_real_work_requests() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let recipe = Player::new(&mut world)
                    .catalog()
                    .unwrap()
                    .1
                    .into_iter()
                    .find(|r| r.name == "fixture-tool")
                    .unwrap()
                    .id;
                let declined = world.ask_work(2, 1, recipe, 15).unwrap();
                {
                    let mut player = Player::new(&mut world);
                    assert!(
                        player
                            .view()
                            .unwrap()
                            .pending_work_requests
                            .iter()
                            .any(|r| r.id == declined)
                    );
                    assert!(!player.respond_work_request(declined, false).unwrap());
                    assert!(
                        !player
                            .view()
                            .unwrap()
                            .pending_work_requests
                            .iter()
                            .any(|r| r.id == declined)
                    );
                }
                let request = world.ask_work(2, 1, recipe, 15).unwrap();
                let saved = world.snapshot();
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(
                    Player::new(&mut world)
                        .respond_work_request(request, true)
                        .unwrap()
                );
                assert!(
                    Player::new(&mut restored)
                        .respond_work_request(request, true)
                        .unwrap()
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                let state = world.snapshot();
                let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert!(
                    requests
                        .work_links
                        .iter()
                        .any(|link| link.request == request && link.worker == 1)
                );
                assert!(!requests.pending.iter().any(|row| row.id == request));
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn controlled_work_response_requires_a_real_admitted_start() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    let recipe = state
        .physical
        .recipes
        .iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    let tool = state
        .physical
        .goods
        .iter()
        .find(|g| g.name == "tool")
        .unwrap()
        .id;
    state.work_tools.push(game_economy::WorkTool {
        recipe,
        good: tool,
        quantity: 100_000,
    });
    let mut world = game_economy::Economy::compile(state).unwrap();
    let request = world.ask_work(2, 1, recipe, 15).unwrap();
    let before = world.snapshot();
    assert!(world.respond_work_request(2, request, true).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(
        !Player::new(&mut world)
            .respond_work_request(request, true)
            .unwrap()
    );
    let state = world.snapshot();
    let requests = state.autowork.as_ref().unwrap().requests.as_ref().unwrap();
    assert!(requests.pending.iter().any(|row| row.id == request));
    assert!(!requests.work_links.iter().any(|row| row.request == request));
}

#[test]
fn player_request_can_be_refused_or_followed_from_personal_fear() {
    use game_economy_combat::Order;
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = world.snapshot();
    for member in &mut initial.family.as_mut().unwrap().members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    let mut world = game_economy::Economy::compile(initial).unwrap();
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
fn player_view_does_not_reveal_an_unobserved_infection_or_its_remedy() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let healthy = world.snapshot();
    let mut infected = healthy.clone();
    infected
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut healthy = game_economy::Economy::compile(healthy).unwrap();
    let mut infected = game_economy::Economy::compile(infected).unwrap();
    let healthy_view = Player::new(&mut healthy).view().unwrap();
    let infected_view = Player::new(&mut infected).view().unwrap();
    assert!(
        healthy_view.treatment.is_some(),
        "the player may see their own selected medicine"
    );
    assert_eq!(
        healthy_view, infected_view,
        "the unseen infection cannot alter the player's view"
    );
}

#[test]
fn player_treats_infection_with_authored_medicine_taken_from_stock() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
        .push(game_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut world = game_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let treatment = player.view().unwrap().treatment.unwrap();
    let (medicine, quantity) = (treatment.medicine, treatment.quantity);
    assert!(player.transfer_stock(medicine, quantity, true).unwrap());
    assert!(player.treat().unwrap());
    assert_eq!(player.view().unwrap().treatment.unwrap(), treatment);
}

#[test]
fn player_can_treat_a_nearby_person_with_carried_medicine() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
        .push(game_disease::Case {
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
    let mut world = game_economy::Economy::compile(state).unwrap();
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
fn medicine_clears_only_its_matching_strain_for_self_and_others() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, other| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                let disease = state.dynamics.as_mut().unwrap().disease.as_mut().unwrap();
                disease.strains[0].infectious_ticks = 100;
                disease.strains[0].transmission_per_million = 0;
                let mut second = disease.strains[0].clone();
                second.id = 2;
                disease.strains.push(second);
                let patient = if other { 2 } else { 1 };
                for strain in [1, 2] {
                    disease.cases.push(game_disease::Case {
                        person: patient,
                        strain,
                        infected_at: 0,
                        source: None,
                    });
                }
                let medicine = state
                    .physical
                    .goods
                    .iter()
                    .find(|good| good.name == "medicine")
                    .unwrap()
                    .id;
                let mut world = game_economy::Economy::compile(state).unwrap();
                assert!(
                    Player::new(&mut world)
                        .transfer_stock(medicine, 1, true)
                        .unwrap()
                );
                if other {
                    assert!(
                        Player::new(&mut world)
                            .treat_other(patient, medicine, 1)
                            .unwrap()
                    );
                } else {
                    assert!(Player::new(&mut world).treat().unwrap());
                }
                let saved = world.snapshot();
                let disease = saved.dynamics.as_ref().unwrap().disease.as_ref().unwrap();
                assert!(
                    !disease
                        .cases
                        .iter()
                        .any(|case| case.person == patient && case.strain == 1)
                );
                assert!(
                    disease
                        .cases
                        .iter()
                        .any(|case| case.person == patient && case.strain == 2)
                );
                let restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(saved, restored.snapshot());
                saved
            })
    };
    for other in [false, true] {
        assert_eq!(run(1, other), run(4, other));
    }
}

#[test]
fn player_can_cultivate_a_visible_field() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let field = player.view().unwrap().fields[0].id;
    assert!(player.farm(field).unwrap());
    assert_eq!(
        player.view().unwrap().fields[0].phase,
        game_economy::CropPhase::Growing
    );
}

#[test]
fn player_rest_advances_time_and_recovers_personal_fatigue() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .fatigue
        .as_mut()
        .unwrap()
        .people
        .push(game_fatigue::Person {
            id: 1,
            fatigue: 10,
            resting: false,
        });
    let mut world = game_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    player.rest().unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.fatigue, Some((6, true)));
}

#[test]
fn player_observes_and_recovers_local_deceased_possessions() {
    use game_economy_player::Player;
    use game_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
            item: None,
            good,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = game_economy::Economy::compile(state).unwrap();
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
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
fn authored_personal_credit_uses_real_help_and_a_nonbinding_promise() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-personal-credit.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let bread = world
                    .snapshot()
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "bread")
                    .unwrap()
                    .id;
                assert!(
                    world
                        .snapshot()
                        .agreements
                        .as_ref()
                        .unwrap()
                        .contracts
                        .is_empty()
                );
                assert!(Player::new(&mut world).give(1, bread, 1).unwrap());
                let saved = world.snapshot();
                assert_eq!(
                    saved.giving.as_ref().unwrap().learning[0].last_helper,
                    Some(3)
                );
                assert!(saved.agreements.as_ref().unwrap().contracts.is_empty());
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                for owner in [&mut world, &mut restored] {
                    owner.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                for owner in [&mut world, &mut restored] {
                    Player::new(owner).decide_promise(1, true).unwrap();
                    for _ in 0..6 {
                        owner.advance().unwrap();
                    }
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                let contracts = &state.agreements.as_ref().unwrap().contracts;
                assert_eq!(contracts.len(), 1);
                assert_eq!(contracts[0].status, game_agreements::Status::Defaulted);
                assert_eq!(contracts[0].delivered, 0);
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn player_can_give_carried_goods_without_creating_a_promise() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    use game_economy_player::Player;
    use game_lots::{Container, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
    let mut world = game_economy::Economy::compile(state).unwrap();
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
    use game_economy::Departure;
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
        &root.join("content/scenarios/economy.toml"),
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
    let source = std::fs::read_to_string(root.join("content/scenarios/economy.toml")).unwrap();
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
    let source = std::fs::read_to_string(root.join("content/scenarios/economy.toml")).unwrap();
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
    let source = std::fs::read_to_string(root.join("content/scenarios/economy.toml")).unwrap();
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
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
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
    let player = game_economy_player::Player::new(&mut world);
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
fn authored_personal_practices_are_visible_only_to_their_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[[giving.practices.minds]]\nperson = 1\ninvent = true\napprove = false\nwilling = false\nask_willing = true\nreporting = \"lie\"\nreport_attitude = \"doubt\"\nunanswered_report_belief = \"believes\"\nrequire_believed_ready = true\nvalue_learning = {{ value = 1, minimum = -4, maximum = 4, received = 2, gave = -1, unanswered = 1 }}\nshortage_threshold = 5\nminimum_shortages = 2\nreview_interval = 3\nproposal_lifetime = 8\n\n[[giving.practices.minds]]\nperson = 2\ninvent = false\napprove = true\nwilling = true\nshortage_threshold = 5\nminimum_shortages = 2\nreview_interval = 3\nproposal_lifetime = 8\n"
    );
    let configured = configured.replacen(
        "shortage_threshold = 5",
        "precedent = { same_person = 3, same_good = 2, quantity_difference = -1, received = 4, gave = -2 }\nshortage_threshold = 5",
        1,
    );
    let configured = configured.replace(
        "report_attitude = \"doubt\"",
        "report_attitude = { trust_at_least = 3 }",
    );
    let configured = configured.replacen(
        "reporting = \"lie\"",
        "reporting = \"lie\"\nproposal_affinity_weight = -3",
        1,
    );
    let configured = configured.replacen(
        "[[medical.policies]]",
        "[[medical.acquaintances]]\nperson = 1\nmedicine = \"medicine\"\nquantity = 1\npatient_health_at_most = 75\nwilling = false\npressure_threshold = 3\nurgency_weight = 2\naffinity_weight = -1\n\n[[medical.policies]]\npatient_health_at_most = 70",
        1,
    );
    let configured = configured.replacen("[[remedies]]", "[[remedies]]\nhealth_at_most = 65", 1);
    let configured = configured.replacen(
        "[[giving.responses]]",
        "[[giving.responses]]\naffinity_at_most = -2\naffinity_weight = -1",
        1,
    );
    let configured = format!(
        "{configured}\n[[giving.acquaintance_asking]]\nperson = 1\nenabled = true\nmin_hunger = 5\nretry_ticks = 2\nrequest_lifetime = 3\nmax_affinity = -1\naffinity_weight = -2\navoid_last = false\n"
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-personal-practices-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        world.snapshot().dynamics.unwrap().treatment[0].health_at_most,
        Some(65)
    );
    assert_eq!(
        world.snapshot().giving.unwrap().responses[0].affinity_at_most,
        Some(-2)
    );
    let asking = world.snapshot().giving.unwrap().acquaintance_asking;
    assert_eq!(
        (
            asking[0].max_affinity,
            asking[0].affinity_weight,
            asking[0].avoid_last
        ),
        (Some(-1), Some(-2), Some(false))
    );
    assert_eq!(
        world.snapshot().medical.unwrap().policies[0].patient_health_at_most,
        Some(70)
    );
    assert_eq!(
        world.snapshot().medical.unwrap().acquaintances[0].affinity_weight,
        -1
    );
    let mut player = game_economy_player::Player::new(&mut world);
    let own = player.view().unwrap().practices.unwrap();
    assert_eq!(own.person, 1);
    assert!(!own.approve && !own.willing && own.ask_willing);
    assert_eq!(own.reporting, game_economy::PracticeReporting::Lie);
    assert_eq!(own.proposal_affinity_weight, -3);
    assert_eq!(
        own.report_attitude,
        game_economy::PracticeReportAttitude::TrustAtLeast(3)
    );
    assert!(own.require_believed_ready);
    assert_eq!(own.value_learning.as_ref().unwrap().value, 1);
    assert_eq!(own.precedent.as_ref().unwrap().same_person, 3);
    player.set_practice_precedent(None).unwrap();
    assert!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .precedent
            .is_none()
    );
    let invalid = game_economy::PracticeValueLearning {
        value: 1,
        minimum: 2,
        maximum: 0,
        received: 0,
        gave: 0,
        unanswered: 0,
    };
    assert!(player.set_practice_value_learning(Some(invalid)).is_err());
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .value_learning
            .unwrap()
            .value,
        1
    );
    assert_eq!(
        own.unanswered_report_belief,
        Some(game_economy::PracticeBelief::Believes)
    );
    player.set_practice_unanswered_belief(None).unwrap();
    assert_eq!(
        player
            .view()
            .unwrap()
            .practices
            .unwrap()
            .unanswered_report_belief,
        None
    );
    player.set_practice_willingness(true).unwrap();
    assert!(player.view().unwrap().practices.unwrap().willing);
    assert!(player.decide_practice(2, 1, true).is_err());
    assert!(world.personal_practice(2).unwrap().willing);
    assert!(
        game_economy_cli_commands::handle(
            &mut world,
            &[
                "practice-precedent",
                "1",
                "3",
                "2",
                "-1",
                "4",
                "-2",
                "0",
                "0"
            ]
        )
        .unwrap()
    );
    assert_eq!(
        world
            .personal_practice(1)
            .unwrap()
            .precedent
            .as_ref()
            .unwrap()
            .same_person,
        3
    );
    assert!(
        game_economy_cli_commands::handle(&mut world, &["practice-precedent", "1", "off"]).unwrap()
    );
}

#[test]
fn authored_food_misreport_is_personal_and_player_can_change_it() {
    let authored: game_opinions::Sensitivity =
        toml::from_str("person = 1\nreport_reception = { trust_at_least = -3 }").unwrap();
    assert_eq!(
        authored.report_reception,
        Some(game_economy::ReportReception::TrustAtLeast(-3))
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = source.replace("\r\n", "\n").replace(
        "[[knowledge.minds]]\nperson = 1",
        "[[knowledge.minds]]\nperson = 1\nmisreport = { target = 2, food_delta = -7 }\nreporting = { willing = false, pressure_threshold = 5 }",
    );
    let configured = format!(
        "{configured}\n[[opinions.sensitivities]]\nperson = 1\nreport_reception = \"never\"\ntrust_report_discrepancy = -2\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nreport_discrepancy_trust = 3\n"
    );
    let path =
        std::env::temp_dir().join(format!("legacy-food-misreport-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let snapshot = world.snapshot();
    let own = snapshot
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|s| s.person == 1)
        .unwrap();
    assert_eq!(own.trust_report_discrepancy, Some(-2));
    assert_eq!(own.relationship_cases[0].report_discrepancy_trust, Some(3));
    let mut player = game_economy_player::Player::new(&mut world);
    assert_eq!(
        player.food_reporting().unwrap(),
        Some(game_economy::Reporting {
            willing: false,
            pressure_threshold: Some(5),
        })
    );
    player
        .set_food_reporting(Some(game_economy::Reporting {
            willing: true,
            pressure_threshold: None,
        }))
        .unwrap();
    assert_eq!(
        player.food_reporting().unwrap(),
        Some(game_economy::Reporting {
            willing: true,
            pressure_threshold: None,
        })
    );
    assert_eq!(
        player.food_misreport().unwrap(),
        Some(game_economy::Misreport {
            target: Some(2),
            food_delta: -7
        })
    );
    assert_eq!(
        player.food_report_reception().unwrap(),
        Some(game_economy::ReportReception::Never)
    );
    player
        .set_food_report_reception(Some(game_economy::ReportReception::TrustAtLeast(-3)))
        .unwrap();
    assert_eq!(
        player.food_report_reception().unwrap(),
        Some(game_economy::ReportReception::TrustAtLeast(-3))
    );
    player.set_food_misreport(None).unwrap();
    assert_eq!(player.food_misreport().unwrap(), None);
    game_economy_cli_commands::player::handle(&mut world, &["food-reporting", "off", "3"]).unwrap();
    assert_eq!(
        world.food_reporting(1).unwrap(),
        Some(game_economy::Reporting {
            willing: false,
            pressure_threshold: Some(3),
        })
    );
    assert_eq!(world.food_misreport(2).unwrap(), None);
}

#[test]
fn aspiration_help_recognition_is_configurable_and_private_to_the_ruler() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = source.replace("[[aspirations]]", "[[aspirations]]\nrecognize_help = true");
    let path = std::env::temp_dir().join(format!("legacy-goal-help-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let mut world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    for _ in 0..4 {
        if !world.snapshot().goals.as_ref().unwrap().people.is_empty() {
            break;
        }
        world.advance().unwrap();
    }
    let mut state = world.snapshot();
    let other = state
        .goals
        .as_ref()
        .unwrap()
        .people
        .iter()
        .find(|goal| goal.person == 2)
        .unwrap()
        .clone();
    assert!(other.recognize_help);
    let mut own = other.clone();
    own.person = 1;
    state.goals.as_mut().unwrap().people.push(own);
    let mut world = game_economy::Economy::compile(state).unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.goals.len(), 1);
    assert_eq!(view.goals[0].person, 1);
    player
        .set_goal_help_recognition(other.recipe, false)
        .unwrap();
    assert!(!player.view().unwrap().goals[0].recognize_help);
    assert!(
        world
            .snapshot()
            .goals
            .unwrap()
            .people
            .iter()
            .find(|goal| goal.person == 2)
            .unwrap()
            .recognize_help
    );
}

#[test]
fn scenario_loads_a_personal_choice_to_retry_after_an_offer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = source.replace("[work_requests]", "[work_requests]\naskers = [{ person = 2, interval = 6, lifetime = 5, retry_after_offer = true }]");
    let path = std::env::temp_dir().join(format!("legacy-work-retry-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(world.snapshot().autowork.unwrap().requests.unwrap().askers[0].retry_after_offer);
}

#[test]
fn campaign_loads_personal_offers_for_observed_helpers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let preference = &state.agreements.as_ref().unwrap().work_offering[0];
    assert_eq!(preference.person, 2);
    assert!(!preference.willing);
    assert!(preference.after_refusal);
    assert_eq!(
        state
            .physical
            .goods
            .iter()
            .find(|good| good.id == preference.good)
            .unwrap()
            .name,
        "bread"
    );
    let sensitivity = state
        .opinions
        .as_ref()
        .unwrap()
        .sensitivities
        .iter()
        .find(|row| row.person == 2)
        .unwrap();
    assert_eq!(
        (
            sensitivity.affinity_work_success,
            sensitivity.affinity_work_refusal
        ),
        (Some(2), Some(-1))
    );
    let worker = state
        .autowork
        .as_ref()
        .unwrap()
        .requests
        .as_ref()
        .unwrap()
        .preferences
        .iter()
        .find(|p| p.person == 2)
        .unwrap();
    let value = worker.offer_value.as_ref().unwrap();
    assert_eq!(
        (value.good, value.value_per_unit, value.max_quantity),
        (preference.good, 5, 1)
    );
}

#[test]
fn salvage_scenario_runs_through_real_player_and_individual_actions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-salvage.toml"),
        &root.join("content"),
    )
    .unwrap();
    let recovered = {
        let mut player = game_economy_player::Player::new(&mut world);
        let good = player.building_blueprints().unwrap()[0].kit;
        assert!(player.build_site(1).unwrap());
        player.claim_site(1).unwrap();
        assert!(player.destroy_site(1).unwrap());
        assert_eq!(player.view().unwrap().debris[0].site, 1);
        player.salvage_site(1, good, 1).unwrap()
    };
    let state = world.snapshot();
    let events = &state.construction.as_ref().unwrap().salvage_events;
    assert_eq!(events.len(), 2);
    assert_eq!(events.iter().filter(|event| event.recovered).count(), 1);
    assert_eq!(
        events
            .iter()
            .any(|event| event.person == 1 && event.recovered),
        recovered
    );
    assert!(state.construction.as_ref().unwrap().materials.is_empty());
    assert_eq!(
        state.opinions.unwrap().index().affinity(1, 2),
        if recovered { 0 } else { -2 }
    );
}

#[test]
fn player_carries_dated_goods_between_local_stores_over_two_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-salvage.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let before = player.view().unwrap();
    assert_eq!(before.local_stores.len(), 2);
    let good = before.local_stores[0].lots[0].good;
    assert!(player.move_local_store(1, good, 1, false).unwrap());
    let carrying = player.view().unwrap();
    assert!(
        carrying
            .possessions
            .iter()
            .any(|lot| lot.good == good && lot.quantity == 1)
    );
    assert!(carrying.local_stores[1].lots.is_empty());
    assert!(player.move_local_store(2, good, 1, true).unwrap());
    let delivered = player.view().unwrap();
    assert_eq!(delivered.tick, before.tick + 2);
    assert!(
        delivered.local_stores[1]
            .lots
            .iter()
            .any(|lot| lot.good == good && lot.quantity == 1)
    );
}

#[test]
fn ruler_can_build_an_authored_site_from_an_actual_kit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{}\n[work_sites]\nsites = []\nrequirements = []\n[construction]\n[[construction.blueprints]]\nid = 1\nname = \"shelter workshop\"\nkind = \"workshop\"\ncapacity = 1\nkit = \"reed-shelter-kit\"\nquantity = 1\nsalvage = 1\n[[construction.demolition]]\nperson = 2\nsite = 10\nwilling = false\npressured_by = 1\npressure_threshold = 3\n[[construction.goal_builders]]\nperson = 2\nenabled = false\n",
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
    let mut player = game_economy_player::Player::new(&mut world);
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
    let debris = player.view().unwrap().debris;
    assert_eq!(debris.len(), 1);
    assert_eq!(debris[0].site, site);
    let good = debris[0].lots[0].good;
    assert!(player.salvage_site(site, good, 1).unwrap());
    assert!(
        player
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == good)
    );
    assert_eq!(player.view().unwrap().site_claims, vec![site]);
    player.withdraw_site_claim(site).unwrap();
    assert!(player.view().unwrap().site_claims.is_empty());
}

#[test]
fn ruler_can_assert_and_withdraw_personal_claims_without_seeing_other_minds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-campaign.toml"),
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
        let mut player = game_economy_player::Player::new(&mut world);
        let before = player.view().unwrap();
        assert!(before.claims.is_empty());
        player.claim_stock(good).unwrap();
        let after = player.view().unwrap();
        assert_eq!(after.tick, before.tick);
        assert_eq!(
            after.claims,
            vec![game_economy_player::ClaimView { region: 1, good }]
        );
        assert!(player.claim_stock(good).is_err());
    }
    let path =
        std::env::temp_dir().join(format!("legacy-personal-claim-{}.json", std::process::id()));
    save(&world, &path).unwrap();
    let mut restored = load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let mut player = game_economy_player::Player::new(&mut restored);
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
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    let configured = format!(
        "{source}\n[work_sites]\nsites = [{{ id = 7, region = 1, kind = \"workshop\", capacity = 1 }}]\nrequirements = []\n[[dynamics.claims.site_claims]]\nperson = 2\nsite = 7\n[[opinions.sensitivities]]\nperson = 1\naffinity_stock_use = 5\ntrust_stock_use = -3\nsocial_affinity_weight = -1\naffinity_site_use = 3\naffinity_site_destruction = -2\n[[opinions.sensitivities.relationship_cases]]\ntarget = 2\nstock_use = 0\nstock_use_trust = 2\nsocial_affinity_weight = 0\nsite_use = -4\n"
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
    assert_eq!(sensitivity.affinity_stock_use, Some(5));
    assert_eq!(sensitivity.relationship_cases[0].stock_use, Some(0));
    assert_eq!(sensitivity.trust_stock_use, Some(-3));
    assert_eq!(sensitivity.relationship_cases[0].stock_use_trust, Some(2));
    assert_eq!(sensitivity.social_affinity_weight, Some(-1));
    assert_eq!(
        sensitivity.relationship_cases[0].social_affinity_weight,
        Some(0)
    );
    let mut player = game_economy_player::Player::new(&mut world);
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

#[test]
fn authored_workshop_wears_and_player_repairs_with_carried_timber() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-maintenance.toml"),
                    &root.join("content"),
                )
                .unwrap();
                for _ in 0..40 {
                    if world
                        .snapshot()
                        .construction
                        .as_ref()
                        .unwrap()
                        .maintenance
                        .as_ref()
                        .unwrap()
                        .conditions[0]
                        .wear
                        == 1
                    {
                        break;
                    }
                    world.advance().unwrap();
                }
                let worn = world.snapshot();
                assert_eq!(
                    worn.construction
                        .as_ref()
                        .unwrap()
                        .maintenance
                        .as_ref()
                        .unwrap()
                        .conditions[0]
                        .wear,
                    1
                );
                let mut player = game_economy_player::Player::new(&mut world);
                let view = player.view().unwrap();
                let site = view
                    .work_sites
                    .iter()
                    .find(|site| site.site.id == 1)
                    .unwrap();
                assert_eq!(site.maintenance.unwrap().0, 1);
                assert!(player.repair_site(1).unwrap());
                assert_eq!(
                    player.view().unwrap().work_sites[0].maintenance.unwrap().0,
                    0
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_fighter_can_retreat_after_feeling_fear_from_a_real_strike() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-fear-retreat.toml"),
                    &root.join("content"),
                )
                .unwrap();
                game_economy_player::Player::new(&mut world)
                    .attack(3)
                    .unwrap();
                world.advance().unwrap();
                let after_strike = world.snapshot();
                assert_eq!(
                    after_strike
                        .people
                        .iter()
                        .find(|p| p.id == 1)
                        .unwrap()
                        .health,
                    99
                );
                assert_eq!(
                    after_strike
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .find(|o| o.observer == 3 && o.target == 1)
                        .unwrap()
                        .fear,
                    1
                );
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&after_strike).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert_eq!(state.people.iter().find(|p| p.id == 1).unwrap().health, 99);
                assert!(
                    state
                        .combat
                        .as_ref()
                        .unwrap()
                        .conflict
                        .encounters
                        .is_empty()
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_fleece_regrows_and_collection_keeps_the_animal_alive() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-fleece.toml"),
                    &root.join("content"),
                )
                .unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let view = game_economy_player::Player::new(&mut world).view().unwrap();
                let wool = view
                    .animals
                    .iter()
                    .find(|a| a.id == 1)
                    .unwrap()
                    .product
                    .unwrap();
                assert_eq!((wool.1, wool.2), (3, 3));
                assert_eq!(
                    game_economy_player::Player::new(&mut world)
                        .collect_animal_product(1, false)
                        .unwrap(),
                    game_economy::AnimalProductOutcome::Collected
                );
                let state = world.snapshot();
                assert!(
                    state
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .any(|a| a.id == 1 && a.health > 0)
                );
                assert_eq!(
                    state
                        .animal_products
                        .as_ref()
                        .unwrap()
                        .progress
                        .iter()
                        .find(|p| p.animal == 1)
                        .unwrap()
                        .growth,
                    0
                );
                assert_eq!(
                    state
                        .physical
                        .custody
                        .containers
                        .iter()
                        .find(|c| c.place == game_lots::Place::Person(1))
                        .unwrap()
                        .lots
                        .iter()
                        .filter(|lot| lot.good == wool.0)
                        .map(|lot| lot.quantity)
                        .sum::<u64>(),
                    2
                );
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap(),
                )
                .unwrap();
                for _ in 0..3 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                assert_eq!(
                    world
                        .snapshot()
                        .animal_products
                        .as_ref()
                        .unwrap()
                        .progress
                        .iter()
                        .find(|p| p.animal == 1)
                        .unwrap()
                        .growth,
                    3
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn animal_product_refusal_and_capacity_failure_preserve_fleece() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let initial = scenario(
        &root.join("content/scenarios/economy-fleece.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    for refusal in [true, false] {
        let mut state = initial.clone();
        state.animal_products.as_mut().unwrap().rules[0].grow_ticks = 1;
        if refusal {
            state.fauna.as_mut().unwrap().animals[0]
                .handling_response
                .as_mut()
                .unwrap()
                .willing = false;
        } else {
            let holder = state
                .physical
                .custody
                .containers
                .iter_mut()
                .find(|c| c.place == game_lots::Place::Person(1))
                .unwrap();
            let good = holder.lots[0].good;
            *holder = game_lots::Container::new(
                holder.place,
                holder.capacity,
                vec![game_lots::Lot {
                    item: None,
                    good,
                    quantity: holder.capacity,
                    expires_at: None,
                }],
            )
            .unwrap();
        }
        let mut world = game_economy::Economy::compile(state).unwrap();
        world.advance().unwrap();
        let outcome = game_economy_player::Player::new(&mut world)
            .collect_animal_product(1, false)
            .unwrap();
        assert_eq!(
            outcome,
            if refusal {
                game_economy::AnimalProductOutcome::Refused
            } else {
                game_economy::AnimalProductOutcome::NoCapacity
            }
        );
        let state = world.snapshot();
        assert_eq!(
            state
                .animal_products
                .as_ref()
                .unwrap()
                .progress
                .iter()
                .find(|p| p.animal == 1)
                .unwrap()
                .growth,
            1
        );
        assert!(
            state
                .fauna
                .as_ref()
                .unwrap()
                .animals
                .iter()
                .any(|a| a.id == 1 && a.health > 0)
        );
    }
}

#[test]
fn renewable_wool_can_support_another_persons_independent_production() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run =
        |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut world = scenario(
                        &root.join("content/scenarios/economy-fleece-workshop.toml"),
                        &root.join("content"),
                    )
                    .unwrap();
                    let initial = world.snapshot();
                    let good = |name: &str| {
                        initial
                            .physical
                            .goods
                            .iter()
                            .find(|g| g.name == name)
                            .unwrap()
                            .id
                    };
                    let wool = good("raw-wool");
                    let yarn = good("wool-yarn");
                    world.advance().unwrap();
                    world.advance().unwrap();
                    assert!(
                        world
                            .snapshot()
                            .physical
                            .custody
                            .containers
                            .iter()
                            .all(|c| c.place
                                != (game_lots::Place::Stock {
                                    region: 1,
                                    good: yarn
                                })
                                || c.used == 0)
                    );
                    assert_eq!(
                        game_economy_player::Player::new(&mut world)
                            .collect_animal_product(1, false)
                            .unwrap(),
                        game_economy::AnimalProductOutcome::Collected
                    );
                    assert!(
                        game_economy_player::Player::new(&mut world)
                            .transfer_stock(wool, 2, false)
                            .unwrap()
                    );
                    let mut declining = world.snapshot();
                    declining
                        .people
                        .iter_mut()
                        .find(|person| person.id == 2)
                        .unwrap()
                        .willing_to_work = false;
                    let mut declining = game_economy::Economy::compile(declining).unwrap();
                    for _ in 0..10 {
                        declining.advance().unwrap();
                    }
                    assert!(declining.snapshot().physical.custody.containers.iter().all(
                        |c| c.place
                            != (game_lots::Place::Stock {
                                region: 1,
                                good: yarn
                            })
                            || c.used == 0
                    ));
                    let saved = serde_json::to_string(&world.snapshot()).unwrap();
                    let mut restored =
                        game_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                            .unwrap();
                    for _ in 0..30 {
                        if world
                            .snapshot()
                            .physical
                            .custody
                            .containers
                            .iter()
                            .any(|c| {
                                c.place
                                    == (game_lots::Place::Stock {
                                        region: 1,
                                        good: yarn,
                                    })
                                    && c.used > 0
                            })
                        {
                            break;
                        }
                        world.advance().unwrap();
                        restored.advance().unwrap();
                    }
                    let state = world.snapshot();
                    assert_eq!(state, restored.snapshot());
                    assert!(state.physical.custody.containers.iter().any(|c| c.place
                        == (game_lots::Place::Stock {
                            region: 1,
                            good: yarn
                        })
                        && c.used == 1));
                    assert!(
                        state
                            .goals
                            .as_ref()
                            .unwrap()
                            .people
                            .iter()
                            .any(|g| g.person == 2 && g.progress == 1)
                    );
                    assert!(
                        state
                            .fauna
                            .as_ref()
                            .unwrap()
                            .animals
                            .iter()
                            .any(|a| a.id == 1 && a.health > 0)
                    );
                    state
                })
        };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_collect_deposit_and_spin_wool_without_player_orders() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/scenarios/economy-autonomous-fleece.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let initial = world.snapshot();
                let yarn = initial
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "wool-yarn")
                    .unwrap()
                    .id;
                let wool = initial
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "raw-wool")
                    .unwrap()
                    .id;
                let mut collected = false;
                let mut deposited = false;
                for _ in 0..20 {
                    world.advance().unwrap();
                    let state = world.snapshot();
                    collected |= state
                        .animal_products
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|e| {
                            e.person == 2
                                && e.outcome == game_economy::AnimalProductOutcome::Collected
                        });
                    deposited |= state
                        .local_transport
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|e| {
                            e.person == 2
                                && e.good == wool
                                && e.moved
                                && e.to
                                    == (game_lots::Place::Stock {
                                        region: 1,
                                        good: wool,
                                    })
                        });
                    if collected && deposited {
                        break;
                    }
                }
                assert!(collected && deposited);
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                for _ in 0..30 {
                    if world
                        .snapshot()
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| {
                            c.place
                                == (game_lots::Place::Stock {
                                    region: 1,
                                    good: yarn,
                                })
                                && c.used > 0
                        })
                    {
                        break;
                    }
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                let state = world.snapshot();
                assert_eq!(state, restored.snapshot());
                assert!(state.physical.custody.containers.iter().any(|c| c.place
                    == (game_lots::Place::Stock {
                        region: 1,
                        good: yarn
                    })
                    && c.used == 1));
                assert!(
                    state
                        .fauna
                        .as_ref()
                        .unwrap()
                        .animals
                        .iter()
                        .any(|a| a.id == 1 && a.health > 0)
                );
                state
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn invoked_food_memories_can_be_told_and_retained_after_dismissal() {
    use game_economy::ReportReception;
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, meets: bool, accepts: bool, lie: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-past-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                let social = state.dynamics.as_mut().unwrap().social.as_mut().unwrap();
                social
                    .participants
                    .iter_mut()
                    .find(|p| p.person == 2)
                    .unwrap()
                    .willing = meets;
                let sensitivity = state
                    .opinions
                    .as_mut()
                    .unwrap()
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|s| s.person == 2)
                    .unwrap();
                sensitivity.report_reception = Some(if accepts {
                    ReportReception::Always
                } else {
                    ReportReception::Never
                });
                let mut world = game_economy::Economy::compile(state).unwrap();
                for _ in 0..10 {
                    if world
                        .snapshot()
                        .campaign
                        .as_ref()
                        .unwrap()
                        .lineage
                        .current
                        .is_some_and(|id| id != 1)
                    {
                        break;
                    }
                    world.advance().unwrap();
                }
                let current = world
                    .snapshot()
                    .campaign
                    .as_ref()
                    .unwrap()
                    .lineage
                    .current
                    .unwrap();
                assert_ne!(current, 1);
                assert!(world.known_food(2).unwrap().iter().all(|r| r.region != 2));
                assert!(Player::new(&mut world).past_knowledge(1).is_err());
                Player::new(&mut world).invoke(&[1]).unwrap();
                assert_eq!(
                    Player::new(&mut world)
                        .past_knowledge(1)
                        .unwrap()
                        .iter()
                        .find(|r| r.region == 2)
                        .unwrap()
                        .food,
                    37
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                let claim = lie.then_some(99);
                assert_eq!(
                    Player::new(&mut world)
                        .share_invoked_food(1, 2, 2, claim)
                        .unwrap(),
                    meets
                );
                assert_eq!(
                    Player::new(&mut restored)
                        .share_invoked_food(1, 2, 2, claim)
                        .unwrap(),
                    meets
                );
                let statement = world
                    .known_food(2)
                    .unwrap()
                    .into_iter()
                    .find(|r| r.region == 2);
                assert_eq!(statement.is_some(), meets && accepts);
                if let Some(statement) = statement {
                    assert_eq!(
                        (statement.food, statement.witness, statement.learned_from),
                        (if lie { 99 } else { 37 }, 1, Some(current))
                    );
                }
                assert_eq!(
                    world
                        .invoked_knowledge(1)
                        .unwrap()
                        .iter()
                        .find(|r| r.region == 2)
                        .unwrap()
                        .food,
                    37
                );
                Player::new(&mut world).dismiss_invocation().unwrap();
                Player::new(&mut restored).dismiss_invocation().unwrap();
                assert!(Player::new(&mut world).past_knowledge(1).is_err());
                assert_eq!(
                    world.known_food(2).unwrap().iter().any(|r| r.region == 2),
                    meets && accepts
                );
                let after_dismissal = world.snapshot();
                assert!(
                    Player::new(&mut world)
                        .share_invoked_food(1, 2, 2, claim)
                        .is_err()
                );
                assert_eq!(world.snapshot(), after_dismissal);
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    for (meets, accepts, lie) in [
        (true, true, false),
        (true, true, true),
        (true, false, true),
        (false, true, false),
    ] {
        assert_eq!(run(1, meets, accepts, lie), run(4, meets, accepts, lie));
    }
}

#[test]
fn written_claims_consume_materials_move_physically_and_require_reading() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                let paper = state.writing.as_ref().unwrap().paper;
                let ink = state.writing.as_ref().unwrap().ink;
                let mut world = game_economy::Economy::compile(state).unwrap();
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|row| row.region != 2)
                );
                let before = world.snapshot();
                let original = before
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == game_lots::Place::Person(1))
                    .unwrap();
                let original_paper = original
                    .lots
                    .iter()
                    .find(|lot| lot.good == paper)
                    .unwrap()
                    .quantity;
                let original_ink = original
                    .lots
                    .iter()
                    .find(|lot| lot.good == ink)
                    .unwrap()
                    .quantity;
                let id = Player::new(&mut world)
                    .write_food(1, 2, None, Some(99))
                    .unwrap()
                    .unwrap();
                assert_eq!(Player::new(&mut world).held_pages().unwrap(), vec![id]);
                let after = world.snapshot();
                let holder = after
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|c| c.place == game_lots::Place::Person(1))
                    .unwrap();
                assert_eq!(
                    holder
                        .lots
                        .iter()
                        .find(|lot| lot.good == paper)
                        .unwrap()
                        .quantity,
                    original_paper - 1
                );
                assert_eq!(
                    holder
                        .lots
                        .iter()
                        .find(|lot| lot.good == ink)
                        .unwrap()
                        .quantity,
                    original_ink - 1
                );
                assert_eq!(
                    after.writing.as_ref().unwrap().pages[0]
                        .claim
                        .as_ref()
                        .unwrap()
                        .food,
                    99
                );
                let mut self_reader = game_economy::Economy::compile(after.clone()).unwrap();
                assert!(Player::new(&mut self_reader).read_page(id).unwrap());
                let self_read = self_reader.snapshot();
                game_economy::Economy::compile(self_read).unwrap();
                let saved = serde_json::to_string(&after).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(Player::new(&mut world).give_page(2, id).unwrap());
                assert!(Player::new(&mut restored).give_page(2, id).unwrap());
                assert!(Player::new(&mut world).held_pages().unwrap().is_empty());
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|row| row.region != 2)
                );
                assert_eq!(world.snapshot(), restored.snapshot());
                let mut reader = world.snapshot();
                reader.campaign = None;
                for person in &mut reader.people {
                    person.controlled = person.id == 2;
                }
                let mut ignored = reader.clone();
                ignored
                    .opinions
                    .as_mut()
                    .unwrap()
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .report_reception = Some(game_economy::ReportReception::Never);
                let mut ignored = game_economy::Economy::compile(ignored).unwrap();
                assert!(ignored.read_page(2, id).unwrap());
                assert!(
                    ignored
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|row| row.region != 2)
                );
                reader
                    .opinions
                    .as_mut()
                    .unwrap()
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .report_reception = Some(game_economy::ReportReception::Always);
                let mut reader = game_economy::Economy::compile(reader).unwrap();
                assert!(reader.read_page(2, id).unwrap());
                let memory = reader
                    .known_food(2)
                    .unwrap()
                    .into_iter()
                    .find(|row| row.region == 2)
                    .unwrap();
                assert_eq!(
                    (memory.food, memory.witness, memory.learned_from),
                    (99, 1, Some(1))
                );
                reader.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn written_claims_respect_materials_capacity_literacy_and_custody_validation() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let initial = scenario(
        &root.join("content/scenarios/economy-written-food.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let mut no_ink = initial.clone();
    let ink = no_ink.writing.as_ref().unwrap().ink;
    let paper = no_ink.writing.as_ref().unwrap().paper;
    let holder = no_ink
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_lots::Place::Person(1))
        .unwrap();
    let lots = holder
        .lots
        .iter()
        .filter(|lot| lot.good != ink)
        .cloned()
        .collect();
    *holder = game_lots::Container::new(holder.place, holder.capacity, lots).unwrap();
    let before = holder
        .lots
        .iter()
        .find(|lot| lot.good == paper)
        .unwrap()
        .quantity;
    let mut no_ink = game_economy::Economy::compile(no_ink).unwrap();
    assert_eq!(
        Player::new(&mut no_ink)
            .write_food(1, 2, None, None)
            .unwrap(),
        None
    );
    let after = no_ink.snapshot();
    assert!(after.writing.as_ref().unwrap().pages.is_empty());
    assert_eq!(after.writing.as_ref().unwrap().next_item, 1);
    assert_eq!(
        after
            .physical
            .custody
            .containers
            .iter()
            .find(|c| c.place == game_lots::Place::Person(1))
            .unwrap()
            .lots
            .iter()
            .find(|lot| lot.good == paper)
            .unwrap()
            .quantity,
        before
    );

    let mut collision = initial.clone();
    let holder = collision
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_lots::Place::Person(2))
        .unwrap();
    let mut lots = holder.lots.clone();
    lots.push(game_lots::Lot {
        good: paper,
        quantity: 1,
        item: Some(1),
        expires_at: None,
    });
    *holder = game_lots::Container::new(holder.place, holder.capacity, lots).unwrap();
    let mut collision = game_economy::Economy::compile(collision).unwrap();
    assert_eq!(
        Player::new(&mut collision)
            .write_food(1, 2, None, None)
            .unwrap(),
        Some(2)
    );
    assert_eq!(collision.snapshot().writing.unwrap().next_item, 3);

    let mut world = game_economy::Economy::compile(initial).unwrap();
    let id = Player::new(&mut world)
        .write_food(1, 2, None, None)
        .unwrap()
        .unwrap();
    let mut full = world.snapshot();
    let recipient = full
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_lots::Place::Person(2))
        .unwrap();
    let mut lots = recipient.lots.clone();
    lots[0].quantity += recipient.capacity - recipient.used;
    *recipient = game_lots::Container::new(recipient.place, recipient.capacity, lots).unwrap();
    let mut full = game_economy::Economy::compile(full).unwrap();
    assert!(!Player::new(&mut full).give_page(2, id).unwrap());
    assert_eq!(Player::new(&mut full).held_pages().unwrap(), vec![id]);

    let mut consumed = world.snapshot();
    consumed.goals = None;
    consumed.dreams = None;
    consumed.skills = None;
    let page_good = consumed.writing.as_ref().unwrap().page;
    let recipe = consumed.physical.recipes.first_mut().unwrap();
    recipe.inputs = vec![game_production::Amount {
        good: page_good,
        quantity: 1,
    }];
    recipe.outputs = vec![game_production::Amount {
        good: paper,
        quantity: 1,
    }];
    recipe.ticks = 1;
    let recipe_id = recipe.id;
    let mut consumed = game_economy::Economy::compile(consumed).unwrap();
    assert!(
        Player::new(&mut consumed)
            .transfer_stock(page_good, 1, false)
            .unwrap()
    );
    assert_eq!(consumed.snapshot().writing.as_ref().unwrap().pages.len(), 1);
    assert!(Player::new(&mut consumed).start_work(recipe_id, 1).unwrap());
    for _ in 0..3 {
        if consumed
            .snapshot()
            .writing
            .as_ref()
            .unwrap()
            .pages
            .is_empty()
        {
            break;
        }
        Player::new(&mut consumed).continue_work().unwrap();
    }
    let consumed = consumed.snapshot();
    assert!(consumed.writing.as_ref().unwrap().pages.is_empty());
    game_economy::Economy::compile(consumed).unwrap();

    let mut unreadable = world.snapshot();
    unreadable
        .writing
        .as_mut()
        .unwrap()
        .literacy_mut()
        .iter_mut()
        .find(|row| row.person == 1)
        .unwrap()
        .read = false;
    let mut unreadable = game_economy::Economy::compile(unreadable).unwrap();
    let before = unreadable.snapshot();
    assert!(Player::new(&mut unreadable).read_page(id).is_err());
    assert_eq!(unreadable.snapshot(), before);

    let mut orphan = world.snapshot();
    let holder = orphan
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == game_lots::Place::Person(1))
        .unwrap();
    let lots = holder
        .lots
        .iter()
        .filter(|lot| lot.item != Some(id))
        .cloned()
        .collect();
    *holder = game_lots::Container::new(holder.place, holder.capacity, lots).unwrap();
    assert!(game_economy::Economy::compile(orphan).is_err());
}

#[test]
fn autonomous_reading_is_personal_and_can_be_disregarded() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, willing: bool, accepts: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state
                    .writing
                    .as_mut()
                    .unwrap()
                    .literacy_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .auto_read = willing;
                state
                    .opinions
                    .as_mut()
                    .unwrap()
                    .sensitivities_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .report_reception = Some(if accepts {
                    game_economy::ReportReception::Always
                } else {
                    game_economy::ReportReception::Never
                });
                let mut world = game_economy::Economy::compile(state).unwrap();
                let id = Player::new(&mut world)
                    .write_food(1, 2, None, Some(99))
                    .unwrap()
                    .unwrap();
                assert!(Player::new(&mut world).give_page(2, id).unwrap());
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|row| row.region != 2)
                );
                world.advance().unwrap();
                let after = world.snapshot();
                assert_eq!(
                    after.writing.as_ref().unwrap().reads.contains(&(2, id)),
                    willing
                );
                assert_eq!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .any(|row| row.region == 2 && row.food == 99),
                    willing && accepts
                );
                let saved = serde_json::to_string(&after).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    for (willing, accepts) in [(true, true), (true, false), (false, true)] {
        assert_eq!(run(1, willing, accepts), run(4, willing, accepts));
    }
}

#[test]
fn autonomous_authors_write_private_memories_with_real_materials() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, has_ink| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state.gathering = None;
                let knowledge = state.knowledge.as_mut().unwrap();
                let record = knowledge
                    .minds
                    .iter()
                    .find(|mind| mind.person == 1)
                    .unwrap()
                    .records
                    .iter()
                    .find(|row| row.region == 2)
                    .unwrap()
                    .clone();
                knowledge
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 2)
                    .unwrap()
                    .records
                    .push(record.clone());
                let writing = state.writing.as_mut().unwrap();
                writing
                    .literacy_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .write = true;
                writing.authors = vec![game_economy::WritingAuthor {
                    person: 2,
                    script: 1,
                    region: 2,
                    food_delta: 12,
                    completed: false,
                    retry_at: 0,
                }]
                .into();
                let (paper, ink) = (writing.paper, writing.ink);
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == game_lots::Place::Person(2))
                    .unwrap();
                let mut lots = holder.lots.clone();
                lots.retain(|lot| lot.good != paper && lot.good != ink);
                lots.push(game_lots::Lot {
                    good: paper,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                });
                if has_ink {
                    lots.push(game_lots::Lot {
                        good: ink,
                        quantity: 1,
                        item: None,
                        expires_at: None,
                    });
                }
                *holder = game_lots::Container::new(holder.place, holder.capacity.max(100), lots)
                    .unwrap();
                let mut world = game_economy::Economy::compile(state).unwrap();
                let manual = Player::new(&mut world)
                    .write_food(1, 2, None, None)
                    .unwrap()
                    .unwrap();
                let state = world.snapshot();
                let writing = state.writing.as_ref().unwrap();
                assert_eq!(writing.pages.len(), if has_ink { 2 } else { 1 });
                assert_eq!(writing.authors[0].completed, has_ink);
                if has_ink {
                    let page = writing.pages.iter().find(|page| page.author == 2).unwrap();
                    assert_ne!(manual, page.item);
                    assert_eq!(page.claim.as_ref().unwrap().food, record.food + 12);
                    assert_eq!(page.claim.as_ref().unwrap().witness, record.witness);
                    assert_eq!(
                        world
                            .known_food(2)
                            .unwrap()
                            .iter()
                            .find(|row| row.region == 2)
                            .unwrap()
                            .food,
                        record.food
                    );
                    assert_eq!(
                        world
                            .snapshot()
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == game_lots::Place::Person(2))
                            .unwrap()
                            .lots
                            .iter()
                            .filter(|lot| lot.good == paper || lot.good == ink)
                            .count(),
                        0
                    );
                } else {
                    assert!(writing.authors[0].retry_at > state.physical.custody.tick);
                    assert!(
                        state
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|c| c.place == game_lots::Place::Person(2))
                            .unwrap()
                            .lots
                            .iter()
                            .any(|lot| lot.good == paper)
                    );
                }
                let saved = serde_json::to_string(&state).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    for has_ink in [false, true] {
        assert_eq!(run(1, has_ink), run(4, has_ink));
    }
}

#[test]
fn writing_practice_consumes_real_materials_and_learns_independently() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, other_has_ink: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-writing-practice.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state.gathering = None;
                let writing = state.writing.as_mut().unwrap();
                let (paper, ink) = (writing.paper, writing.ink);
                if !other_has_ink {
                    let holder = state
                        .physical
                        .custody
                        .containers
                        .iter_mut()
                        .find(|row| row.place == game_lots::Place::Person(2))
                        .unwrap();
                    let lots = holder
                        .lots
                        .iter()
                        .filter(|lot| lot.good != ink)
                        .cloned()
                        .collect();
                    *holder =
                        game_lots::Container::new(holder.place, holder.capacity, lots).unwrap();
                }
                let mut world = game_economy::Economy::compile(state).unwrap();
                assert!(Player::new(&mut world).practice_writing(1).unwrap());
                let (literacy, progress) = Player::new(&mut world).writing_progress().unwrap();
                assert!(!literacy[0].write);
                assert_eq!(progress.unwrap().progress, 1);
                let first = world.snapshot();
                let practice = &first.writing.as_ref().unwrap().practice;
                assert_eq!(practice[0].progress, 1);
                assert_eq!(practice[1].progress, u32::from(other_has_ink));
                if !other_has_ink {
                    assert!(practice[1].retry_at > first.physical.custody.tick);
                    assert!(
                        first
                            .physical
                            .custody
                            .containers
                            .iter()
                            .find(|row| row.place == game_lots::Place::Person(2))
                            .unwrap()
                            .lots
                            .iter()
                            .any(|lot| lot.good == paper && lot.quantity == 2)
                    );
                }
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(Player::new(&mut world).practice_writing(1).unwrap());
                assert!(Player::new(&mut restored).practice_writing(1).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                let after = world.snapshot();
                let writing = after.writing.as_ref().unwrap();
                assert!(writing.can_write(1, 1));
                assert!(Player::new(&mut world).writing_progress().unwrap().0[0].write);
                assert_eq!(writing.can_write(2, 1), other_has_ink);
                assert!(writing.pages.is_empty());
                assert!(Player::new(&mut world).practice_writing(1).is_err());
                after
            })
    };
    for other_has_ink in [false, true] {
        assert_eq!(run(1, other_has_ink), run(4, other_has_ink));
    }
}

#[test]
fn repeated_personal_testimony_reinforces_recall_without_refreshing_the_claim() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                let knowledge = state.knowledge.as_mut().unwrap();
                let source = knowledge
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 1)
                    .unwrap();
                source.reporting = Some(game_economy::Reporting {
                    willing: true,
                    pressure_threshold: None,
                });
                let mut remembered = source
                    .records
                    .iter()
                    .find(|record| record.region == 2)
                    .unwrap()
                    .clone();
                remembered.learned_from = Some(1);
                knowledge
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 2)
                    .unwrap()
                    .records
                    .push(remembered.clone());
                knowledge.retention = (1..=2)
                    .map(|person| game_economy::Retention {
                        person,
                        ticks: 2,
                        traces: vec![],
                    })
                    .collect();
                let mut world = game_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                world.advance().unwrap();
                assert!(
                    world
                        .known_food(1)
                        .unwrap()
                        .iter()
                        .any(|record| record.region == 2)
                );
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .any(|record| record == &remembered)
                );
                let saved = world.snapshot();
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                world.advance().unwrap();
                restored.advance().unwrap();
                assert_eq!(world.snapshot(), restored.snapshot());
                assert!(
                    world
                        .known_food(1)
                        .unwrap()
                        .iter()
                        .all(|record| record.region != 2)
                );
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|record| record.region != 2)
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn faded_personal_memory_can_be_recovered_from_an_old_page() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state
                    .knowledge
                    .as_mut()
                    .unwrap()
                    .retention
                    .push(game_economy::Retention {
                        person: 1,
                        ticks: 2,
                        traces: vec![],
                    });
                let mut world = game_economy::Economy::compile(state).unwrap();
                let item = Player::new(&mut world)
                    .write_food(1, 2, None, None)
                    .unwrap()
                    .unwrap();
                world.advance().unwrap();
                assert!(
                    world
                        .known_food(1)
                        .unwrap()
                        .iter()
                        .all(|record| record.region != 2)
                );
                let before = world.snapshot();
                assert!(
                    Player::new(&mut world)
                        .view()
                        .unwrap()
                        .food_reports
                        .iter()
                        .all(|record| record.region != 2)
                );
                assert!(
                    Player::new(&mut world)
                        .write_food(1, 2, None, None)
                        .is_err()
                );
                assert_eq!(before, world.snapshot());
                let mut restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                assert!(Player::new(&mut world).read_page(item).unwrap());
                assert!(Player::new(&mut restored).read_page(item).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                let recovered = world
                    .known_food(1)
                    .unwrap()
                    .into_iter()
                    .find(|record| record.region == 2)
                    .unwrap();
                assert_eq!(
                    (recovered.food, recovered.observed_at, recovered.witness),
                    (37, 0, 1)
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn autonomous_writers_cannot_use_a_faded_memory() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, fades| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state.gathering = None;
                let knowledge = state.knowledge.as_mut().unwrap();
                let record = knowledge
                    .minds
                    .iter()
                    .find(|mind| mind.person == 1)
                    .unwrap()
                    .records
                    .iter()
                    .find(|record| record.region == 2)
                    .unwrap()
                    .clone();
                knowledge
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 2)
                    .unwrap()
                    .records
                    .push(record);
                if fades {
                    knowledge.retention.push(game_economy::Retention {
                        person: 2,
                        ticks: 1,
                        traces: vec![],
                    });
                }
                let writing = state.writing.as_mut().unwrap();
                writing
                    .literacy_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap()
                    .write = true;
                writing.authors = vec![game_economy::WritingAuthor {
                    person: 2,
                    script: 1,
                    region: 2,
                    food_delta: 0,
                    completed: false,
                    retry_at: 2,
                }]
                .into();
                let mut world = game_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                world.advance().unwrap();
                let snapshot = world.snapshot();
                assert_eq!(
                    snapshot.writing.as_ref().unwrap().pages.len(),
                    usize::from(!fades)
                );
                snapshot
            })
    };
    for fades in [false, true] {
        assert_eq!(run(1, fades), run(4, fades));
    }
}

#[test]
fn autonomous_written_claims_can_be_physically_shared_and_read() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                state.gathering = None;
                let knowledge = state.knowledge.as_mut().unwrap();
                let memory = knowledge
                    .minds
                    .iter()
                    .find(|mind| mind.person == 1)
                    .unwrap()
                    .records
                    .iter()
                    .find(|record| record.region == 2)
                    .unwrap()
                    .clone();
                knowledge
                    .minds
                    .iter_mut()
                    .find(|mind| mind.person == 2)
                    .unwrap()
                    .records
                    .push(memory.clone());
                let writing = state.writing.as_mut().unwrap();
                let literacy = writing
                    .literacy_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap();
                literacy.write = true;
                literacy.auto_read = false;
                writing.authors = vec![game_economy::WritingAuthor {
                    person: 2,
                    script: 1,
                    region: 2,
                    food_delta: 12,
                    completed: false,
                    retry_at: 0,
                }]
                .into();
                let page_good = writing.page;
                let giving = state.giving.as_mut().unwrap();
                giving.policies_mut().retain(|policy| policy.person != 2);
                giving.responses_mut().retain(|policy| policy.person != 2);
                giving.policies_mut().push(game_economy::GivingPolicy {
                    person: 2,
                    recipient: 1,
                    good: page_good,
                    quantity: 1,
                    recipient_hunger_at_least: 0,
                    willing: true,
                    requires_request: false,
                    pressured_by: None,
                    pressure_threshold: None,
                });
                let mut world = game_economy::Economy::compile(state).unwrap();
                world.advance().unwrap();
                let written = world.snapshot();
                let page = written
                    .writing
                    .as_ref()
                    .unwrap()
                    .pages
                    .iter()
                    .find(|page| page.author == 2)
                    .unwrap();
                let item = page.item;
                assert_eq!(page.claim.as_ref().unwrap().food, memory.food + 12);
                assert!(Player::new(&mut world).held_pages().unwrap().is_empty());
                world.advance().unwrap();
                assert_eq!(Player::new(&mut world).held_pages().unwrap(), vec![item]);
                assert_eq!(
                    world
                        .known_food(1)
                        .unwrap()
                        .iter()
                        .find(|record| record.region == 2)
                        .unwrap()
                        .food,
                    memory.food
                );
                assert!(Player::new(&mut world).read_page(item).unwrap());
                assert_eq!(
                    world
                        .known_food(1)
                        .unwrap()
                        .iter()
                        .find(|record| record.region == 2)
                        .unwrap()
                        .food,
                    memory.food + 12
                );
                let saved = world.snapshot();
                let restored = game_economy::Economy::compile(
                    serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                )
                .unwrap();
                assert_eq!(saved, restored.snapshot());
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_learns_a_script_from_observing_a_page_during_encounters() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers, enabled| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-written-food.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                let writing = state.writing.as_mut().unwrap();
                let student = writing
                    .literacy_mut()
                    .iter_mut()
                    .find(|row| row.person == 2)
                    .unwrap();
                student.read = false;
                student.auto_read = false;
                writing.study = vec![game_economy::WritingStudy {
                    person: 2,
                    script: 1,
                    required: 2,
                    progress: 0,
                    enabled,
                }]
                .into();
                let mut world = game_economy::Economy::compile(state).unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert_eq!(
                    world.snapshot().writing.as_ref().unwrap().study[0].progress,
                    0
                );
                Player::new(&mut world)
                    .write_food(1, 2, None, Some(99))
                    .unwrap()
                    .unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                let halfway = world.snapshot();
                assert_eq!(
                    halfway.writing.as_ref().unwrap().study[0].progress,
                    u32::from(enabled)
                );
                assert!(!halfway.writing.as_ref().unwrap().can_read(2, 1));
                let saved = serde_json::to_string(&halfway).unwrap();
                let mut restored =
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap()).unwrap();
                assert!(Player::new(&mut world).meet(2).unwrap());
                assert!(Player::new(&mut restored).meet(2).unwrap());
                assert_eq!(world.snapshot(), restored.snapshot());
                let after = world.snapshot();
                assert_eq!(after.writing.as_ref().unwrap().can_read(2, 1), enabled);
                assert!(
                    world
                        .known_food(2)
                        .unwrap()
                        .iter()
                        .all(|record| record.region != 2)
                );
                after
            })
    };
    for enabled in [false, true] {
        assert_eq!(run(1, enabled), run(4, enabled));
    }
}

#[test]
fn invoked_memory_can_be_written_and_read_after_dismissal() {
    use game_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = scenario(
                    &root.join("content/scenarios/economy-past-testimony.toml"),
                    &root.join("content"),
                )
                .unwrap()
                .snapshot();
                state.autowork = None;
                state.goals = None;
                state.dreams = None;
                let mut world = game_economy::Economy::compile(state).unwrap();
                for _ in 0..10 {
                    if world
                        .snapshot()
                        .campaign
                        .as_ref()
                        .unwrap()
                        .lineage
                        .current
                        .is_some_and(|id| id != 1)
                    {
                        break;
                    }
                    world.advance().unwrap();
                }
                let mut state = world.snapshot();
                let current = state.campaign.as_ref().unwrap().lineage.current.unwrap();
                assert_ne!(current, 1);
                let good = |name: &str| {
                    state
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.name == name)
                        .unwrap()
                        .id
                };
                let (paper, ink, page) = (good("paper"), good("ink"), good("written-page"));
                let holder = state
                    .physical
                    .custody
                    .containers
                    .iter_mut()
                    .find(|row| row.place == game_lots::Place::Person(current))
                    .unwrap();
                let mut lots = holder.lots.clone();
                lots.push(game_lots::Lot {
                    good: paper,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                });
                lots.push(game_lots::Lot {
                    good: ink,
                    quantity: 1,
                    item: None,
                    expires_at: None,
                });
                *holder = game_lots::Container::new(holder.place, holder.capacity, lots).unwrap();
                state.writing = Some(game_economy::Writing {
                    text_retention: Default::default(),
                    text_memories: Default::default(),
                    text_templates: Default::default(),
                    text_authors: Default::default(),
                    practice: Default::default(),
                    study: Default::default(),
                    authors: Default::default(),
                    paper,
                    ink,
                    page,
                    paper_quantity: 1,
                    ink_quantity: 1,
                    next_item: 1,
                    literacy: vec![game_economy::Literacy {
                        person: current,
                        script: 1,
                        read: true,
                        write: true,
                        auto_read: false,
                    }]
                    .into(),
                    pages: vec![].into(),
                    reads: vec![].into(),
                });
                let mut world = game_economy::Economy::compile(state).unwrap();
                assert!(
                    Player::new(&mut world)
                        .write_food(1, 2, Some(1), None)
                        .is_err()
                );
                Player::new(&mut world).invoke(&[1]).unwrap();
                let id = Player::new(&mut world)
                    .write_food(1, 2, Some(1), None)
                    .unwrap()
                    .unwrap();
                Player::new(&mut world).dismiss_invocation().unwrap();
                assert!(Player::new(&mut world).past_knowledge(1).is_err());
                assert_eq!(Player::new(&mut world).held_pages().unwrap(), vec![id]);
                assert!(Player::new(&mut world).read_page(id).unwrap());
                assert_eq!(
                    world
                        .known_food(current)
                        .unwrap()
                        .into_iter()
                        .find(|row| row.region == 2)
                        .unwrap()
                        .food,
                    37
                );
                let saved = serde_json::to_string(&world.snapshot()).unwrap();
                assert_eq!(
                    world.snapshot(),
                    game_economy::Economy::compile(serde_json::from_str(&saved).unwrap())
                        .unwrap()
                        .snapshot()
                );
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn player_sees_only_carried_tool_conditions_and_can_repair_one() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-player-tool-repair.toml"),
        &root.join("content"),
    )
    .unwrap();
    let before = world.snapshot();
    {
        let mut player = game_economy_player::Player::new(&mut world);
        let view = player.view().unwrap();
        assert_eq!(view.tools.len(), 1);
        assert_eq!(view.tools[0].item, 1001);
        assert_eq!(view.tools[0].damage, 2);
        assert!(player.repair_tool(1002).is_err());
    }
    assert_eq!(world.snapshot(), before);
    let mut player = game_economy_player::Player::new(&mut world);
    assert!(player.repair_tool(1001).unwrap());
    let view = player.view().unwrap();
    assert_eq!(view.tools.len(), 1);
    assert_eq!(view.tools[0].item, 1001);
    assert_eq!(view.tools[0].damage, 0);
}

#[test]
fn player_sees_local_moisture_and_waters_using_finite_stock() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/scenarios/economy-player-irrigation.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = game_economy_player::Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.field_moisture.len(), 1);
    assert_eq!(view.field_moisture[0].field, 1);
    assert_eq!(view.field_moisture[0].amount, 0);
    assert!(player.farm(1).unwrap());
    assert!(player.water_field(1, 1, false).unwrap());
    let view = player.view().unwrap();
    assert_eq!(view.field_moisture[0].amount, 1);
    assert!(!player.water_field(1, 1, false).unwrap());
    let view = player.view().unwrap();
    assert_eq!(view.field_moisture[0].amount, 0);
}

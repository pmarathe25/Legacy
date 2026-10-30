use game_parallel::{
    World,
    work_requests::{Outcome, Proposal},
};
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    World::from_toml(include_str!(
        "../../../../content/parallel/work_requests.toml"
    ))
    .unwrap()
}
fn proposal() -> Proposal {
    Proposal {
        requester: 1,
        worker: 2,
        region: 1,
        recipe: 1,
        expires: 5,
    }
}

#[test]
fn asked_work_starts_through_real_reservations_and_finishes_later() {
    let mut engine = Kernel::compile(world()).unwrap();
    assert_eq!(engine.request_work(proposal()).unwrap(), 1);
    let before = engine.export();
    let draft = engine.prepare().unwrap();
    assert_eq!(engine.export(), before);
    engine.commit(draft).unwrap();
    let started = engine.export();
    assert_eq!(
        started
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .events[0]
            .outcome,
        Outcome::Started { job: 100 }
    );
    assert_eq!(
        started.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        6
    );
    assert_eq!(started.production.as_ref().unwrap().jobs[0].worker, 2);
    let mut loaded =
        Kernel::compile(World::from_json(&serde_json::to_string(&started).unwrap()).unwrap())
            .unwrap();
    for _ in 0..2 {
        engine.step().unwrap();
        loaded.step().unwrap();
    }
    assert_eq!(engine.export(), loaded.export());
    assert_eq!(
        engine.export().production.unwrap().inventories[0].stocks[1].quantity,
        2
    );
}

#[test]
fn addressed_person_can_start_or_refuse_and_a_failed_start_is_atomic() {
    let mut engine = Kernel::compile(world()).unwrap();
    let id = engine.request_work(proposal()).unwrap();
    let before = engine.export();
    assert!(engine.respond_work(id, 3, true).is_err());
    assert_eq!(engine.export(), before);
    engine.respond_work(id, 2, true).unwrap();
    let result = engine.export();
    assert_eq!(result.production.as_ref().unwrap().jobs[0].worker, 2);
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        6
    );
    let requests = result.autowork.unwrap().requests.unwrap();
    assert!(requests.pending.is_empty());
    assert!(matches!(
        requests.events[0].outcome,
        Outcome::Started { .. }
    ));

    let mut engine = Kernel::compile(world()).unwrap();
    let id = engine.request_work(proposal()).unwrap();
    engine.respond_work(id, 2, false).unwrap();
    let result = engine.export();
    assert!(result.production.unwrap().jobs.is_empty());
    assert_eq!(
        result.autowork.unwrap().requests.unwrap().events[0].outcome,
        Outcome::Refused
    );

    let mut state = world();
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 0;
    let mut engine = Kernel::compile(state).unwrap();
    let id = engine.request_work(proposal()).unwrap();
    let before = engine.export();
    assert!(engine.respond_work(id, 2, true).is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn work_request_id_overflow_does_not_publish_any_part_of_a_batch() {
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .next_id = u64::MAX;
    let mut engine = Kernel::compile(state).unwrap();
    let before = engine.export();
    assert!(engine.request_work_batch(&[proposal()]).is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn issuing_requests_invalidates_prepared_ticks_without_losing_the_requests() {
    let mut engine = Kernel::compile(world()).unwrap();
    let draft = engine.prepare().unwrap();
    engine.request_work(proposal()).unwrap();
    let before = engine.export();
    assert!(engine.commit(draft).is_err());
    assert_eq!(engine.export(), before);
    assert_eq!(before.autowork.unwrap().requests.unwrap().pending.len(), 1);
}

#[test]
fn a_working_requester_cannot_send_a_request() {
    let mut engine = Kernel::compile(world()).unwrap();
    engine
        .start_work(&[game_parallel::StartWork {
            id: 50,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = engine.export();
    assert!(engine.request_work(proposal()).is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn a_person_can_work_with_claimed_inputs_without_permission_and_be_seen() {
    let state = World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    let job = game_parallel::StartWork {
        id: 50,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    let mut engine = Kernel::compile(state).unwrap();
    let before = engine.export();
    assert!(engine.start_work(&[job]).is_err());
    assert_eq!(engine.export(), before);
    engine.work_without_permission(job).unwrap();
    let result = engine.export();
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        6
    );
    assert_eq!(result.production.as_ref().unwrap().jobs[0].worker, 1);
    let property = result.property.unwrap();
    assert_eq!(property.claims[0].owner, 2);
    assert_eq!(property.thefts.len(), 1);
    assert_eq!(
        (
            property.thefts[0].actor,
            property.thefts[0].owner,
            property.thefts[0].quantity,
            property.thefts[0].witnessed
        ),
        (1, 2, 3, true)
    );
    assert_eq!(result.opinions.unwrap().records[0].trust, -2);
}

#[test]
fn unobserved_unpermitted_work_and_failed_reservations_do_not_create_false_knowledge() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    let owner = state.regions[0].people.remove(1);
    state.regions[1].people.push(owner);
    let job = game_parallel::StartWork {
        id: 50,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    let mut engine = Kernel::compile(state).unwrap();
    let before = engine.export();
    assert!(
        engine
            .work_without_permission(game_parallel::StartWork { units: 100, ..job })
            .is_err()
    );
    assert_eq!(engine.export(), before);
    engine.work_without_permission(job).unwrap();
    let result = engine.export();
    assert!(!result.property.unwrap().thefts[0].witnessed);
    assert!(result.opinions.unwrap().records.is_empty());
}

#[test]
fn an_autonomous_person_can_use_claimed_material_and_a_witness_learns() {
    let mut state = world();
    let other = World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    state.property = other.property;
    state.opinions = other.opinions;
    state.market = None;
    let auto = state.autowork.as_mut().unwrap();
    auto.requests = None;
    auto.policies = vec![game_parallel::WorkPolicy {
        person: 1,
        recipe: 1,
        enabled: true,
        disregard_claims: true,
    }];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.step().unwrap();
                let result = engine.export();
                assert_eq!(
                    result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
                    6
                );
                assert!(
                    result
                        .production
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .any(|job| job.worker == 1)
                );
                let theft = &result.property.as_ref().unwrap().thefts[0];
                assert_eq!(
                    (theft.actor, theft.owner, theft.quantity, theft.witnessed),
                    (1, 2, 3, true)
                );
                assert_eq!(result.opinions.as_ref().unwrap().records[0].trust, -2);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_recipient_can_choose_to_help_with_inputs_claimed_by_another_person() {
    let mut state = world();
    let other = World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    state.property = other.property;
    state.property.as_mut().unwrap().claims[0].owner = 1;
    state.opinions = other.opinions;
    let requester = state.regions[1].people.remove(0);
    state.regions[0].people.push(requester);
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .disregard_claims = true;
    let mut engine = Kernel::compile(state).unwrap();
    engine
        .request_work(Proposal {
            requester: 3,
            ..proposal()
        })
        .unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert!(matches!(
        result.autowork.unwrap().requests.unwrap().events[0].outcome,
        Outcome::Started { .. }
    ));
    assert_eq!(
        (
            result.property.as_ref().unwrap().thefts[0].actor,
            result.property.as_ref().unwrap().thefts[0].owner
        ),
        (2, 1)
    );
    assert_eq!(result.opinions.unwrap().records[0].trust, -2);
}

#[test]
fn concentrated_unpermitted_work_and_witness_updates_are_worker_invariant() {
    use game_parallel::{Person, WorkPolicy};
    let mut state = world();
    let other = World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    state.property = other.property;
    state.opinions = other.opinions;
    state.autowork.as_mut().unwrap().requests = None;
    let count = 5000u64;
    state.rules.hunger_per_tick = 0;
    state.rules.food_capacity = 20_000;
    state.regions[0].food = 15_000;
    state.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.market = None;
    state.property.as_mut().unwrap().claims[0].owner = 1;
    let inventory = &mut state.production.as_mut().unwrap().inventories[0];
    inventory.stocks[0].quantity = count * 3;
    inventory.stocks[0].capacity = count * 3;
    inventory.stocks[1].capacity = count * 2;
    state.autowork.as_mut().unwrap().policies = (2..=count + 1)
        .map(|person| WorkPolicy {
            person,
            recipe: 1,
            enabled: true,
            disregard_claims: true,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.step().unwrap();
                let result = engine.export();
                assert_eq!(
                    result.production.as_ref().unwrap().jobs.len(),
                    count as usize
                );
                assert_eq!(
                    result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
                    0
                );
                assert_eq!(
                    result.property.as_ref().unwrap().thefts.len(),
                    count as usize
                );
                assert_eq!(
                    result.opinions.as_ref().unwrap().records.len(),
                    count as usize
                );
                assert!(
                    result
                        .opinions
                        .as_ref()
                        .unwrap()
                        .records
                        .iter()
                        .all(|row| row.observer == 1 && row.trust == -2)
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_person_can_refuse_or_defer_when_resources_are_missing() {
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .help_value = 0;
    let mut refused = Kernel::compile(state).unwrap();
    refused.request_work(proposal()).unwrap();
    refused.step().unwrap();
    let result = refused.export();
    assert_eq!(
        result.autowork.unwrap().requests.unwrap().events[0].outcome,
        Outcome::Refused
    );
    assert!(result.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(
        result.production.unwrap().inventories[0].stocks[0].quantity,
        9
    );

    let mut state = world();
    state.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 2;
    let mut deferred = Kernel::compile(state).unwrap();
    let mut request = proposal();
    request.expires = 1;
    deferred.request_work(request).unwrap();
    deferred.step().unwrap();
    assert_eq!(
        deferred
            .export()
            .autowork
            .unwrap()
            .requests
            .unwrap()
            .pending
            .len(),
        1
    );
    deferred.step().unwrap();
    let result = deferred.export();
    assert_eq!(
        result.autowork.unwrap().requests.unwrap().events[0].outcome,
        Outcome::Expired
    );
    assert!(result.production.unwrap().jobs.is_empty());
}

#[test]
fn fear_can_motivate_work_without_trust_or_a_contract() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .help_value = 0;
    state.opinions = Some(Opinions {
        sensitivities: vec![],
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: -4,
            fear: 4,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    });
    let mut engine = Kernel::compile(state).unwrap();
    engine.request_work(proposal()).unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert!(matches!(
        result.autowork.unwrap().requests.unwrap().events[0].outcome,
        Outcome::Started { .. }
    ));
    assert_eq!(result.opinions.unwrap().records[0].trust, -4);
    assert!(result.agreements.is_none());
}

#[test]
fn a_person_can_stop_complying_after_fear_fades() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences[0]
        .help_value = 0;
    state.opinions = Some(Opinions {
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 0,
            default_loss: 0,
            theft_loss: 0,
            encounter_gain: 0,
            fear_per_damage: 0,
            fear_decay_per_tick: 2,
            gratitude_gain: 0,
            obligation_relief: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
        },
        sensitivities: vec![],
        events: vec![],
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: -4,
            fear: 4,
            obligation: 0,
            affinity: 0,
        }],
    });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.request_work(proposal()).unwrap();
                engine.step().unwrap();
                let first = engine.export();
                assert!(matches!(
                    first
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .events[0]
                        .outcome,
                    Outcome::Started { .. }
                ));
                assert_eq!(first.opinions.as_ref().unwrap().index().fear(2, 1), 2);
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                engine.step().unwrap();
                engine.step().unwrap();
                engine
                    .request_work(Proposal {
                        expires: 8,
                        ..proposal()
                    })
                    .unwrap();
                engine.step().unwrap();
                let result = engine.export();
                assert_eq!(
                    result
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .events[0]
                        .outcome,
                    Outcome::Refused
                );
                assert_eq!(result.opinions.as_ref().unwrap().index().fear(2, 1), 0);
                assert_eq!(result.opinions.as_ref().unwrap().index().trust(2, 1), -4);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn concentrated_requested_work_reserves_shared_inputs_deterministically() {
    use game_parallel::{
        Person,
        work_requests::{Preference, Requests},
    };
    let mut state = world();
    let count = 5000u64;
    state.rules.food_capacity = 20_000;
    state.regions[0].food = 15_000;
    state.regions[0].people = (1..=count + 1)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.market = None;
    let inventory = &mut state.production.as_mut().unwrap().inventories[0];
    inventory.stocks[0].capacity = count * 3;
    inventory.stocks[0].quantity = count / 2 * 3;
    inventory.stocks[1].capacity = count * 2;
    let requests = Requests {
        memory_limit: 0,
        work_links: vec![],
        memories: vec![],
        askers: vec![],
        next_id: 1,
        preferences: (2..=count + 1)
            .map(|person| Preference {
                person,
                effort: 1,
                help_value: 1,
                disregard_claims: false,
            })
            .collect(),
        pending: vec![],
        events: vec![],
    };
    state.autowork.as_mut().unwrap().requests = Some(requests);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                let proposals: Vec<_> = (2..=count + 1)
                    .map(|worker| Proposal {
                        worker,
                        ..proposal()
                    })
                    .collect();
                assert_eq!(
                    engine.request_work_batch(&proposals).unwrap(),
                    (1..=count).collect::<Vec<_>>()
                );
                let before = engine.export();
                assert!(engine.request_work_batch(&[proposal(); 4]).is_err());
                assert_eq!(engine.export(), before);
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&before).unwrap()).unwrap(),
                )
                .unwrap();
                engine.step().unwrap();
                let result = engine.export();
                let requests = result.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert_eq!(requests.events.len(), (count / 2) as usize);
                assert_eq!(requests.pending.len(), (count / 2) as usize);
                assert_eq!(
                    result.production.as_ref().unwrap().jobs.len(),
                    (count / 2) as usize
                );
                assert_eq!(
                    result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
                    0
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn an_inbox_with_multiple_requests_considers_the_oldest_first() {
    let mut state = world();
    let third = state.regions[1].people.remove(0);
    state.regions[0].people.push(third);
    state.regions[0].people.push(game_parallel::Person {
        id: 10,
        health: 100,
        hunger: 0,
    });
    let preferences = &mut state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .preferences;
    preferences.push(game_parallel::work_requests::Preference {
        person: 10,
        effort: 1,
        help_value: 1,
        disregard_claims: false,
    });
    preferences.reverse();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                let proposals = [
                    Proposal {
                        requester: 3,
                        ..proposal()
                    },
                    proposal(),
                ];
                assert_eq!(engine.request_work_batch(&proposals).unwrap(), vec![1, 2]);
                engine.step().unwrap();
                let result = engine.export();
                let requests = result.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert_eq!(requests.events[0].request, 1);
                assert!(matches!(
                    requests.events[0].outcome,
                    Outcome::Started { .. }
                ));
                assert_eq!(requests.pending[0].id, 2);
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn failed_tick_rolls_back_requested_work_and_response() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut content = world();
    state.production = content.production.take();
    state.goods_cargo = content.goods_cargo.take();
    state.market = content.market.take();
    state.autowork = content.autowork.take();
    state.opinions = World::from_toml(include_str!(
        "../../../../content/parallel/fear_recovery.toml"
    ))
    .unwrap()
    .opinions;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut engine = Kernel::compile(state).unwrap();
    engine.request_work(proposal()).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn a_person_with_a_goal_can_autonomously_request_help_that_starts_real_work() {
    use game_parallel::{Goal, Goals, work_requests::Asker};
    let mut state = world();
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 10,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
        }],
        events: vec![],
    });
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .push(Asker {
            person: 1,
            interval: 1,
            lifetime: 5,
        });
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.step().unwrap();
                let first = engine.export();
                let pending = &first
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap()
                    .pending;
                assert_eq!(pending.len(), 1);
                assert_eq!(
                    (pending[0].requester, pending[0].worker, pending[0].recipe),
                    (1, 2, 1)
                );
                assert!(first.production.as_ref().unwrap().jobs.is_empty());
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                engine.step().unwrap();
                let second = engine.export();
                assert!(
                    second
                        .autowork
                        .as_ref()
                        .unwrap()
                        .requests
                        .as_ref()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.worker == 2
                            && matches!(event.outcome, Outcome::Started { .. }))
                );
                assert!(
                    second
                        .production
                        .as_ref()
                        .unwrap()
                        .jobs
                        .iter()
                        .any(|job| job.worker == 2)
                );
                for _ in 0..2 {
                    engine.step().unwrap();
                }
                let result = engine.export();
                assert_eq!(
                    result.production.as_ref().unwrap().inventories[0].stocks[1].quantity,
                    4
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn requested_work_is_remembered_only_when_completion_is_witnessed() {
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memory_limit = 4;
    let run = |workers, leave| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.request_work(proposal()).unwrap();
                engine.step().unwrap();
                let started = engine.export();
                let requests = started
                    .autowork
                    .as_ref()
                    .unwrap()
                    .requests
                    .as_ref()
                    .unwrap();
                assert_eq!(requests.work_links.len(), 1);
                assert!(requests.memories.is_empty());
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&started).unwrap()).unwrap(),
                )
                .unwrap();
                if leave {
                    engine
                        .move_people(&[game_parallel::Move {
                            person: 1,
                            destination: 2,
                        }])
                        .unwrap();
                }
                engine.step().unwrap();
                engine.step().unwrap();
                let result = engine.export();
                let requests = result.autowork.as_ref().unwrap().requests.as_ref().unwrap();
                assert!(requests.work_links.is_empty());
                if leave {
                    assert!(requests.memories.is_empty());
                } else {
                    assert_eq!(
                        (
                            requests.memories[0].observer,
                            requests.memories[0].worker,
                            requests.memories[0].successes
                        ),
                        (1, 2, 1)
                    );
                }
                result
            })
    };
    assert_eq!(run(1, false), run(4, false));
    assert_eq!(run(1, true), run(4, true));
}

#[test]
fn interrupted_work_is_a_failure_only_for_a_present_requester() {
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memory_limit = 4;
    for reassign in [false, true] {
        let mut engine = Kernel::compile(state.clone()).unwrap();
        engine.request_work(proposal()).unwrap();
        engine.step().unwrap();
        if reassign {
            engine.reassign_work(100, 1).unwrap();
        } else {
            engine.cancel_work(100).unwrap();
        }
        let after = engine.export();
        let requests = after.autowork.as_ref().unwrap().requests.as_ref().unwrap();
        assert!(requests.work_links.is_empty());
        assert_eq!(
            (requests.memories[0].worker, requests.memories[0].failures),
            (2, 1)
        );
        Kernel::compile(World::from_json(&serde_json::to_string(&after).unwrap()).unwrap())
            .unwrap();
    }
}

#[test]
fn a_person_can_choose_a_witnessed_helper_for_a_later_goal() {
    use game_parallel::{Goal, Goals, work_requests::Asker};
    let mut state = world();
    let helper = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(helper);
    let requests = state.autowork.as_mut().unwrap().requests.as_mut().unwrap();
    requests.memory_limit = 4;
    requests
        .preferences
        .push(game_parallel::work_requests::Preference {
            person: 3,
            effort: 1,
            help_value: 1,
            disregard_claims: false,
        });
    let mut engine = Kernel::compile(state).unwrap();
    engine
        .request_work(Proposal {
            worker: 3,
            ..proposal()
        })
        .unwrap();
    for _ in 0..3 {
        engine.step().unwrap();
    }
    let mut state = engine.export();
    assert_eq!(
        state
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .memories[0]
            .worker,
        3
    );
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 10,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
        }],
        events: vec![],
    });
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .push(Asker {
            person: 1,
            interval: 1,
            lifetime: 5,
        });
    let mut engine = Kernel::compile(state).unwrap();
    engine.step().unwrap();
    let result = engine.export();
    let requests = result.autowork.unwrap().requests.unwrap();
    assert_eq!(requests.pending[0].worker, 3);
}

#[test]
fn witnessed_refusal_changes_whom_the_person_asks_without_compelling_anyone() {
    use game_parallel::{
        Goal, Goals,
        work_requests::{Asker, Preference},
    };
    let mut state = world();
    let third = state.regions[1].people.pop().unwrap();
    state.regions[0].people.push(third);
    let requests = state.autowork.as_mut().unwrap().requests.as_mut().unwrap();
    requests.memory_limit = 4;
    requests.preferences[0].help_value = 0;
    requests.preferences.push(Preference {
        person: 3,
        effort: 1,
        help_value: 1,
        disregard_claims: false,
    });
    let mut engine = Kernel::compile(state).unwrap();
    engine.request_work(proposal()).unwrap();
    engine.step().unwrap();
    let mut state = engine.export();
    assert_eq!(
        state
            .autowork
            .as_ref()
            .unwrap()
            .requests
            .as_ref()
            .unwrap()
            .memories[0]
            .failures,
        1
    );
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![Goal {
            person: 1,
            recipe: 1,
            priority: 10,
            target: 1,
            progress: 0,
            next_review: 0,
            enabled: true,
        }],
        events: vec![],
    });
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .askers
        .push(Asker {
            person: 1,
            interval: 1,
            lifetime: 5,
        });
    let mut engine = Kernel::compile(state).unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert_eq!(
        result.autowork.unwrap().requests.unwrap().pending[0].worker,
        3
    );
}

#[test]
fn manual_refusal_is_learned_only_if_the_requester_is_present() {
    let mut state = world();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memory_limit = 4;
    let mut engine = Kernel::compile(state.clone()).unwrap();
    engine.request_work(proposal()).unwrap();
    engine.respond_work(1, 2, false).unwrap();
    assert_eq!(
        engine.export().autowork.unwrap().requests.unwrap().memories[0].failures,
        1
    );
    let mut engine = Kernel::compile(state).unwrap();
    engine.request_work(proposal()).unwrap();
    engine
        .move_people(&[game_parallel::Move {
            person: 1,
            destination: 2,
        }])
        .unwrap();
    engine.respond_work(1, 2, false).unwrap();
    assert!(
        engine
            .export()
            .autowork
            .unwrap()
            .requests
            .unwrap()
            .memories
            .is_empty()
    );
}

#[test]
fn completed_work_memory_rolls_back_if_lifecycle_fails() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut content = world();
    state.production = content.production.take();
    state.goods_cargo = content.goods_cargo.take();
    state.market = content.market.take();
    state.autowork = content.autowork.take();
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memory_limit = 4;
    let mut engine = Kernel::compile(state).unwrap();
    engine.request_work(proposal()).unwrap();
    engine.respond_work(1, 2, true).unwrap();
    engine.step().unwrap();
    let mut state = engine.export();
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut engine = Kernel::compile(state).unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
    let requests = before.autowork.unwrap().requests.unwrap();
    assert!(requests.memories.is_empty());
    assert_eq!(requests.work_links.len(), 1);
}

#[test]
fn experienced_help_can_motivate_a_later_unpromised_food_gift() {
    use game_parallel::{FoodSharingPolicy, Holding};
    let mut state = world();
    let mut food =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let cargo = food.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.acquire_when_hungry.clear();
    cargo.holdings = vec![Holding { person: 1, food: 4 }];
    cargo.food_sharing = vec![FoodSharingPolicy {
        person: 1,
        amount: 2,
        retain: 2,
        threshold: 1,
        interval: 1,
        enabled: true,
    }];
    state.cargo = food.cargo.take();
    state.opinions = food.opinions.take();
    state.opinions.as_mut().unwrap().records.clear();
    state.opinions.as_mut().unwrap().rules.obligation_relief = 1;
    state
        .autowork
        .as_mut()
        .unwrap()
        .requests
        .as_mut()
        .unwrap()
        .memory_limit = 4;
    let run = |workers, decline| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut engine = Kernel::compile(state.clone()).unwrap();
                engine.request_work(proposal()).unwrap();
                for _ in 0..3 {
                    engine.step().unwrap();
                }
                let completed = engine.export();
                let opinion = completed
                    .opinions
                    .as_ref()
                    .unwrap()
                    .records
                    .iter()
                    .find(|o| o.observer == 1 && o.target == 2)
                    .unwrap();
                assert_eq!(opinion.obligation, 1);
                assert!(
                    completed
                        .cargo
                        .as_ref()
                        .unwrap()
                        .holdings
                        .iter()
                        .all(|h| h.person != 2)
                );
                engine = Kernel::compile(
                    World::from_json(&serde_json::to_string(&completed).unwrap()).unwrap(),
                )
                .unwrap();
                if decline {
                    engine.set_food_aid(1, false).unwrap();
                }
                engine.step().unwrap();
                let result = engine.export();
                let received = result
                    .cargo
                    .as_ref()
                    .unwrap()
                    .holdings
                    .iter()
                    .find(|h| h.person == 2)
                    .map_or(0, |h| h.food);
                assert_eq!(received, if decline { 0 } else { 2 });
                let opinion = result
                    .opinions
                    .as_ref()
                    .unwrap()
                    .records
                    .iter()
                    .find(|o| o.observer == 1 && o.target == 2)
                    .unwrap();
                assert_eq!(opinion.obligation, if decline { 1 } else { 0 });
                assert!(result.agreements.is_none());
                assert!(result.institutions.is_none());
                result
            })
    };
    assert_eq!(run(1, false), run(4, false));
    assert_eq!(run(1, true), run(4, true));
}

#[test]
fn general_food_sharing_can_follow_fear_without_an_agreement() {
    use game_parallel::FoodSharingPolicy;
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/food_aid.toml")).unwrap();
    let cargo = state.cargo.as_mut().unwrap();
    cargo.food_aid.clear();
    cargo.food_sharing = vec![FoodSharingPolicy {
        person: 1,
        amount: 2,
        retain: 0,
        threshold: 3,
        interval: 1,
        enabled: true,
    }];
    let mut engine = Kernel::compile(state).unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert_eq!(result.cargo.as_ref().unwrap().holdings[0].person, 2);
    assert!(result.agreements.is_none());
}

use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_economy_taking::{Context, Intent, Outcome, Policy, Response, Taking};
use game_parallel_lot_runtime::{Runtime, Snapshot};
use game_parallel_lots::{Container, Lot, Place, State};
use game_parallel_production_model::Good;

fn people(count: u64) -> Vec<Person> {
    (1..=count)
        .map(|id| Person {
            id,
            region: Some(1),
            health: 100,
            hunger: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        })
        .collect()
}

fn policy(person: u64, target: u64) -> Policy {
    Policy {
        person,
        target,
        good: 1,
        quantity: 1,
        willing: true,
        interval: 1,
        escalate: true,
        pressured_by: None,
        pressure_threshold: None,
        affinity_at_least: None,
        affinity_at_most: None,
    }
}

fn runtime(capacity: u64, stock: u64) -> Runtime {
    Runtime::compile(Snapshot {
        custody: State {
            tick: 0,
            revision: 0,
            containers: vec![
                Container::new(
                    Place::Person(1),
                    capacity.max(1),
                    if capacity == 0 {
                        vec![Lot {
                            good: 1,
                            quantity: 1,
                            expires_at: Some(5),
                        }]
                    } else {
                        vec![]
                    },
                )
                .unwrap(),
                Container::new(
                    Place::Person(2),
                    10,
                    if stock == 0 {
                        vec![]
                    } else {
                        vec![Lot {
                            good: 1,
                            quantity: stock,
                            expires_at: Some(5),
                        }]
                    },
                )
                .unwrap(),
            ],
        },
        goods: vec![Good {
            id: 1,
            name: "goods".into(),
            food: None,
        }],
        recipes: vec![],
        tick_millis: 1000,
        work: vec![],
    })
    .unwrap()
}

fn plan(
    owner: &Taking,
    people: &[Person],
    manual: Option<Intent>,
) -> game_parallel_economy_taking::Plan {
    let locations = Colocation::new(None);
    Context {
        owner,
        people,
        opinions: None,
        locations: &locations,
        busy: &[],
        blocked: &[],
        can_touch: &|_, _| true,
        max_hunger: 100,
        manual,
        action_base: 10,
        seed: 7,
        tick: 1,
    }
    .prepare()
    .unwrap()
}

#[test]
fn unnoticed_taking_moves_only_admitted_possessions() {
    let owner = Taking {
        policies: vec![policy(1, 2)].into(),
        ..Default::default()
    };
    let people = people(2);
    let mut plan = plan(&owner, &people, None);
    assert_eq!(plan.actors, vec![1]);
    assert!(plan.contests.is_empty());
    plan.rebase(20).unwrap();
    assert_eq!(plan.actions[0].id, 20);
    assert_eq!(plan.actions[0].movements[0].id, 20);
    assert!(plan.rebase(u64::MAX).is_err());
    let mut physical = runtime(5, 1);
    let before = physical.snapshot();
    let draft = physical
        .prepare_with_choices(&[], &plan.actions, &[], &[], 7)
        .unwrap();
    assert_eq!(physical.snapshot(), before);
    let events = plan.publish(draft.events(), &[]);
    assert_eq!(events[0].outcome, Outcome::Transferred);
    assert!(!events[0].noticed);
    physical.commit_advance(draft).unwrap();
    assert_eq!(
        physical.custody(Place::Person(1)).unwrap().lots[0].quantity,
        1
    );
    assert!(physical.custody(Place::Person(2)).unwrap().lots.is_empty());
    let mut blocked = runtime(0, 1);
    let draft = blocked
        .prepare_with_choices(&[], &plan.actions, &[], &[], 7)
        .unwrap();
    assert_eq!(
        plan.publish(draft.events(), &[])[0].outcome,
        Outcome::Unavailable
    );
    blocked.commit_advance(draft).unwrap();
    assert_eq!(
        blocked.custody(Place::Person(2)).unwrap().lots[0].quantity,
        1
    );
}

#[test]
fn resistance_and_escalation_are_separate_personal_choices() {
    let mut owner = Taking {
        policies: vec![policy(1, 2)].into(),
        responses: vec![Response {
            person: 2,
            notices: true,
            resist: true,
            resist_if_fear_at_least: None,
            resist_if_affinity_at_least: None,
            resist_if_affinity_at_most: None,
        }]
        .into(),
        events: vec![],
        ..Default::default()
    };
    let mut people = people(2);
    people[1].controlled = true;
    let opposed = plan(&owner, &people, None);
    assert_eq!(opposed.actors, vec![1, 2]);
    assert!(opposed.actions.is_empty());
    assert_eq!(opposed.contests.len(), 1);
    let physical = runtime(5, 1);
    let draft = physical
        .prepare_with_choices(&[], &[], &[], &[], 7)
        .unwrap();
    assert_eq!(
        opposed.publish(draft.events(), &[])[0].outcome,
        Outcome::Resisted
    );
    assert_eq!(
        opposed.publish(draft.events(), &[(1, 2)])[0].outcome,
        Outcome::Conflict
    );
    owner.policies_mut()[0].escalate = false;
    let withdrawn = plan(&owner, &people, None);
    assert!(withdrawn.contests.is_empty());
    assert_eq!(
        withdrawn.publish(draft.events(), &[(1, 2)])[0].outcome,
        Outcome::Resisted
    );
    owner.responses_mut()[0].resist = false;
    let unopposed = plan(&owner, &people, None);
    assert_eq!(unopposed.actions.len(), 1);
    assert!(unopposed.contests.is_empty());
    let saved: Taking = serde_json::from_str(&serde_json::to_string(&owner).unwrap()).unwrap();
    assert_eq!(saved, owner);
}

#[test]
fn a_hot_target_and_crossed_attempts_remain_disjoint_across_workers() {
    let count = 4096;
    let people = people(count + 1);
    let owner = Taking {
        policies: (1..=count)
            .map(|person| policy(person, count + 1))
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let result = plan(&owner, &people, None);
                assert_eq!(result.actors.len(), 1);
                assert_eq!(result.actions.len(), 1);
                (result.actors, result.actions)
            })
    };
    assert_eq!(run(1), run(4));
    let crossed = Taking {
        policies: vec![policy(1, 2), policy(2, 1)].into(),
        ..Default::default()
    };
    let result = plan(&crossed, &people, None);
    assert_eq!(result.actors.len(), 1);
    assert_eq!(result.actions.len(), 1);
}

#[test]
fn barriers_and_controlled_intent_have_physical_priority() {
    let mut people = people(3);
    people[0].controlled = true;
    let owner = Taking {
        policies: vec![policy(3, 2)].into(),
        ..Default::default()
    };
    let manual = Intent {
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        escalate: false,
    };
    let result = plan(&owner, &people, Some(manual));
    assert_eq!(result.actors, vec![1]);
    assert_eq!(result.actions[0].movements[0].to, Some(Place::Person(1)));
    let locations = Colocation::new(None);
    let blocked = Context {
        owner: &owner,
        people: &people,
        opinions: None,
        locations: &locations,
        busy: &[],
        blocked: &[],
        can_touch: &|_, _| false,
        max_hunger: 100,
        manual: Some(manual),
        action_base: 10,
        seed: 7,
        tick: 1,
    };
    assert!(blocked.prepare().is_err());
}

#[test]
fn participants_interpret_only_experiences_they_notice_using_their_own_rules() {
    use game_parallel_economy_taking::{Event, Interpretation, Reaction};
    use game_parallel_opinions::SubjectiveReaction;
    let response = |affinity| {
        Some(SubjectiveReaction {
            affinity,
            ..Default::default()
        })
    };
    let owner = Taking {
        reactions: vec![
            Reaction {
                person: 1,
                as_actor: Interpretation {
                    transferred: response(-2),
                    ..Default::default()
                },
                ..Default::default()
            },
            Reaction {
                person: 2,
                as_target: Interpretation {
                    transferred: response(3),
                    ..Default::default()
                },
                ..Default::default()
            },
            Reaction {
                person: 2,
                other: Some(1),
                as_target: Interpretation {
                    transferred: response(0),
                    ..Default::default()
                },
                ..Default::default()
            },
        ]
        .into(),
        ..Default::default()
    };
    let event = Event {
        tick: 1,
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        noticed: true,
        outcome: Outcome::Transferred,
    };
    let changes = owner.personal_changes(std::slice::from_ref(&event));
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].affinity, -2);
    assert_eq!(changes[1].affinity, 0);
    let unaware = Event {
        noticed: false,
        ..event
    };
    let changes = owner.personal_changes(&[unaware]);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].observer, 1);
}

#[test]
fn fear_can_motivate_an_unwilling_actor_and_the_target_uses_their_own_affinity() {
    use game_parallel_opinions::{AffinityRules, Opinion, Opinions, Rules};
    let mut actor = policy(1, 2);
    actor.willing = false;
    actor.pressured_by = Some(2);
    actor.pressure_threshold = Some(3);
    let owner = Taking {
        policies: vec![actor].into(),
        responses: vec![Response {
            person: 2,
            notices: true,
            resist: false,
            resist_if_fear_at_least: None,
            resist_if_affinity_at_least: Some(2),
            resist_if_affinity_at_most: None,
        }]
        .into(),
        ..Default::default()
    };
    let opinions = Opinions {
        rules: Rules {
            minimum: -10,
            maximum: 10,
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
            affinity: Some(AffinityRules {
                limit: 10,
                encounter_change: 0,
                pressured_encounter_change: 0,
                help_change: 0,
                harm_change_per_damage: 0,
            }),
            newborn_obligation_development: None,
        },
        sensitivities: vec![].into(),
        records: vec![
            Opinion {
                observer: 1,
                target: 2,
                trust: 0,
                affinity: 0,
                fear: 3,
                obligation: 0,
            },
            Opinion {
                observer: 2,
                target: 1,
                trust: 0,
                affinity: 2,
                fear: 0,
                obligation: 0,
            },
        ],
        events: vec![],
    };
    let people = people(2);
    let locations = Colocation::new(None);
    let run = |feelings| {
        Context {
            owner: &owner,
            people: &people,
            opinions: feelings,
            locations: &locations,
            busy: &[],
            blocked: &[],
            can_touch: &|_, _| true,
            max_hunger: 100,
            manual: None,
            action_base: 10,
            seed: 7,
            tick: 1,
        }
        .prepare()
        .unwrap()
    };
    assert!(run(None).actors.is_empty());
    let plan = run(Some(&opinions));
    assert_eq!(plan.actors, vec![1, 2]);
    assert!(plan.actions.is_empty());
    assert_eq!(plan.contests.len(), 1);
}

#[test]
#[ignore = "manual contested taking benchmark"]
fn taking_selection_benchmark() {
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let shared = std::env::var_os("LEGACY_SHARED").is_some();
    let people = people(count * 2);
    let owner = Taking {
        policies: (1..=count)
            .map(|id| policy(id, if shared { count + 1 } else { count + id }))
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..5 {
                let now = std::time::Instant::now();
                let next = plan(&owner, &people, None);
                let elapsed = now.elapsed().as_secs_f64();
                assert_eq!(next.actions.len(), if shared { 1 } else { count as usize });
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} attempts={count} shared={shared} elapsed={:.6}",
                samples[1]
            );
        });
}

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
    let mut receipts = physical.commit_advance(draft).unwrap();
    receipts.physical_actions.insert(
        0,
        game_parallel_lots::TransactionOutcome {
            id: 1,
            accepted: false,
        },
    );
    assert_eq!(
        plan.publish(&receipts, &[])[0].outcome,
        Outcome::Transferred
    );
    receipts.physical_actions.reverse();
    assert_eq!(
        plan.publish(&receipts, &[])[0].outcome,
        Outcome::Transferred
    );
    receipts.physical_actions.clear();
    assert_eq!(
        plan.publish(&receipts, &[])[0].outcome,
        Outcome::Unavailable
    );
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
fn witnesses_require_attention_and_visibility_and_choose_their_own_reaction() {
    use game_parallel_economy_taking::{Event, Interpretation, Witness};
    use game_parallel_opinions::SubjectiveReaction;
    use std::collections::HashSet;
    let witness = Witness {
        person: 3,
        actor: 1,
        target: Some(2),
        notices: true,
        toward_actor: Interpretation {
            transferred: Some(SubjectiveReaction {
                affinity: 4,
                obligation: 0,
                ..Default::default()
            }),
            ..Default::default()
        },
        toward_target: Interpretation {
            transferred: Some(SubjectiveReaction {
                affinity: -3,
                ..Default::default()
            }),
            ..Default::default()
        },
        last_seen: None,
    };
    let mut owner = Taking {
        witnesses: vec![witness.clone()].into(),
        ..Default::default()
    };
    let event = Event {
        tick: 1,
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        noticed: false,
        outcome: Outcome::Transferred,
    };
    assert!(
        owner
            .validate(&HashSet::from([1, 2, 3]), &HashSet::from([1]), 1)
            .is_ok()
    );
    assert!(
        owner
            .witness_changes(std::slice::from_ref(&event), &|_, _| false)
            .is_empty()
    );
    let changes = owner.witness_changes(std::slice::from_ref(&event), &|_, _| true);
    assert_eq!(changes.len(), 2);
    assert_eq!(
        (
            changes[0].observer,
            changes[0].target,
            changes[0].affinity,
            changes[0].obligation
        ),
        (3, 1, 4, 0)
    );
    assert_eq!(
        (changes[1].observer, changes[1].target, changes[1].affinity),
        (3, 2, -3)
    );
    owner.observe_witnesses(std::slice::from_ref(&event), &|_, _| false);
    assert!(owner.witnesses[0].last_seen.is_none());
    assert_eq!(
        owner.observe_witnesses(std::slice::from_ref(&event), &|_, _| true),
        changes
    );
    let seen = owner.witnesses[0].last_seen.unwrap();
    assert_eq!(
        (
            seen.tick,
            seen.target,
            seen.good,
            seen.quantity,
            seen.outcome
        ),
        (1, 2, 1, 1, Outcome::Transferred)
    );
    owner.witnesses_mut()[0].notices = false;
    assert!(
        owner
            .witness_changes(std::slice::from_ref(&event), &|_, _| true)
            .is_empty()
    );
    owner.witnesses_mut().push(witness);
    assert!(
        owner
            .validate(&HashSet::from([1, 2, 3]), &HashSet::from([1]), 1)
            .is_err()
    );
}

#[test]
fn many_witnesses_independently_interpret_one_event_across_workers() {
    use game_parallel_economy_taking::{Event, Interpretation, Witness};
    use game_parallel_opinions::SubjectiveReaction;
    let owner = Taking {
        witnesses: (3..=4098)
            .map(|person| Witness {
                person,
                actor: 1,
                target: Some(2),
                notices: true,
                toward_actor: Interpretation {
                    transferred: Some(SubjectiveReaction {
                        affinity: if person % 2 == 0 { 1 } else { -1 },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let event = Event {
        tick: 1,
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        noticed: false,
        outcome: Outcome::Transferred,
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let changes = owner.witness_changes(std::slice::from_ref(&event), &|_, _| true);
                assert_eq!(changes.len(), 4096);
                assert!(changes.iter().all(|change| change.target == 1
                    && change.affinity == if change.observer % 2 == 0 { 1 } else { -1 }));
                changes
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn remembered_reports_can_be_false_and_listeners_choose_their_own_belief() {
    use game_parallel_economy_taking::{
        Event, Interpretation, Reception, ReportBelief, ReportPolicy, Stance, Witness,
    };
    use game_parallel_opinions::SubjectiveReaction;
    use std::collections::HashSet;
    let mut previous = Taking {
        witnesses: vec![Witness {
            person: 3,
            actor: 1,
            notices: true,
            ..Default::default()
        }]
        .into(),
        report_policies: vec![ReportPolicy {
            person: 3,
            recipient: 4,
            actor: 1,
            willing: false,
            pressure_threshold: Some(3),
            claimed_outcome: Some(Outcome::Resisted),
            source: Default::default(),
            heard_from: None,
        }]
        .into(),
        receptions: vec![Reception {
            person: 4,
            stance: Stance::TrustAtLeast(2),
            toward_actor: Interpretation {
                resisted: Some(SubjectiveReaction {
                    affinity: -3,
                    ..Default::default()
                }),
                ..Default::default()
            },
            toward_target: Interpretation {
                resisted: Some(SubjectiveReaction {
                    affinity: 2,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };
    let event = Event {
        tick: 1,
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        noticed: false,
        outcome: Outcome::Transferred,
    };
    previous.observe_witnesses(&[event], &|_, _| true);
    let mut next = previous.clone();
    assert!(
        next.receive_reports(&previous, &[(3, 4)], 1, &|_, _| (3, 3, 0))
            .is_empty()
    );
    assert!(
        next.receive_reports(&previous, &[], 2, &|_, _| (3, 3, 0))
            .is_empty()
    );
    assert!(next.reports.is_empty());
    assert!(
        next.receive_reports(&previous, &[(3, 4)], 2, &|_, _| (3, 2, 0))
            .is_empty()
    );
    assert!(next.reports.is_empty());
    let changes = next.receive_reports(&previous, &[(4, 3)], 2, &|_, _| (3, 0, 3));
    assert_eq!(changes.len(), 2);
    assert_eq!(
        (changes[0].observer, changes[0].target, changes[0].affinity),
        (4, 1, -3)
    );
    assert_eq!(
        (changes[1].observer, changes[1].target, changes[1].affinity),
        (4, 2, 2)
    );
    assert_eq!(
        previous.witnesses[0].last_seen.unwrap().outcome,
        Outcome::Transferred
    );
    assert_eq!(
        (
            next.reports.get(4)[0].claimed_outcome,
            next.reports.get(4)[0].belief
        ),
        (Outcome::Resisted, ReportBelief::Believes)
    );
    assert!(
        next.validate(&HashSet::from([1, 2, 3, 4]), &HashSet::from([1]), 2)
            .is_ok()
    );
    let saved: Taking = serde_json::from_str(&serde_json::to_string(&next).unwrap()).unwrap();
    assert_eq!(saved, next);
    let mut old_save = serde_json::to_value(&next).unwrap();
    for policy in old_save["report_policies"].as_array_mut().unwrap() {
        policy.as_object_mut().unwrap().remove("source");
        policy.as_object_mut().unwrap().remove("heard_from");
    }
    let restored: Taking = serde_json::from_value(old_save).unwrap();
    assert_eq!(restored, next);
    let mut repeated = next.clone();
    assert!(
        repeated
            .receive_reports(&next, &[(3, 4)], 3, &|_, _| (3, 3, 0))
            .is_empty()
    );
    assert_eq!(repeated.reports, next.reports);
    let mut skeptical = previous.clone();
    let changes = skeptical.receive_reports(&previous, &[(3, 4)], 2, &|_, _| (-1, 3, 0));
    assert!(changes.is_empty());
    assert_eq!(skeptical.reports.get(4)[0].belief, ReportBelief::Doubts);
    let mut predisposed = previous.clone();
    predisposed.receptions_mut()[0].doubted_toward_actor = Interpretation {
        resisted: Some(SubjectiveReaction {
            affinity: 5,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut skeptical = predisposed.clone();
    let changes = skeptical.receive_reports(&predisposed, &[(3, 4)], 2, &|_, _| (-1, 3, 0));
    assert_eq!(changes.len(), 1);
    assert_eq!(
        (changes[0].observer, changes[0].target, changes[0].affinity),
        (4, 1, 5)
    );
    let mut source = next.clone();
    source.report_policies_mut().push(ReportPolicy {
        person: 4,
        recipient: 5,
        actor: 1,
        willing: true,
        pressure_threshold: None,
        claimed_outcome: None,
        source: game_parallel_economy_taking::ReportSource::Heard,
        heard_from: Some(3),
    });
    let mut forwarded = source.clone();
    assert!(
        forwarded
            .receive_reports(&source, &[(4, 5)], 2, &|_, _| (0, 0, 0))
            .is_empty()
    );
    assert_eq!(forwarded.reports.len(), 1);
    assert!(
        forwarded
            .receive_reports(&source, &[(4, 5)], 3, &|_, _| (0, 0, 0))
            .is_empty()
    );
    let report = forwarded.reports.iter().find(|r| r.recipient == 5).unwrap();
    assert_eq!(
        (
            report.speaker,
            report.source_speaker,
            report.claimed_outcome,
            report.belief
        ),
        (4, Some(3), Outcome::Resisted, ReportBelief::Unassessed)
    );
    assert!(
        forwarded
            .validate(&HashSet::from([1, 2, 3, 4, 5]), &HashSet::from([1]), 3)
            .is_ok()
    );
    let mut wrong_source = source.clone();
    wrong_source
        .report_policies_mut()
        .last_mut()
        .unwrap()
        .heard_from = Some(2);
    let mut none = wrong_source.clone();
    none.receive_reports(&wrong_source, &[(4, 5)], 3, &|_, _| (0, 0, 0));
    assert_eq!(none.reports.len(), 1);
    let mut multiple = source.clone();
    let mut earlier = multiple.reports.get(4)[0].clone();
    earlier.speaker = 2;
    earlier.received_at = 1;
    earlier.claimed_outcome = Outcome::Unavailable;
    multiple.reports = vec![earlier, multiple.reports.get(4)[0].clone()].into();
    multiple
        .report_policies_mut()
        .last_mut()
        .unwrap()
        .heard_from = Some(2);
    let mut selected = multiple.clone();
    selected.receive_reports(&multiple, &[(4, 5)], 3, &|_, _| (0, 0, 0));
    let report = selected.reports.iter().find(|r| r.recipient == 5).unwrap();
    assert_eq!(
        (report.source_speaker, report.claimed_outcome),
        (Some(2), Outcome::Unavailable)
    );
    let mut doubtful = skeptical.clone();
    doubtful
        .report_policies_mut()
        .push(source.report_policies.last().unwrap().clone());
    let mut forwarded = doubtful.clone();
    forwarded.receive_reports(&doubtful, &[(4, 5)], 3, &|_, _| (0, 0, 0));
    assert_eq!(doubtful.reports.get(4)[0].belief, ReportBelief::Doubts);
    assert_eq!(
        forwarded
            .reports
            .iter()
            .find(|r| r.recipient == 5)
            .unwrap()
            .belief,
        ReportBelief::Unassessed
    );
}

#[test]
fn concentrated_reports_are_bounded_and_deterministic() {
    use game_parallel_economy_taking::{
        Interpretation, ObservedTaking, Reception, ReportPolicy, Stance, Witness,
    };
    use game_parallel_opinions::SubjectiveReaction;
    let observers = 3..=1026;
    let previous = Taking {
        witnesses: observers
            .clone()
            .map(|person| Witness {
                person,
                actor: 1,
                notices: true,
                last_seen: Some(ObservedTaking {
                    tick: 1,
                    target: 2,
                    good: 1,
                    quantity: 1,
                    outcome: Outcome::Transferred,
                }),
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into(),
        report_policies: observers
            .clone()
            .map(|person| ReportPolicy {
                person,
                recipient: 5000,
                actor: 1,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: Default::default(),
                heard_from: None,
            })
            .collect::<Vec<_>>()
            .into(),
        receptions: vec![Reception {
            person: 5000,
            stance: Stance::Believe,
            toward_actor: Interpretation {
                transferred: Some(SubjectiveReaction {
                    affinity: 1,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };
    let contacts: Vec<_> = observers.map(|person| (person, 5000)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = previous.clone();
                let changes = next.receive_reports(&previous, &contacts, 2, &|_, _| (0, 0, 0));
                assert_eq!(changes.len(), 64);
                assert!(changes.iter().all(|change| change.observer == 5000
                    && change.target == 1
                    && change.affinity == 1));
                assert_eq!(next.reports.len(), 64);
                assert_eq!(next.reports.first().unwrap().speaker, 3);
                assert_eq!(next.reports.last().unwrap().speaker, 66);
                next
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn sparse_and_multiple_witness_keys_match_reversed_contacts() {
    use game_parallel_economy_taking::{ObservedTaking, ReportPolicy, Witness};
    let observation = ObservedTaking {
        tick: 1,
        target: 2,
        good: 1,
        quantity: 1,
        outcome: Outcome::Transferred,
    };
    let previous = Taking {
        witnesses: [(30, 5), (10, 7), (10, 1)]
            .into_iter()
            .map(|(person, actor)| Witness {
                person,
                actor,
                notices: true,
                last_seen: Some(observation),
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into(),
        report_policies: [(30, 3, 5), (10, 4, 7), (10, 3, 1)]
            .into_iter()
            .map(|(person, recipient, actor)| ReportPolicy {
                person,
                recipient,
                actor,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: Default::default(),
                heard_from: None,
            })
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let contacts = [(30, 3), (10, 3), (10, 4)];
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = previous.clone();
                next.receive_reports(&previous, &contacts, 2, &|_, _| (0, 0, 0));
                assert_eq!(next.reports.len(), 3);
                assert_eq!(
                    next.reports
                        .iter()
                        .map(|r| (r.recipient, r.speaker, r.actor))
                        .collect::<Vec<_>>(),
                    vec![(3, 10, 1), (3, 30, 5), (4, 10, 7)]
                );
                next
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn participants_can_report_their_own_experiences_only_if_noticed() {
    use game_parallel_economy_taking::{Event, ReportPolicy, Witness};
    use std::collections::HashSet;
    let mut previous = Taking {
        witnesses: vec![
            Witness {
                person: 1,
                actor: 1,
                target: Some(2),
                notices: true,
                ..Default::default()
            },
            Witness {
                person: 2,
                actor: 1,
                target: Some(2),
                notices: true,
                ..Default::default()
            },
        ]
        .into(),
        report_policies: vec![
            ReportPolicy {
                person: 1,
                recipient: 3,
                actor: 1,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: Default::default(),
                heard_from: None,
            },
            ReportPolicy {
                person: 2,
                recipient: 3,
                actor: 1,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: Default::default(),
                heard_from: None,
            },
        ]
        .into(),
        ..Default::default()
    };
    let mut event = Event {
        tick: 1,
        actor: 1,
        target: 2,
        good: 1,
        quantity: 1,
        noticed: false,
        outcome: Outcome::Transferred,
    };
    assert!(
        previous
            .observe_witnesses(std::slice::from_ref(&event), &|_, _| true)
            .is_empty()
    );
    assert!(previous.witnesses[0].last_seen.is_some());
    assert!(previous.witnesses[1].last_seen.is_none());
    event.noticed = true;
    previous.observe_witnesses(&[event], &|_, _| true);
    assert!(previous.witnesses[1].last_seen.is_some());
    let mut next = previous.clone();
    next.receive_reports(&previous, &[(1, 3), (2, 3)], 2, &|_, _| (0, 0, 0));
    assert_eq!(next.reports.len(), 2);
    assert_eq!(
        (
            next.reports.get(3)[0].speaker,
            next.reports.get(3)[1].speaker
        ),
        (1, 2)
    );
    assert!(
        next.validate(&HashSet::from([1, 2, 3]), &HashSet::from([1]), 2)
            .is_ok()
    );
}

#[test]
#[ignore = "manual personal testimony benchmark"]
fn testimony_benchmark() {
    use game_parallel_economy_taking::{ObservedTaking, ReportPolicy, Witness};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let shared = std::env::var_os("LEGACY_SHARED").is_some();
    let speakers = 3..count + 3;
    let recipient = |person| {
        if shared {
            2 * count + 10
        } else {
            count + person
        }
    };
    let previous = Taking {
        witnesses: speakers
            .clone()
            .map(|person| Witness {
                person,
                actor: 1,
                notices: true,
                last_seen: Some(ObservedTaking {
                    tick: 1,
                    target: 2,
                    good: 1,
                    quantity: 1,
                    outcome: Outcome::Transferred,
                }),
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into(),
        report_policies: speakers
            .clone()
            .map(|person| ReportPolicy {
                person,
                recipient: recipient(person),
                actor: 1,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: Default::default(),
                heard_from: None,
            })
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let contacts: Vec<_> = speakers.map(|person| (person, recipient(person))).collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..6 {
                let mut next = previous.clone();
                let start = std::time::Instant::now();
                let changes = next.receive_reports(&previous, &contacts, 2, &|_, _| (0, 0, 0));
                let elapsed = start.elapsed().as_secs_f64();
                assert!(changes.is_empty());
                assert_eq!(next.reports.len(), if shared { 64 } else { count as usize });
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} reports={count} shared={shared} elapsed={:.6}",
                samples[2]
            );
        });
}

#[test]
#[ignore = "manual relayed testimony benchmark"]
fn relayed_testimony_benchmark() {
    use game_parallel_economy_taking::{Report, ReportBelief, ReportPolicy, ReportSource};
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(100_000);
    let active = std::env::var("LEGACY_ACTIVE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(count)
        .min(count);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let speakers = 3..count + 3;
    let previous = Taking {
        reports: speakers
            .clone()
            .map(|recipient| Report {
                speaker: 2 * count + recipient,
                recipient,
                actor: 1,
                target: 2,
                good: 1,
                quantity: 1,
                observed_at: 0,
                received_at: 1,
                claimed_outcome: Outcome::Transferred,
                belief: ReportBelief::Unassessed,
                source_speaker: None,
            })
            .collect::<Vec<_>>()
            .into(),
        report_policies: speakers
            .clone()
            .take(active as usize)
            .map(|person| ReportPolicy {
                person,
                recipient: count + person,
                actor: 1,
                willing: true,
                pressure_threshold: None,
                claimed_outcome: None,
                source: ReportSource::Heard,
                heard_from: None,
            })
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    let contacts: Vec<_> = speakers
        .take(active as usize)
        .map(|person| (person, count + person))
        .collect();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let mut samples = Vec::new();
            for round in 0..6 {
                let mut next = previous.clone();
                let start = std::time::Instant::now();
                let changes = next.receive_reports(&previous, &contacts, 2, &|_, _| (0, 0, 0));
                let elapsed = start.elapsed().as_secs_f64();
                assert!(changes.is_empty());
                assert_eq!(next.reports.len(), (count + active) as usize);
                assert_eq!(
                    next.reports
                        .iter()
                        .filter(|r| r.source_speaker.is_some())
                        .count(),
                    active as usize
                );
                if round > 0 {
                    samples.push(elapsed);
                }
            }
            samples.sort_by(f64::total_cmp);
            println!(
                "workers={workers} history={count} relays={active} elapsed={:.6}",
                samples[2]
            );
            #[cfg(feature = "profile")]
            for (name, measurement) in game_diagnostics::snapshot() {
                println!(
                    "{name}: {:.3} ms/call",
                    measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0
                );
            }
        });
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

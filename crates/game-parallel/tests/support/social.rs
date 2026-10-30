use game_parallel::{Move, Person, SocialParticipant, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/social.toml")).unwrap()
}
#[test]
fn local_willing_people_build_relationships_and_travel_changes_encounters() {
    let mut w = world();
    w.step().unwrap();
    let social = w.social.as_ref().unwrap();
    assert_eq!(social.events.len(), 1);
    assert_eq!(
        (
            social.events[0].a,
            social.events[0].b,
            social.events[0].region
        ),
        (1, 2, 1)
    );
    assert_eq!(social.relationships[0].familiarity, 2);
    w.step().unwrap();
    assert_eq!(w.social.as_ref().unwrap().relationships[0].encounters, 2);
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.move_people(&[Move {
        person: 2,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    assert!(w.social.as_ref().unwrap().events.is_empty());
    w.step().unwrap();
    assert!(w.social.as_ref().unwrap().events.is_empty());
    w.step().unwrap();
    let social = w.social.as_ref().unwrap();
    assert_eq!(
        (
            social.events[0].a,
            social.events[0].b,
            social.events[0].region
        ),
        (2, 3, 2)
    );
    assert_eq!(social.relationships.len(), 2);
    w.validate().unwrap();
}

#[test]
fn unwilling_or_incapacitated_people_do_not_meet() {
    let mut w = world();
    w.social.as_mut().unwrap().participants[1].willing = false;
    w.step().unwrap();
    assert!(w.social.as_ref().unwrap().events.is_empty());
    w.social.as_mut().unwrap().participants[1].willing = true;
    w.regions[0].people[1].health = 0;
    w.step().unwrap();
    assert!(w.social.as_ref().unwrap().events.is_empty());
}

#[test]
fn social_overflow_and_invalid_state_reject_tick_without_mutation() {
    use game_parallel::Relationship;
    let mut w = world();
    w.social.as_mut().unwrap().relationships.push(Relationship {
        a: 1,
        b: 2,
        familiarity: 1,
        encounters: u64::MAX,
        last_met: 0,
    });
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
    let mut w = world();
    w.social.as_mut().unwrap().participants[1].person = 1;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn failed_family_tick_does_not_publish_prepared_social_events() {
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.social = world().social;
    w.families.as_mut().unwrap().members[7].age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn large_local_encounters_are_independent_of_layout_and_workers() {
    let mut base = world();
    base.rules.food_growth = 0;
    base.regions[0].people = (1..=10000)
        .map(|id| Person {
            id,
            hunger: 0,
            health: 100,
        })
        .collect();
    base.regions[1].people.clear();
    base.social.as_mut().unwrap().participants = (1..=10000)
        .map(|person| SocialParticipant {
            person,
            willing: person % 7 != 0,
        })
        .collect();
    let mut outcomes = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = base.clone();
        if workers > 1 {
            w.regions[0].people.reverse();
            w.social.as_mut().unwrap().participants.reverse();
        }
        pool.install(|| {
            for _ in 0..3 {
                w.step().unwrap();
            }
        });
        w.regions[0].people.sort_unstable_by_key(|p| p.id);
        w.social
            .as_mut()
            .unwrap()
            .participants
            .sort_unstable_by_key(|p| p.person);
        assert!(!w.social.as_ref().unwrap().relationships.is_empty());
        w.validate().unwrap();
        outcomes.push(w);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn social_isolation_grows_without_contact_and_recovers_through_encounters() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    world.step().unwrap();
    let needs = world.social.as_ref().unwrap().needs.as_ref().unwrap();
    assert_eq!(
        needs
            .people
            .iter()
            .map(|row| (row.person, row.isolation))
            .collect::<Vec<_>>(),
        vec![(3, 2)]
    );
    world.step().unwrap();
    assert_eq!(
        world
            .social
            .as_ref()
            .unwrap()
            .needs
            .as_ref()
            .unwrap()
            .people[0]
            .isolation,
        4
    );
    let saved = serde_json::to_string(&world).unwrap();
    let mut loaded = World::from_json(&saved).unwrap();
    world.step().unwrap();
    loaded.step().unwrap();
    assert_eq!(world, loaded);
}
#[test]
fn people_with_greatest_isolation_are_prioritized_for_limited_contact() {
    let mut world = World::from_toml(include_str!(
        "../../../../content/parallel/social_needs.toml"
    ))
    .unwrap();
    let third = world.regions[1].people.pop().unwrap();
    world.regions[0].people.push(third);
    world
        .social
        .as_mut()
        .unwrap()
        .needs
        .as_mut()
        .unwrap()
        .people
        .push(game_parallel_social::Need {
            person: 3,
            isolation: 10,
        });
    world.step().unwrap();
    let social = world.social.unwrap();
    assert_eq!(social.events.len(), 1);
    assert!(social.events[0].a == 3 || social.events[0].b == 3);
    assert_eq!(
        social
            .needs
            .unwrap()
            .people
            .iter()
            .find(|row| row.person == 3)
            .unwrap()
            .isolation,
        9
    );
}

#[test]
fn parallel_relationship_merge_matches_independent_keyed_updates_across_ranges() {
    use std::collections::BTreeMap;
    let mut state = world();
    state.rules.hunger_per_tick = 0;
    state.regions[0].people = (1..=12000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state.social.as_mut().unwrap().participants = (1..=12000)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    for _ in 0..4 {
        let before = state.social.as_ref().unwrap().clone();
        state.step().unwrap();
        let social = state.social.as_ref().unwrap();
        let mut expected: BTreeMap<_, _> = before
            .relationships
            .into_iter()
            .map(|r| ((r.a, r.b), r))
            .collect();
        for event in &social.events {
            expected
                .entry((event.a, event.b))
                .and_modify(|r| {
                    r.familiarity = r
                        .familiarity
                        .saturating_add(social.rules.familiarity_gain)
                        .min(social.rules.familiarity_max);
                    r.encounters += 1;
                    r.last_met = state.tick;
                })
                .or_insert_with(|| game_parallel::Relationship {
                    a: event.a,
                    b: event.b,
                    familiarity: social
                        .rules
                        .familiarity_gain
                        .min(social.rules.familiarity_max),
                    encounters: 1,
                    last_met: state.tick,
                });
        }
        assert_eq!(
            social.relationships,
            expected.into_values().collect::<Vec<_>>()
        );
    }
}

#[test]
fn stale_relationships_expire_on_idle_ticks_and_later_meetings_start_fresh() {
    let mut state = world();
    state.social.as_mut().unwrap().rules.relationship_retention = Some(1);
    state.step().unwrap();
    assert_eq!(
        state.social.as_ref().unwrap().relationships[0].encounters,
        1
    );
    state.social.as_mut().unwrap().rules.interval = 3;
    state.social.as_mut().unwrap().participants[1].willing = false;
    state.step().unwrap();
    assert_eq!(state.social.as_ref().unwrap().relationships.len(), 1);
    state.step().unwrap();
    assert!(state.social.as_ref().unwrap().relationships.is_empty());
    state.social.as_mut().unwrap().participants[1].willing = true;
    for _ in 0..3 {
        state.step().unwrap();
    }
    let relationship = &state.social.as_ref().unwrap().relationships[0];
    assert_eq!(relationship.encounters, 1);
    assert_eq!(relationship.last_met, 6);
    let reloaded = World::from_json(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(state, reloaded);
}

#[test]
fn finite_relationship_retention_bounds_long_running_graph_across_workers() {
    let mut base = world();
    base.rules.hunger_per_tick = 0;
    base.regions[0].people = (1..=4000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    base.regions[1].people.clear();
    let social = base.social.as_mut().unwrap();
    social.participants = (1..=4000)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    social.rules.relationship_retention = Some(4);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = base.clone();
                for _ in 0..16 {
                    state.step().unwrap();
                    assert!(state.social.as_ref().unwrap().relationships.len() <= 5 * 2000);
                }
                state
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn social_needs_handle_sorted_and_reordered_contacts_equally() {
    use game_parallel_social::{Contact, Encounter, Need, NeedRules, Needs};
    let people: Vec<_> = (1..=10_000)
        .map(|person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    let contacts: Vec<_> = people
        .iter()
        .map(|row| Contact {
            person: row.person,
            region: Some(1),
            health: 100,
            hunger: 0,
            busy: false,
        })
        .collect();
    let needs = Needs {
        rules: NeedRules {
            capacity: 12,
            isolation_gain: 2,
            encounter_relief: 3,
        },
        people: vec![
            Need {
                person: 2,
                isolation: 6,
            },
            Need {
                person: 9_000,
                isolation: 3,
            },
        ],
    };
    let events = [Encounter {
        tick: 1,
        region: 1,
        a: 1,
        b: 2,
        pressured: None,
    }];
    let expected = needs.advance(&people, &contacts, &events);
    let mut reversed = contacts.clone();
    reversed.reverse();
    assert_eq!(needs.advance(&people, &reversed, &events), expected);
}

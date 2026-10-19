mod support;
use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = support::builder();
    game_traits::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_companionship::plugin(&mut b).unwrap();
    b
}
fn population(count: usize) -> Host {
    population_with_duration(count, SimDuration(5 * MINUTE.0))
}
fn population_with_duration(count: usize, duration: SimDuration) -> Host {
    let mut h = builder().build(9, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_social_life::Configure(game_social_life::Rules {
        duration,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Courtyard".into(),
        })
        .unwrap();
    let mut people = vec![];
    for index in 0..count {
        let name = ["A", "B", "C"]
            .get(index)
            .map_or_else(|| format!("P{index}"), |name| (*name).to_owned());
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        h.dispatch(game_social_life::Register { person, name })
            .unwrap();
        h.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
        people.push(person);
    }
    for i in 0..count {
        h.dispatch(game_relationships::SetRelationship {
            observer: people[i],
            other: people[(i + 1) % count],
            relationship: game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    h.dispatch(game_characters::Start).unwrap();
    h
}
#[test]
fn asymmetric_companionship_preferences_consume_time_and_cannot_cycle_at_one_instant() {
    let mut h = population(3);
    assert_eq!(
        h.advance(SimTime(10 * MINUTE.0), 2000).unwrap().stop,
        Stop::Target
    );
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        assert_eq!(
            host.advance(SimTime(30 * MINUTE.0), 4000).unwrap().stop,
            Stop::Target
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn mutually_willing_partners_complete_conversations_started_by_either_person() {
    let mut h = population(2);
    assert_eq!(
        h.advance(SimTime(30 * MINUTE.0), 4000).unwrap().stop,
        Stop::Target
    );
    assert!(
        h.get::<game_social_life::SocialLife>()
            .unwrap()
            .people
            .values()
            .all(|p| p.encounters > 0)
    );
}

#[test]
fn long_conversations_with_asymmetric_preferences_do_not_switch_forever_at_one_instant() {
    let mut h = population_with_duration(3, SimDuration(2 * HOUR.0));
    assert_eq!(
        h.advance(SimTime(10 * MINUTE.0), 2000).unwrap().stop,
        Stop::Target
    );
}

#[test]
fn a_willing_local_partner_is_reserved_by_the_timed_conversation_itself() {
    let mut h = population(2);
    support::run(&mut h, SimTime(MINUTE.0));
    assert_eq!(
        h.get::<game_social_life::SocialLife>()
            .unwrap()
            .meetings
            .len(),
        2
    );
    assert!(
        h.get::<Actions>()
            .unwrap()
            .0
            .values()
            .all(|a| a.kind == "socialize")
    );
}

#[test]
fn current_conversations_remain_known_outside_dense_and_top_eight_search_windows() {
    for count in [12, 24] {
        let mut h = population_with_duration(count, SimDuration(2 * HOUR.0));
        let people: Vec<_> = h
            .get::<game_social_life::SocialLife>()
            .unwrap()
            .people
            .keys()
            .copied()
            .collect();
        let (person, partner) = (people[0], people[1]);
        h.dispatch(game_relationships::SetRelationship {
            observer: person,
            other: partner,
            relationship: game_relationships::Relationship {
                affection: 99,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
        for other in people.iter().skip(2) {
            h.dispatch(game_relationships::SetRelationship {
                observer: person,
                other: *other,
                relationship: game_relationships::Relationship {
                    affection: 100,
                    trust: 100,
                    ..Default::default()
                },
            })
            .unwrap();
        }
        h.dispatch(game_social_life::Talk {
            person,
            other: partner,
        })
        .unwrap();
        h.dispatch(game_social_life::Scanned {
            person,
            through: partner,
            failed: false,
        })
        .unwrap();
        let action = h.get::<Actions>().unwrap().0[&person].id;
        let offers = h.dispatch(game_choice::Collect { person }).unwrap();
        let key = format!("company:{}", partner.0);
        let current: Vec<_> = offers.iter().filter(|p| p.key == key).collect();
        assert_eq!(
            current.len(),
            1,
            "current partner was lost in a crowd of {count}"
        );
        assert_eq!(current[0].continuation, Some(action));
        let saved = h.save().unwrap();
        let mut loaded = builder().load(&saved, 4).unwrap();
        for host in [&mut h, &mut loaded] {
            support::run(host, SimTime(10 * MINUTE.0));
            assert_eq!(
                host.get::<game_social_life::SocialLife>().unwrap().meetings[&person].other,
                partner
            );
            assert_eq!(host.get::<Actions>().unwrap().0[&person].id, action);
        }
        assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    }
}

#[test]
fn retaining_the_current_partner_still_allows_a_genuinely_better_alternative() {
    let mut h = population_with_duration(12, SimDuration(2 * HOUR.0));
    let people: Vec<_> = h
        .get::<game_social_life::SocialLife>()
        .unwrap()
        .people
        .keys()
        .copied()
        .collect();
    let (person, partner, other) = (people[0], people[1], people[2]);
    h.dispatch(game_social_life::Talk {
        person,
        other: partner,
    })
    .unwrap();
    h.dispatch(game_social_life::Scanned {
        person,
        through: partner,
        failed: false,
    })
    .unwrap();
    for (target, relationship) in [
        (
            partner,
            game_relationships::Relationship {
                affection: -100,
                grievance: 100,
                fear: 100,
                ..Default::default()
            },
        ),
        (
            other,
            game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        ),
    ] {
        h.dispatch(game_relationships::SetRelationship {
            observer: person,
            other: target,
            relationship,
        })
        .unwrap();
    }
    let proposals = h.dispatch(game_choice::Collect { person }).unwrap();
    let current = proposals
        .iter()
        .find(|p| p.key == format!("company:{}", partner.0))
        .unwrap();
    let choice = game_characters::choose(
        &proposals.iter().map(|p| p.motive).collect::<Vec<_>>(),
        Some(current.motive.id),
        20,
        false,
    )
    .unwrap();
    assert_ne!(choice.id, current.motive.id);
}

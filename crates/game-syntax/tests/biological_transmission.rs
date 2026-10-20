mod support;
use game_core::*;
use game_sim::*;
use game_syntax::*;
use support::*;
fn run(h: &mut Host, at: SimTime) {
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
}
fn birth_pair(h: &mut Host, mother: PersonId, father: PersonId) -> (PersonId, SimTime) {
    let id = h
        .dispatch(game_lifecycle::Conceive {
            mother,
            father,
            attendant: mother,
        })
        .unwrap();
    let due = h.get::<game_lifecycle::Population>().unwrap().pregnancies[&id].due;
    run(h, due);
    let child = *h
        .get::<game_lifecycle::Population>()
        .unwrap()
        .families
        .iter()
        .find(|(_, f)| f.mother == mother && f.father == father)
        .unwrap()
        .0;
    (child, due)
}
fn carry(h: &mut Host, p: PersonId) {
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 1000,
        })
        .unwrap();
    for good in [game_cargo::FOOD, game_cargo::TIMBER] {
        h.dispatch(game_cargo::Deposit {
            inventory,
            good,
            quantity: 10,
        })
        .unwrap();
    }
}
fn carries(p: PersonId, good: GoodId) -> Scene {
    [
        ("@predicate".into(), "relation:carries".into()),
        ("actor".into(), format!("person:{}", p.0)),
        ("thing".into(), format!("good:{}", good.0)),
    ]
    .into()
}
#[test]
fn newborns_inherit_capacities_but_three_biological_generations_transmit_constructions_only_through_contact()
 {
    let mut b = builder();
    game_lifecycle::plugin(&mut b).unwrap();
    game_heredity::plugin(&mut b).unwrap();
    // Fixed founding genotypes isolate cultural transmission from random impairment.
    // This is a biological fixture with real gestation, not an unstructured history.
    let (mut h, p, area) = setup_with(b, true);
    for (person, role) in p.iter().copied().zip([0, 1000, 1000, 2000]) {
        let mut profile = h.get::<game_traits::Traits>().unwrap().people[&person].clone();
        profile.insert("reproductive_role".into(), role);
        h.dispatch(game_traits::Set { person, profile }).unwrap();
        h.dispatch(game_heredity::Founder { person }).unwrap();
    }
    let first_due = h.now().after(SimDuration(270 * DAY.0)).unwrap();
    for (mother, father) in [(p[0], p[1]), (p[2], p[3])] {
        h.dispatch(game_lifecycle::Conceive {
            mother,
            father,
            attendant: mother,
        })
        .unwrap();
    }
    run(&mut h, first_due);
    let population = h.get::<game_lifecycle::Population>().unwrap();
    let children: Vec<_> = [p[0], p[2]]
        .iter()
        .map(|mother| {
            *population
                .families
                .iter()
                .find(|(_, f)| f.mother == *mother)
                .unwrap()
                .0
        })
        .collect();
    for child in &children {
        assert_eq!(
            h.get::<game_heredity::Heredity>().unwrap().generation[child],
            1
        );
        assert!(!h.get::<Languages>().unwrap().people.contains_key(child));
        assert!(!h.get::<Languages>().unwrap().observed.contains_key(child));
    }
    run(
        &mut h,
        first_due.after(SimDuration(20 * 365 * DAY.0)).unwrap(),
    );
    assert!(
        !h.get::<Languages>()
            .unwrap()
            .people
            .contains_key(&children[0])
    );
    carry(&mut h, p[0]);
    for n in 0..40 {
        h.dispatch(Describe {
            speaker: p[0],
            listener: children[0],
            scene: scene(p[n % 2], area),
            demonstrate: true,
        })
        .unwrap();
        let frame = carries(
            p[0],
            if n % 2 == 0 {
                game_cargo::FOOD
            } else {
                game_cargo::TIMBER
            },
        );
        h.dispatch(Describe {
            speaker: p[0],
            listener: children[0],
            scene: frame,
            demonstrate: true,
        })
        .unwrap();
    }
    assert!(h.get::<Languages>().unwrap().people[&children[0]].can_embed(4000));
    let (grandchild, born) = birth_pair(&mut h, children[0], children[1]);
    assert_eq!(
        h.get::<game_heredity::Heredity>().unwrap().generation[&grandchild],
        2
    );
    assert!(
        !h.get::<Languages>()
            .unwrap()
            .people
            .contains_key(&grandchild)
    );
    run(&mut h, born.after(SimDuration(20 * 365 * DAY.0)).unwrap());
    carry(&mut h, children[0]);
    let mut loaded = bio_builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        for n in 0..12 {
            let mut frame = carries(
                children[0],
                if n % 2 == 0 {
                    game_cargo::FOOD
                } else {
                    game_cargo::TIMBER
                },
            );
            frame.insert("actor.@predicate".into(), "relation:at".into());
            frame.insert("actor.place".into(), format!("place:{}", area.0));
            host.dispatch(Describe {
                speaker: children[0],
                listener: grandchild,
                scene: frame,
                demonstrate: true,
            })
            .unwrap();
        }
        assert!(host.get::<Languages>().unwrap().people[&grandchild].productive_rules() > 0);
        assert!(
            host.dispatch(Read { reader: grandchild })
                .unwrap()
                .iter()
                .any(|r| r
                    .interpretation
                    .as_ref()
                    .is_some_and(|s| s.contains_key("actor.@predicate")))
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
fn bio_builder() -> Builder {
    let mut b = builder();
    game_lifecycle::plugin(&mut b).unwrap();
    game_heredity::plugin(&mut b).unwrap();
    b
}

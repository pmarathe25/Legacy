mod support;
use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = support::builder();
    game_traits::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_households::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    b
}
#[test]
fn wounded_neighbors_remember_all_their_appeals_and_do_not_request_aid_forever_at_one_instant() {
    let mut h = builder().build(3, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Empty clinic".into(),
        })
        .unwrap();
    for i in 0..4 {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        h.dispatch(game_social_life::Register {
            person,
            name: format!("Neighbor {i}"),
        })
        .unwrap();
        h.dispatch(game_physiology::Damage { person, amount: 60 })
            .unwrap();
        h.dispatch(game_characters::Register {
            person,
            mind: Default::default(),
        })
        .unwrap();
    }
    h.dispatch(game_care::Enable).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    assert_eq!(
        h.advance(SimTime(10 * MINUTE.0), 1000).unwrap().stop,
        Stop::Target
    );
    let care = h.get::<game_care::Care>().unwrap();
    assert_eq!(care.sought.values().map(|r| r.len()).sum::<usize>(), 12);
    assert!(care.ongoing.is_empty());
    let saved = h.save().unwrap();
    let mut loaded = builder().load(&saved, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        assert_eq!(
            host.advance(SimTime(HOUR.0), 2000).unwrap().stop,
            Stop::Target
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn a_received_appeal_does_not_make_a_person_care_against_their_own_morality() {
    let mut h = builder().build(3, 1).unwrap();
    let objects = game_objects::standard().unwrap();
    let medicine = objects.resolve("medicine").unwrap();
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Clinic".into(),
        })
        .unwrap();
    let mut people = vec![];
    for name in ["Doctor", "Patient"] {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        h.dispatch(game_social_life::Register {
            person,
            name: name.into(),
        })
        .unwrap();
        people.push(person);
    }
    let doctor = people[0];
    let patient = people[1];
    h.dispatch(game_physiology::Damage {
        person: patient,
        amount: 60,
    })
    .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(doctor),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: medicine,
        quantity: 3,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: doctor,
        profile: [("empathy".into(), 1000), ("altruism".into(), 0)].into(),
    })
    .unwrap();
    h.dispatch(game_health::Configure(game_health::TreatmentRules {
        duration: MINUTE,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_care::Enable).unwrap();
    assert!(
        h.dispatch(game_care::RequestAid {
            patient,
            caregiver: doctor
        })
        .unwrap()
    );
    h.dispatch(game_characters::Register {
        person: doctor,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    assert_eq!(
        h.advance(SimTime(MINUTE.0), 1000).unwrap().stop,
        Stop::Target
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(medicine),
        3
    );
    assert!(h.get::<game_care::Care>().unwrap().ongoing.is_empty());
    h.dispatch(game_traits::Set {
        person: doctor,
        profile: [("empathy".into(), 1000), ("altruism".into(), 1000)].into(),
    })
    .unwrap();
    assert_eq!(
        h.advance(SimTime(2 * MINUTE.0), 1000).unwrap().stop,
        Stop::Target
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(medicine),
        2
    );
    assert_eq!(
        h.advance(SimTime(3 * MINUTE.0), 1000).unwrap().stop,
        Stop::Target
    );
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&patient].health > 40);
}

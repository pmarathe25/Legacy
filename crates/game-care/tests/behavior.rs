mod support;
use game_care::*;
use game_core::*;
use support::*;
fn patient(h: &mut game_sim::Host, p: PersonId, area: AreaId) -> PersonId {
    let patient = person(h, area);
    for who in [p, patient] {
        h.dispatch(game_social_life::Register {
            person: who,
            name: format!("Person {}", who.0),
        })
        .unwrap();
    }
    h.dispatch(game_physiology::Damage {
        person: patient,
        amount: 30,
    })
    .unwrap();
    h.dispatch(game_health::Configure(game_health::TreatmentRules {
        duration: MINUTE,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("empathy".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(Enable).unwrap();
    patient
}

#[test]
fn inherited_altruism_varies_sacrifice_and_learned_duty_can_motivate_a_selfish_carer() {
    for (altruism, duty, willing) in [(0, false, false), (1000, false, true), (0, true, true)] {
        let (mut h, caregiver, pack, area) = setup();
        patient(&mut h, caregiver, area);
        deposit(&mut h, pack, "medicine", 3);
        h.dispatch(game_traits::Set {
            person: caregiver,
            profile: [("empathy".into(), 1000), ("altruism".into(), altruism)].into(),
        })
        .unwrap();
        if duty {
            for _ in 0..20 {
                h.dispatch(game_relationships::LearnValue {
                    person: caregiver,
                    concept: "care".into(),
                    outcome: 5,
                })
                .unwrap();
            }
        }
        assert_eq!(h.dispatch(Consider { person: caregiver }).unwrap(), willing);
        assert_eq!(amount(&h, pack, "medicine"), if willing { 2 } else { 3 });
    }
}
#[test]
fn local_care_consumes_medicine_and_changes_the_patients_own_impression() {
    let (mut h, p, i, area) = setup();
    let patient = patient(&mut h, p, area);
    deposit(&mut h, i, "medicine", 3);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "medicine"), 2);
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&patient].health,
        70
    );
    let save = h.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, MINUTE.0);
        assert_eq!(
            host.get::<game_physiology::Bodies>().unwrap().people[&patient].health,
            80
        );
        assert_eq!(
            host.get::<game_relationships::Social>()
                .unwrap()
                .relationships[&patient][&p]
                .affection,
            3
        );
        assert!(host.get::<Care>().unwrap().assigned.is_empty());
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn cancellation_frees_the_patient_without_healing_or_gratitude() {
    let (mut h, p, i, area) = setup();
    let patient = patient(&mut h, p, area);
    deposit(&mut h, i, "medicine", 3);
    assert!(h.dispatch(Consider { person: p }).unwrap());
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, MINUTE.0);
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&patient].health,
        70
    );
    assert_eq!(amount(&h, i, "medicine"), 2);
    assert!(h.get::<Care>().unwrap().assigned.is_empty());
    assert!(h.get::<Care>().unwrap().treated.is_empty());
}
#[test]
fn concern_uses_the_caregivers_own_disposition_and_grievance() {
    let (mut h, p, i, area) = setup();
    let patient = patient(&mut h, p, area);
    deposit(&mut h, i, "medicine", 3);
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("empathy".into(), 0)].into(),
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    h.dispatch(game_traits::Set {
        person: p,
        profile: [("empathy".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_relationships::SetRelationship {
        observer: p,
        other: patient,
        relationship: game_relationships::Relationship {
            grievance: 100,
            ..Default::default()
        },
    })
    .unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "medicine"), 3);
}
#[test]
fn distant_people_and_player_control_do_not_trigger_care_actions() {
    let (mut h, p, i, area) = setup();
    let patient = patient(&mut h, p, area);
    deposit(&mut h, i, "medicine", 3);
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Away".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: patient,
        destination: away,
    })
    .unwrap();
    run(&mut h, MINUTE.0);
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    h.dispatch(game_lineage::Begin { person: p }).unwrap();
    assert!(!h.dispatch(Consider { person: p }).unwrap());
    assert_eq!(amount(&h, i, "medicine"), 3);
}

#[test]
fn a_second_automatic_caregiver_does_not_duplicate_an_assignment() {
    let (mut h, p, i, area) = setup();
    let patient = patient(&mut h, p, area);
    deposit(&mut h, i, "medicine", 4);
    let second = person(&mut h, area);
    h.dispatch(game_traits::Set {
        person: second,
        profile: [("empathy".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_property::Permit {
        issuer: p,
        person: second,
        inventory: i,
        allow: true,
    })
    .unwrap();
    assert!(h.dispatch(Consider { person: p }).unwrap());
    assert!(!h.dispatch(Consider { person: second }).unwrap());
    assert_eq!(h.get::<Care>().unwrap().assigned[&patient], p);
    assert_eq!(amount(&h, i, "medicine"), 3);
}

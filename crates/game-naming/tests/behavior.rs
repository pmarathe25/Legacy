use game_core::*;
use game_names::*;
use game_sim::*;
const YEAR: u64 = 365 * DAY.0;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_lifecycle::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_names::plugin(&mut b).unwrap();
    game_naming::plugin(&mut b).unwrap();
    game_wellbeing::plugin(&mut b).unwrap();
    b
}
fn setup() -> (Host, AreaId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(Enable).unwrap();
    h.advance(SimTime(30 * YEAR), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Glade".into(),
        })
        .unwrap();
    (h, area)
}
fn person(h: &mut Host, area: AreaId, name: &str, traits: &[(&str, i32)]) -> PersonId {
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_traits::Set {
        person: p,
        profile: traits.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    })
    .unwrap();
    h.dispatch(game_physiology::Express { person: p }).unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    h.dispatch(game_social_life::Register {
        person: p,
        name: name.into(),
    })
    .unwrap();
    p
}
fn fond(h: &mut Host, observer: PersonId, other: PersonId, affection: i32) {
    h.dispatch(game_relationships::SetRelationship {
        observer,
        other,
        relationship: game_relationships::Relationship {
            affection,
            trust: 50,
            ..Default::default()
        },
    })
    .unwrap();
}
fn meet(h: &mut Host, first: PersonId, second: PersonId) {
    let until = h
        .dispatch(game_social_life::Talk {
            person: first,
            other: second,
        })
        .unwrap();
    while h.advance(until, 1000).unwrap().stop != Stop::Target {}
}
#[test]
fn parent_honors_a_loved_person_heard_locally_instead_of_using_an_identity_number() {
    let (mut h, area) = setup();
    let mother = person(
        &mut h,
        area,
        "Mariel",
        &[("empathy", 900), ("creativity", 0)],
    );
    let father = person(&mut h, area, "Toren", &[]);
    fond(&mut h, mother, father, 80);
    meet(&mut h, mother, father);
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            caregiver: mother,
            area,
        })
        .unwrap();
    h.dispatch(NameChild { child }).unwrap();
    let identity = &h.get::<Names>().unwrap().identities[&child];
    assert_eq!(identity.given, "Toren");
    assert_eq!(identity.chosen_by, mother);
    assert_eq!(
        h.get::<game_social_life::SocialLife>().unwrap().people[&child].name,
        "Toren"
    );
    assert_eq!(h.get::<Names>().unwrap().heard[&mother][&child], "Toren");
    let saved = h.save().unwrap();
    assert!(h.dispatch(NameChild { child }).is_err());
    assert_eq!(h.save().unwrap(), saved);
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
#[test]
fn personal_aliases_and_remote_memories_survive_a_chosen_name_change() {
    let (mut h, area) = setup();
    let subject = person(&mut h, area, "Toren", &[]);
    let friend = person(&mut h, area, "Mariel", &[]);
    let distant = person(&mut h, area, "Mave", &[]);
    for speaker in [friend, distant] {
        h.dispatch(LearnName {
            listener: speaker,
            person: subject,
        })
        .unwrap();
    }
    h.dispatch(Label {
        speaker: friend,
        person: subject,
        name: "Oak".into(),
        kind: Kind::Nickname,
    })
    .unwrap();
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Hill".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: HOUR,
    })
    .unwrap();
    let until = h
        .dispatch(game_transport::Move {
            person: distant,
            destination: away,
        })
        .unwrap();
    h.advance(until, 1000).unwrap();
    h.dispatch(Label {
        speaker: distant,
        person: subject,
        name: "Cinder".into(),
        kind: Kind::Epithet,
    })
    .unwrap();
    assert!(!h.get::<Names>().unwrap().offers[&subject].contains_key(&distant));
    h.dispatch(Change {
        person: subject,
        name: "River".into(),
    })
    .unwrap();
    let names = h.get::<Names>().unwrap();
    assert_eq!(names.heard[&friend][&subject], "River");
    assert_eq!(names.heard[&distant][&subject], "Toren");
    assert_eq!(names.identities[&subject].given, "Toren");
    assert_eq!(names.identities[&subject].changes.len(), 2);
    for (observer, name) in [(friend, "Oak"), (distant, "Cinder")] {
        assert_eq!(
            h.query(game_social_life::NameFor {
                observer,
                person: subject,
                fallback: "?".into()
            })
            .unwrap(),
            name
        );
    }
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Change {
            person: subject,
            name: "bad\nname".into()
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    assert_eq!(builder().load(&saved, 2).unwrap().save().unwrap(), saved);
}
#[test]
fn a_person_can_adopt_a_nickname_they_heard_from_someone_they_like() {
    let (mut h, area) = setup();
    let subject = person(
        &mut h,
        area,
        "Toren",
        &[("creativity", 1000), ("ambition", 1000), ("patience", 0)],
    );
    let friend = person(&mut h, area, "Mariel", &[]);
    fond(&mut h, subject, friend, 100);
    h.dispatch(LearnName {
        listener: friend,
        person: subject,
    })
    .unwrap();
    h.dispatch(Label {
        speaker: friend,
        person: subject,
        name: "Ash".into(),
        kind: Kind::Nickname,
    })
    .unwrap();
    assert!(
        h.dispatch(game_naming::Reconsider { person: subject })
            .unwrap()
    );
    assert_eq!(
        h.get::<game_social_life::SocialLife>().unwrap().people[&subject].name,
        "Ash"
    );
    assert_eq!(
        h.get::<Names>().unwrap().identities[&subject].given,
        "Toren"
    );
    assert!(
        !h.dispatch(game_naming::Reconsider { person: subject })
            .unwrap()
    );
}
#[test]
fn successful_treatment_earns_a_personal_epithet_without_a_global_reputation_assignment() {
    let (mut h, area) = setup();
    let patient = person(&mut h, area, "Mariel", &[]);
    let healer = person(&mut h, area, "Toren", &[]);
    let bystander = person(&mut h, area, "Mave", &[]);
    let objects = game_objects::load(&mut game_content::Store::standard()).unwrap();
    let medicine = objects.resolve("medicine").unwrap();
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_health::Configure(game_health::TreatmentRules {
        supply_object: medicine,
        ..Default::default()
    }))
    .unwrap();
    let supplies = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(healer),
            capacity: 10000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: supplies,
        good: medicine,
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_physiology::Damage {
        person: patient,
        amount: 10,
    })
    .unwrap();
    let until = h
        .dispatch(game_health::Treat {
            caregiver: healer,
            patient,
            supplies,
        })
        .unwrap();
    h.advance(until, 1000).unwrap();
    let names = h.get::<Names>().unwrap();
    assert_eq!(names.usage[&patient][&healer][0].name, "Toren the Kind");
    assert!(!names.usage.contains_key(&bystander));
    assert_eq!(
        h.get::<game_social_life::SocialLife>().unwrap().people[&healer].name,
        "Toren"
    );
}
#[test]
fn a_member_can_use_an_actual_office_title_without_making_others_recognize_it() {
    let (mut h, area) = setup();
    let holder = person(&mut h, area, "Toren", &[]);
    let member = person(&mut h, area, "Mariel", &[("creativity", 0)]);
    let outsider = person(&mut h, area, "Mave", &[("creativity", 0)]);
    fond(&mut h, member, holder, 60);
    h.dispatch(game_institutions::Found(game_institutions::Institution {
        name: "Seed Circle".into(),
        jurisdiction: area,
        members: [member, holder].into(),
        procedure: game_institutions::Procedure::Personal,
        offices: [("Seedkeeper".into(), holder)].into(),
        rules: vec![],
    }))
    .unwrap();
    meet(&mut h, member, holder);
    meet(&mut h, outsider, holder);
    assert!(
        h.get::<Names>().unwrap().usage[&member][&holder]
            .iter()
            .any(|a| a.kind == Kind::Title && a.name == "Seedkeeper Toren")
    );
    assert!(
        !h.get::<Names>()
            .unwrap()
            .usage
            .get(&outsider)
            .is_some_and(|row| row
                .get(&holder)
                .is_some_and(|a| a.iter().any(|a| a.kind == Kind::Title)))
    );
    assert_eq!(
        h.get::<game_social_life::SocialLife>().unwrap().people[&holder].name,
        "Toren"
    );
}
#[test]
fn childhood_attitudes_develop_through_trusted_conversation_rather_than_birth() {
    let (mut h, area) = setup();
    let mother = person(&mut h, area, "Mariel", &[]);
    let father = person(&mut h, area, "Toren", &[]);
    h.dispatch(game_relationships::DefineBeliefs {
        person: mother,
        beliefs: game_relationships::Beliefs {
            sharing: 10,
            authority: -10,
            honesty: 10,
        },
    })
    .unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            caregiver: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Express { person: child })
        .unwrap();
    h.dispatch(NameChild { child }).unwrap();
    h.dispatch(game_relationships::Initialize {
        person: child,
        beliefs: Default::default(),
        values: Default::default(),
    })
    .unwrap();
    assert_eq!(
        h.get::<game_relationships::Social>().unwrap().beliefs[&child],
        game_relationships::Beliefs::default()
    );
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_social_life::Talk {
            person: child,
            other: mother
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    h.advance(SimTime(h.now().0 + 8 * YEAR), 1000).unwrap();
    fond(&mut h, child, mother, 60);
    meet(&mut h, child, mother);
    let beliefs = &h.get::<game_relationships::Social>().unwrap().beliefs[&child];
    assert_eq!(beliefs.sharing, 1);
    assert_eq!(beliefs.authority, -1);
    assert_eq!(beliefs.honesty, 1);
    assert!(
        h.query(game_vitality::Assess { person: child })
            .unwrap()
            .unwrap()
            .current_traits["foresight"]
            < 1000
    );
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}

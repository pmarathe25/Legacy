mod support;
use game_core::*;
use game_pastimes::*;
use support::*;
fn install(h: &mut game_sim::Host) {
    h.dispatch(load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap())
        .unwrap();
}
fn install_selected(h: &mut game_sim::Host, keys: &[&str]) {
    let mut definitions = load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap();
    definitions
        .activities
        .retain(|key, _| keys.contains(&key.as_str()));
    h.dispatch(definitions).unwrap();
}
#[test]
fn completed_activity_spends_supplies_and_builds_only_the_participants_interest() {
    let (mut h, p, i, area) = setup();
    let other = person(&mut h, area);
    install(&mut h);
    deposit(&mut h, i, "timber", 2);
    h.dispatch(Pursue {
        person: p,
        activity: "whittle".into(),
        inventory: i,
    })
    .unwrap();
    assert_eq!(amount(&h, i, "timber"), 1);
    assert!(!h.get::<Pastimes>().unwrap().experiences.contains_key(&p));
    let save = h.save().unwrap();
    let mut loaded = builder().load(&save, 4).unwrap();
    for host in [&mut h, &mut loaded] {
        run(host, 15 * MINUTE.0);
        assert_eq!(
            host.get::<Pastimes>().unwrap().experiences[&p]["whittle"],
            1
        );
        assert_eq!(
            host.get::<game_relationships::Social>().unwrap().values[&p]["carving"],
            1
        );
        assert!(
            !host
                .get::<game_relationships::Social>()
                .unwrap()
                .values
                .contains_key(&other)
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn canceled_or_lost_instruments_do_not_credit_an_experience() {
    let (mut h, p, i, _) = setup();
    install(&mut h);
    deposit(&mut h, i, "reed-pipe", 1);
    h.dispatch(Pursue {
        person: p,
        activity: "play-reed-pipe".into(),
        inventory: i,
    })
    .unwrap();
    let good = good(&h, "reed-pipe");
    h.dispatch(game_cargo::Consume {
        inventory: i,
        good,
        quantity: 1,
    })
    .unwrap();
    run(&mut h, 10 * MINUTE.0);
    assert!(h.get::<Pastimes>().unwrap().experiences.is_empty());
    deposit(&mut h, i, "timber", 1);
    h.dispatch(Pursue {
        person: p,
        activity: "whittle".into(),
        inventory: i,
    })
    .unwrap();
    h.dispatch(Cancel(p)).unwrap();
    run(&mut h, HOUR.0);
    assert!(h.get::<Pastimes>().unwrap().experiences.is_empty());
    assert!(h.get::<game_skills::Skills>().unwrap().levels.is_empty());
}
#[test]
fn contested_use_is_possible_while_one_physical_action_is_required() {
    let (mut h, p, i, area) = setup();
    let other = person(&mut h, area);
    install(&mut h);
    deposit(&mut h, i, "timber", 3);
    h.dispatch(Pursue {
        person: other,
        activity: "whittle".into(),
        inventory: i,
    })
    .unwrap();
    assert!(h.notices().any(|n| n.kind == "property.interfered"));
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(other, i)
    );
    h.dispatch(Cancel(other)).unwrap();
    h.dispatch(Pursue {
        person: p,
        activity: "whittle".into(),
        inventory: i,
    })
    .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Pursue {
            person: p,
            activity: "whittle".into(),
            inventory: i
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
}
#[test]
fn autonomous_leisure_considers_satisfaction_and_never_reserves_the_player() {
    let (mut h, p, i, _) = setup();
    install_selected(&mut h, &["whittle"]);
    deposit(&mut h, i, "timber", 3);
    h.dispatch(Enable).unwrap();
    h.dispatch(Idle(p)).unwrap();
    assert_eq!(h.get::<game_sim::Actions>().unwrap().0[&p].kind, "pastime");
    run(&mut h, 15 * MINUTE.0);
    h.dispatch(Idle(p)).unwrap();
    assert!(!h.get::<game_sim::Actions>().unwrap().0.contains_key(&p));
    run(&mut h, 4 * HOUR.0);
    h.dispatch(game_lineage::Begin { person: p }).unwrap();
    h.dispatch(Idle(p)).unwrap();
    assert!(!h.get::<game_sim::Actions>().unwrap().0.contains_key(&p));
}

#[test]
fn different_dispositions_select_different_activities_from_the_same_possessions() {
    let (mut h, creative, i, area) = setup();
    install_selected(&mut h, &["play-reed-pipe", "burn-incense"]);
    let reflective = person(&mut h, area);
    let j = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(reflective),
            capacity: 100,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: reflective,
        inventory: j,
        recognized_by: [reflective].into(),
    })
    .unwrap();
    for pack in [i, j] {
        deposit(&mut h, pack, "reed-pipe", 1);
        deposit(&mut h, pack, "incense", 1);
    }
    for (person, c, r) in [(creative, 1000, 0), (reflective, 0, 1000)] {
        h.dispatch(game_traits::Set {
            person,
            profile: [("creativity".into(), c), ("contemplation".into(), r)].into(),
        })
        .unwrap();
    }
    h.dispatch(Enable).unwrap();
    h.dispatch(Idle(creative)).unwrap();
    h.dispatch(Idle(reflective)).unwrap();
    run(&mut h, 15 * MINUTE.0);
    assert_eq!(
        h.get::<Pastimes>().unwrap().experiences[&creative]["play-reed-pipe"],
        1
    );
    assert_eq!(
        h.get::<Pastimes>().unwrap().experiences[&reflective]["burn-incense"],
        1
    );
    assert_eq!(amount(&h, i, "incense"), 1);
    assert_eq!(amount(&h, j, "incense"), 0);
}
#[test]
fn death_cancels_the_activity_and_removes_its_retry_deadline() {
    let (mut h, p, i, _) = setup();
    install(&mut h);
    deposit(&mut h, i, "timber", 1);
    h.dispatch(Pursue {
        person: p,
        activity: "whittle".into(),
        inventory: i,
    })
    .unwrap();
    h.dispatch(game_physiology::Damage {
        person: p,
        amount: 100,
    })
    .unwrap();
    run(&mut h, HOUR.0);
    assert!(h.get::<Pastimes>().unwrap().experiences.is_empty());
    assert!(h.get::<Pastimes>().unwrap().satisfaction.is_empty());
    builder().load(&h.save().unwrap(), 4).unwrap();
}

#[test]
fn a_person_can_repeat_a_pastime_immediately_despite_recent_satisfaction() {
    let (mut h, p, i, _) = setup();
    install(&mut h);
    deposit(&mut h, i, "timber", 3);
    let until = h
        .dispatch(Pursue {
            person: p,
            activity: "whittle".into(),
            inventory: i,
        })
        .unwrap();
    run(&mut h, until.0);
    let before = h.get::<Pastimes>().unwrap().experiences[&p]["whittle"];
    let again = h
        .dispatch(Pursue {
            person: p,
            activity: "whittle".into(),
            inventory: i,
        })
        .unwrap();
    run(&mut h, again.0);
    assert_eq!(
        h.get::<Pastimes>().unwrap().experiences[&p]["whittle"],
        before + 1
    );
}
#[test]
fn lethal_consumption_cannot_start_a_posthumous_pastime() {
    let mut b = builder();
    game_nutrition::plugin(&mut b).unwrap();
    let mut h = b.build(17, 1).unwrap();
    let mut objects = game_objects::standard().unwrap();
    objects
        .items
        .get_mut("grain")
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .effects
        .health = -200;
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_nutrition::Enable).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let p = person(&mut h, area);
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(p),
            capacity: 1000,
        })
        .unwrap();
    deposit(&mut h, inventory, "grain", 20);
    let activity = game_pastimes::Activity {
        abilities: Default::default(),
        name: "Try a dangerous meal".into(),
        inputs: [(game_cargo::FOOD, 10)].into(),
        requires: Default::default(),
        duration: MINUTE,
        satisfaction_span: HOUR,
        affinity: "patience".into(),
        concept: "cooking".into(),
        skill: "cooking".into(),
    };
    h.dispatch(game_pastimes::Install {
        activities: [("dangerous".into(), activity)].into(),
        stamps: vec![],
    })
    .unwrap();
    h.dispatch(game_pastimes::Pursue {
        person: p,
        inventory,
        activity: "dangerous".into(),
    })
    .unwrap();
    assert!(!h.get::<game_physiology::Bodies>().unwrap().people[&p].alive);
    assert!(!h.get::<game_sim::Actions>().unwrap().0.contains_key(&p));
    run(&mut h, HOUR.0);
    assert!(!h.get::<Pastimes>().unwrap().experiences.contains_key(&p));
}

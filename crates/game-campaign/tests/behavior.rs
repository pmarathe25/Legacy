mod support;
use game_core::*;
use game_sim::*;
use std::collections::{BTreeMap, BTreeSet};
use support::*;
#[test]
fn inherited_characteristics_are_expressed_without_inheriting_skills_or_knowledge() {
    // Isolate high expressed inheritance with homozygous intelligence.
    // Diverse equal-phenotype parents can legitimately have lower-phenotype children.
    let (mut h, area) = world_with_spread(0);
    // At fifty years since epoch, these founders were born thirty years ago.
    let mut parents = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(20 * 365 * DAY.0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_social_life::Register {
            person: p,
            name: format!("Parent {}", p.0),
        })
        .unwrap();
        h.dispatch(game_traits::Set {
            person: p,
            profile: BTreeMap::from([
                ("empathy".into(), 900),
                ("patience".into(), 900),
                ("intelligence".into(), 12000),
                ("memory_capacity".into(), 4000),
                ("memory_reliability".into(), 4000),
                ("memory_durability".into(), 4000),
            ]),
        })
        .unwrap();
        h.dispatch(game_heredity::Founder { person: p }).unwrap();
        parents.push(p);
    }
    for (observer, other) in [(parents[0], parents[1]), (parents[1], parents[0])] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                affection: 50,
                trust: 20,
                ..Default::default()
            },
        })
        .unwrap();
    }
    h.dispatch(game_households::Enable).unwrap();
    assert!(
        h.dispatch(game_households::Form {
            first: parents[0],
            second: parents[1]
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .len(),
        1
    );
    let at = h.now().after(SimDuration(270 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let population = h.get::<game_lifecycle::Population>().unwrap();
    let child = *population.families.keys().next().unwrap();
    for key in [
        "rigidity",
        "reproductive-drive",
        "memory_capacity",
        "memory_reliability",
        "memory_durability",
    ] {
        assert!(h.get::<game_heredity::Heredity>().unwrap().loci[&child].contains_key(key));
        assert!(h.get::<game_traits::Traits>().unwrap().people[&child].contains_key(key));
    }
    for key in ["memory_capacity", "memory_reliability", "memory_durability"] {
        assert!(h.get::<game_traits::Traits>().unwrap().ability(child, key) > 3500);
    }
    assert_eq!(
        h.get::<game_heredity::Heredity>().unwrap().generation[&child],
        1
    );
    assert!(
        h.get::<game_traits::Traits>()
            .unwrap()
            .ability(child, "intelligence")
            > 11000
    );
    assert!(
        !h.get::<game_skills::Skills>()
            .unwrap()
            .levels
            .contains_key(&child)
    );
    assert!(
        h.get::<game_characters::Minds>()
            .unwrap()
            .people
            .contains_key(&child)
    );
    assert!(
        h.get::<game_cargo::Cargo>()
            .unwrap()
            .carried(child)
            .next()
            .is_some()
    );
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}
#[test]
fn transported_food_keeps_its_age_and_failed_transfers_rollback_batches() {
    let (mut h, area) = world();
    let (a, first) = person(&mut h, area);
    let (_, second) = person(&mut h, area);
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_preservation::Enable).unwrap();
    let expiry = h.get::<game_preservation::Preservation>().unwrap().batches[&first]
        [&game_cargo::FOOD][0]
        .expires;
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_cargo::Transfer {
            from: first,
            to: second,
            good: game_cargo::FOOD,
            quantity: 200
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(game_cargo::Transfer {
        from: first,
        to: second,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_preservation::Preservation>().unwrap().batches[&second][&game_cargo::FOOD][0]
            .expires,
        expiry
    );
    while h.advance(expiry, 128).unwrap().stop == Stop::Budget {}
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&second].amount(game_cargo::FOOD),
        0
    );
    assert_eq!(
        h.get::<game_preservation::Preservation>().unwrap().spoiled,
        20
    );
    let _ = a;
    h.save().unwrap();
}
#[test]
fn exchange_requires_both_peoples_acceptance_and_moves_existing_goods_atomically() {
    let (mut h, area) = world();
    let (a, first) = person(&mut h, area);
    let (b, second) = person(&mut h, area);
    h.dispatch(game_cargo::Deposit {
        inventory: first,
        good: game_cargo::TIMBER,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: second,
        good: game_cargo::MEDICINE,
        quantity: 20,
    })
    .unwrap();
    let agreement = h
        .dispatch(game_exchange::Barter {
            first: a,
            second: b,
            offered: game_cargo::TIMBER,
            requested: game_cargo::MEDICINE,
            quantity: 1,
        })
        .unwrap();
    assert_eq!(
        h.get::<game_agreements::Agreements>().unwrap().0[&agreement].status,
        game_agreements::Status::Complete
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&first].amount(game_cargo::MEDICINE),
        1
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&second].amount(game_cargo::TIMBER),
        1
    );
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_exchange::Barter {
            first: a,
            second: b,
            offered: game_cargo::TIMBER,
            requested: game_cargo::MEDICINE,
            quantity: 9999
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
}
#[test]
fn admission_obeys_actual_procedure_and_a_departed_leader_leaves_no_office() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    let (c, _) = person(&mut h, area);
    let id = h
        .dispatch(game_institutions::Found(game_institutions::Institution {
            name: "A compact".into(),
            jurisdiction: area,
            members: BTreeSet::from([a, b]),
            procedure: game_institutions::Procedure::Leader(a),
            offices: BTreeMap::from([("chief".into(), a)]),
            rules: vec![],
        }))
        .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(game_institutions::Join {
            institution: id,
            person: c,
            responses: BTreeSet::from([b, c])
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    h.dispatch(game_institutions::Join {
        institution: id,
        person: c,
        responses: BTreeSet::from([a, c]),
    })
    .unwrap();
    h.dispatch(game_institutions::Leave {
        institution: id,
        person: a,
    })
    .unwrap();
    let group = &h.get::<game_institutions::Institutions>().unwrap().groups[&id];
    assert_eq!(group.members.len(), 2);
    assert!(group.offices.is_empty());
    assert_eq!(group.procedure, game_institutions::Procedure::Personal);
    h.save().unwrap();
}

#[test]
fn dependent_care_consumes_food_and_travels_with_the_caregiver() {
    let (mut h, area) = world();
    let (mother, inventory) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    // This fixture chooses a caregiver willing to bear the food cost.
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&mother].clone();
    profile.insert("altruism".into(), 1000);
    h.dispatch(game_traits::Set {
        person: mother,
        profile,
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let target = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(target, 128).unwrap().stop == Stop::Budget {}
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger >= 30);
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger,
        0
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        94
    );
    let other = h
        .dispatch(game_land::CreateArea {
            name: "Other clearing".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: other,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: mother,
        destination: other,
    })
    .unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    assert_eq!(
        game_transport::area(h.get().unwrap(), child).unwrap(),
        other
    );
    assert_eq!(
        game_transport::area(h.get().unwrap(), mother).unwrap(),
        other
    );
    h.save().unwrap();
}

#[test]
fn local_children_can_be_fed_without_mandating_a_reunion_journey() {
    let (mut h, area) = world();
    // Isolate care ordering from physiological harm during the six-year setup.
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        deprivation: u64::MAX,
        ..Default::default()
    }))
    .unwrap();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    let older = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    let at = h.now().after(SimDuration(6 * 365 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    // A newly arrived foster parent has no accumulated needs from the clock-only setup.
    h.dispatch(game_physiology::Damage {
        person: mother,
        amount: 1000,
    })
    .unwrap();
    let (mother, inventory) = person(&mut h, area);
    // This fixture chooses a caregiver willing to bear the food cost.
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&mother].clone();
    profile.insert("altruism".into(), 1000);
    h.dispatch(game_traits::Set {
        person: mother,
        profile,
    })
    .unwrap();
    h.dispatch(game_households::Foster {
        caregiver: mother,
        child: older,
    })
    .unwrap();
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
    // Keep the older child's other parent with them. Their delayed six-year
    // physiology is not a competing local patient in this ordering fixture.
    for (person, destination) in [
        (older, away),
        (father, away),
        (mother, away),
        (mother, area),
    ] {
        h.dispatch(game_transport::Move {
            person,
            destination,
        })
        .unwrap();
        let at = h.now().after(MINUTE).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    }
    // Keep this fixture about separated care; accompaniment is tested independently.
    if game_transport::area(h.get().unwrap(), older).unwrap() != away {
        h.dispatch(game_transport::Move {
            person: older,
            destination: away,
        })
        .unwrap();
        let at = h.now().after(MINUTE).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    }
    h.dispatch(game_knowledge::ObserveSurroundings { person: mother })
        .unwrap();
    let younger = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&younger].hunger >= 30);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(game_households::Tend { person: mother })
            .unwrap();
        assert_eq!(
            host.get::<game_physiology::Bodies>().unwrap().people[&younger].hunger,
            0,
            "mother={:?}, patient={:?}, care={:?}, action={:?}, positions={:?}/{:?}",
            host.get::<game_physiology::Bodies>().unwrap().people[&mother],
            host.get::<game_physiology::Bodies>().unwrap().people[&younger],
            host.get::<game_households::Households>()
                .unwrap()
                .food_care
                .get(&mother),
            host.get::<Actions>().unwrap().0.get(&mother),
            game_transport::area(host.get().unwrap(), mother),
            game_transport::area(host.get().unwrap(), younger)
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
            94
        );
        assert!(!host.get::<Actions>().unwrap().0.contains_key(&mother));
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn contested_food_is_physically_usable_and_only_local_witnesses_can_react() {
    let (mut h, area) = world();
    let (owner, inventory) = person(&mut h, area);
    let (actor, _) = person(&mut h, area);
    let (remote, _) = person(&mut h, area);
    let other = h
        .dispatch(game_land::CreateArea {
            name: "Distant".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: other,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: remote,
        destination: other,
    })
    .unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_grievances::Enable).unwrap();
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(actor, inventory)
    );
    h.dispatch(game_physiology::Eat {
        person: actor,
        inventory,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        10
    );
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(actor, inventory)
    );
    let notices: Vec<_> = h
        .notices()
        .filter(|n| n.kind == "property.interfered")
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].witnesses.contains(&owner));
    assert!(!notices[0].witnesses.contains(&remote));
    assert!(
        h.get::<game_grievances::Grievances>().unwrap().forgiven > 0
            || h.get::<game_grievances::Grievances>()
                .unwrap()
                .noticed
                .get(&owner)
                .is_some_and(|r| r.contains_key(&actor))
    );
}

#[test]
fn older_partners_with_independent_children_can_choose_more_children_without_a_personality_gate() {
    let (mut h, area) = world();
    let mut parents = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(20 * 365 * DAY.0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_traits::Set {
            person: p,
            profile: BTreeMap::from([("empathy".into(), 0), ("patience".into(), 0)]),
        })
        .unwrap();
        parents.push(p);
    }
    h.dispatch(game_households::Enable).unwrap();
    for _ in 0..5 {
        h.dispatch(game_lifecycle::Birth {
            mother: parents[0],
            father: parents[1],
            attendant: parents[0],
            area,
        })
        .unwrap();
    }
    let at = h.now().after(SimDuration(18 * 365 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    for (observer, other) in [(parents[0], parents[1]), (parents[1], parents[0])] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    assert!(
        h.dispatch(game_households::Form {
            first: parents[0],
            second: parents[1]
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .len(),
        1
    );
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 2).unwrap().save().unwrap(), save);
}

#[test]
fn current_care_demands_can_outweigh_a_parents_desire_for_another_child() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    for _ in 0..5 {
        h.dispatch(game_lifecycle::Birth {
            mother: a,
            father: b,
            attendant: a,
            area,
        })
        .unwrap();
    }
    for (observer, other) in [(a, b), (b, a)] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    assert!(
        h.dispatch(game_households::Form {
            first: a,
            second: b
        })
        .unwrap()
    );
    assert!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .is_empty()
    );
}

#[test]
fn a_local_adult_can_decline_or_accept_care_for_an_orphan_and_feed_them() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    let (volunteer, pack) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_care::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Damage {
        person: mother,
        amount: 1000,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: volunteer,
        profile: BTreeMap::from([("empathy".into(), 0)]),
    })
    .unwrap();
    h.dispatch(game_care::Consider { person: volunteer })
        .unwrap();
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .carers_of(child)
            .next()
            .is_none()
    );
    h.dispatch(game_traits::Set {
        person: volunteer,
        profile: BTreeMap::from([("empathy".into(), 1000), ("altruism".into(), 1000)]),
    })
    .unwrap();
    h.dispatch(game_care::Consider { person: volunteer })
        .unwrap();
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .committed(volunteer, child)
    );
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_households::Tend { person: volunteer })
        .unwrap();
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger,
        0
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::FOOD),
        94
    );
    let save = h.save().unwrap();
    assert_eq!(builder().load(&save, 2).unwrap().save().unwrap(), save);
}

#[test]
fn an_orphan_enrolls_in_autonomy_even_when_no_adult_has_adopted_them() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Damage {
        person: mother,
        amount: 1000,
    })
    .unwrap();
    let at = h.now().after(SimDuration(6 * 365 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    assert!(
        h.get::<game_characters::Minds>()
            .unwrap()
            .people
            .contains_key(&child)
    );
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .carers_of(child)
            .next()
            .is_none()
    );
    h.save().unwrap();
}

#[test]
fn foster_children_also_count_when_a_person_considers_another_child() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    let (carer, _) = person(&mut h, area);
    let (partner, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    let children: Vec<_> = (0..5)
        .map(|_| {
            h.dispatch(game_lifecycle::Birth {
                mother,
                father,
                attendant: mother,
                area,
            })
            .unwrap()
        })
        .collect();
    h.dispatch(game_physiology::Damage {
        person: mother,
        amount: 1000,
    })
    .unwrap();
    for child in children {
        h.dispatch(game_households::Foster {
            caregiver: carer,
            child,
        })
        .unwrap();
    }
    for (observer, other) in [(carer, partner), (partner, carer)] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    // Isolate care burden from the additional physiological incentive.
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&carer].clone();
    profile.insert("reproductive-drive".into(), 0);
    h.dispatch(game_traits::Set {
        person: carer,
        profile,
    })
    .unwrap();
    assert!(
        h.dispatch(game_households::Form {
            first: carer,
            second: partner
        })
        .unwrap()
    );
    assert!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .is_empty()
    );
    assert_eq!(
        h.get::<game_provisioning::Reserves>().unwrap().dependents[&carer],
        5
    );
    assert_eq!(
        h.get::<game_provisioning::Reserves>()
            .unwrap()
            .dependents
            .get(&mother)
            .copied()
            .unwrap_or(0),
        0
    );
}

#[test]
fn an_existing_partner_can_withdraw_and_later_restore_reproductive_consent() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    for (observer, other) in [(a, b), (b, a)] {
        h.dispatch(game_relationships::SetRelationship {
            observer,
            other,
            relationship: game_relationships::Relationship {
                affection: 100,
                trust: 100,
                ..Default::default()
            },
        })
        .unwrap();
    }
    assert!(
        h.dispatch(game_households::Form {
            first: a,
            second: b
        })
        .unwrap()
    );
    let at = h.now().after(SimDuration(320 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    assert_eq!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .families
            .len(),
        1
    );
    h.dispatch(game_relationships::SetRelationship {
        observer: b,
        other: a,
        relationship: game_relationships::Relationship {
            respect: 0,
            admiration: 0,
            affection: 100,
            trust: 100,
            fear: 100,
            grievance: 20,
        },
    })
    .unwrap();
    assert!(
        !h.dispatch(game_parenthood::Consider { person: a.min(b) })
            .unwrap()
    );
    assert!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .is_empty()
    );
    h.dispatch(game_relationships::SetRelationship {
        observer: b,
        other: a,
        relationship: game_relationships::Relationship {
            affection: 100,
            trust: 100,
            ..Default::default()
        },
    })
    .unwrap();
    assert!(
        h.dispatch(game_parenthood::Consider { person: a.min(b) })
            .unwrap()
    );
}

#[test]
fn a_distant_caregivers_death_does_not_become_a_local_adults_knowledge() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    let (volunteer, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_care::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    let at = h.now().after(SimDuration(6 * 365 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Elsewhere".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    h.dispatch(game_transport::Move {
        person: mother,
        destination: away,
    })
    .unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_physiology::Damage {
        person: mother,
        amount: 1000,
    })
    .unwrap();
    h.dispatch(game_care::Consider { person: volunteer })
        .unwrap();
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .carers_of(child)
            .next()
            .is_none()
    );
}

#[test]
fn routine_reunion_does_not_preempt_a_caregivers_committed_food_plan() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    let at = h.now().after(SimDuration(6 * 365 * DAY.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    let away = h
        .dispatch(game_land::CreateArea {
            name: "Other clearing".into(),
        })
        .unwrap();
    h.dispatch(game_land::Link {
        from: area,
        to: away,
        duration: MINUTE,
    })
    .unwrap();
    // The child chooses to remain rather than accompanying the parent back.
    set_opinion(&mut h, child, "independence", 100);
    for (person, destination) in [(child, away), (mother, away), (mother, area)] {
        h.dispatch(game_transport::Move {
            person,
            destination,
        })
        .unwrap();
        let at = h.now().after(MINUTE).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    }
    assert_eq!(game_transport::area(h.get().unwrap(), child).unwrap(), away);
    let _patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::FOOD,
            stock: 1000,
            capacity: 1000,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(game_provisioning::Enable).unwrap();
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&mother].clone();
    profile.insert("foresight".into(), 16000);
    h.dispatch(game_traits::Set {
        person: mother,
        profile,
    })
    .unwrap();
    h.dispatch(game_characters::Register {
        person: mother,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_characters::Start).unwrap();
    let at = h.now().after(MINUTE).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    assert_eq!(h.get::<Actions>().unwrap().0[&mother].kind, "gather");
    let action = h.get::<Actions>().unwrap().0[&mother].clone();
    let plan = h.get::<game_characters::Minds>().unwrap().people[&mother]
        .plan
        .clone();
    assert!(plan.is_some());
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    assert_eq!(h.get::<Actions>().unwrap().0[&mother], action);
    assert_eq!(
        h.get::<game_characters::Minds>().unwrap().people[&mother]
            .plan
            .as_ref(),
        plan.as_ref()
    );
    assert_eq!(
        game_transport::area(h.get().unwrap(), mother).unwrap(),
        area
    );
    h.save().unwrap();
}

#[test]
fn a_caregiver_accounts_for_harvest_time_when_food_is_already_carried() {
    let (mut h, area) = world();
    let (mother, inventory) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&mother].clone();
    profile.insert("altruism".into(), 1000);
    h.dispatch(game_traits::Set {
        person: mother,
        profile,
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger,
        0
    );
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
        94
    );
    assert_eq!(
        h.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
        100
    );
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&mother));
}

#[test]
fn a_caregiver_can_harvest_less_than_a_portion_and_finish_delivery_after_reload() {
    let (mut h, area) = world();
    let (mother, inventory) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&mother].clone();
    profile.insert("altruism".into(), 1000);
    h.dispatch(game_traits::Set {
        person: mother,
        profile,
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_physiology::Eat {
        person: mother,
        inventory,
    })
    .unwrap();
    let patch = h
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            good: game_cargo::FOOD,
            stock: 4,
            capacity: 4,
            renew: 0,
        }))
        .unwrap();
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    assert_eq!(h.get::<Actions>().unwrap().0[&mother].kind, "gather");
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .food_care
            .contains_key(&mother)
    );
    let finish = h.get::<Actions>().unwrap().0[&mother].until.unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(finish, 128).unwrap().stop == Stop::Budget {}
        assert_eq!(
            host.get::<game_ecology::Ecology>().unwrap().patches[&patch].stock,
            0
        );
        assert_eq!(
            host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD),
            0
        );
        assert!(host.notices().any(|n| n.kind == "physiology.fed"
            && n.data["patient"].as_u64() == Some(child.0)
            && n.data["quantity"] == 4));
        assert_eq!(
            host.get::<game_physiology::Bodies>().unwrap().people[&child].hunger,
            16
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn identical_care_perceptions_choose_the_same_serving_despite_hidden_hunger() {
    let (mut h, area) = world();
    let (mother, inventory) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    h.dispatch(game_traits::Set {
        person: mother,
        profile: [
            ("empathy".into(), 1000),
            ("altruism".into(), 1000),
            ("intelligence".into(), 4000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Express { person: child })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_physiology::Eat {
        person: mother,
        inventory,
    })
    .unwrap();
    let save = h.save().unwrap();
    let mut amounts = vec![];
    for hunger in [35, 39] {
        let mut variant: serde_json::Value = serde_json::from_str(&save).unwrap();
        variant["resources"]["physiology.bodies"]["people"][child.0.to_string()]["hunger"] =
            serde_json::json!(hunger);
        let mut host = builder().load(&variant.to_string(), 4).unwrap();
        host.dispatch(game_households::Tend { person: mother })
            .unwrap();
        amounts
            .push(host.get::<game_cargo::Cargo>().unwrap().0[&inventory].amount(game_cargo::FOOD));
    }
    assert_eq!(amounts[0], amounts[1]);
    assert!(amounts[0] < 90);
}

#[test]
fn another_parent_can_choose_to_feed_a_child_without_replacing_the_primary_caregiver() {
    let (mut h, area) = world();
    let (mother, _) = person(&mut h, area);
    let (father, inventory) = person(&mut h, area);
    for (parent, intelligence, interpretation) in [(mother, 0, 0), (father, 4000, 1000)] {
        h.dispatch(game_traits::Set {
            person: parent,
            profile: [
                ("intelligence".into(), intelligence),
                ("interpretation".into(), interpretation),
                ("altruism".into(), 1000),
                ("empathy".into(), 1000),
            ]
            .into(),
        })
        .unwrap();
    }
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Express { person: child })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    // Settle the parent's own hunger before asking about care.
    h.dispatch(game_physiology::Eat {
        person: father,
        inventory,
    })
    .unwrap();
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    let hungry = h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger;
    assert!(hungry > 0);
    let mut unwilling = builder().load(&h.save().unwrap(), 2).unwrap();
    unwilling
        .dispatch(game_traits::Set {
            person: father,
            profile: [
                ("intelligence".into(), 4000),
                ("interpretation".into(), 1000),
                ("altruism".into(), 0),
                ("empathy".into(), 0),
            ]
            .into(),
        })
        .unwrap();
    unwilling
        .dispatch(game_relationships::SetRelationship {
            observer: father,
            other: child,
            relationship: game_relationships::Relationship {
                grievance: 100,
                ..Default::default()
            },
        })
        .unwrap();
    unwilling
        .dispatch(game_households::Tend { person: father })
        .unwrap();
    assert_eq!(
        unwilling.get::<game_physiology::Bodies>().unwrap().people[&child].hunger,
        hungry,
        "being a parent offers care without compelling it"
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        host.dispatch(game_households::Tend { person: father })
            .unwrap();
        assert!(host.get::<game_physiology::Bodies>().unwrap().people[&child].hunger < hungry);
        assert!(
            host.get::<game_households::Households>()
                .unwrap()
                .carers_of(child)
                .next()
                .is_none()
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn independence_does_not_erase_a_parents_voluntary_food_help() {
    let (mut h, area) = world();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let (mother, pack) = person(&mut h, area);
    let (father, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother,
            father,
            attendant: mother,
            area,
        })
        .unwrap();
    h.dispatch(game_traits::Set {
        person: mother,
        profile: [
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
            ("interpretation".into(), 0),
            ("intelligence".into(), 0),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_households::Independent { person: child })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_physiology::Eat {
        person: mother,
        inventory: pack,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: mother,
        profile: [
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
            ("interpretation".into(), 1000),
            ("intelligence".into(), 4000),
        ]
        .into(),
    })
    .unwrap();
    let hungry = h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger;
    assert!(hungry > 0);
    h.dispatch(game_households::Tend { person: mother })
        .unwrap();
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger < hungry);
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .caregivers
            .contains_key(&child)
    );
}
#[test]
fn foresight_can_support_safe_accompaniment_and_relationships_still_allow_refusal() {
    for affection in [100, -100] {
        let (mut h, area) = world();
        let (parent, _) = person(&mut h, area);
        let (other, _) = person(&mut h, area);
        h.dispatch(game_households::Enable).unwrap();
        let child = h
            .dispatch(game_lifecycle::Birth {
                mother: parent,
                father: other,
                attendant: parent,
                area,
            })
            .unwrap();
        let at = h.now().after(SimDuration(5 * 365 * DAY.0)).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
        h.dispatch(game_traits::Set {
            person: child,
            profile: [("foresight".into(), 16000), ("risk-tolerance".into(), 1000)].into(),
        })
        .unwrap();
        h.dispatch(game_relationships::SetRelationship {
            observer: child,
            other: parent,
            relationship: game_relationships::Relationship {
                affection,
                trust: affection,
                ..Default::default()
            },
        })
        .unwrap();
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
            person: parent,
            destination: away,
        })
        .unwrap();
        let at = h.now().after(MINUTE).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
        assert_eq!(
            game_transport::area(h.get().unwrap(), child).unwrap(),
            if affection > 0 { away } else { area }
        );
    }
}
#[test]
fn a_food_delivery_can_search_known_roads_after_the_recipient_leaves() {
    let (mut h, home) = world();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let (carer, pack) = person(&mut h, home);
    let (recipient, _) = person(&mut h, home);
    h.dispatch(game_traits::Set {
        person: carer,
        profile: [
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
            ("interpretation".into(), 1000),
            ("intelligence".into(), 4000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    let source = h
        .dispatch(game_land::CreateArea {
            name: "Source".into(),
        })
        .unwrap();
    let refuge = h
        .dispatch(game_land::CreateArea {
            name: "Refuge".into(),
        })
        .unwrap();
    for to in [source, refuge] {
        h.dispatch(game_land::Link {
            from: home,
            to,
            duration: MINUTE,
        })
        .unwrap();
    }
    h.dispatch(game_knowledge::ObserveSurroundings { person: carer })
        .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    h.dispatch(game_communication::Notice {
        observer: carer,
        person: recipient,
    })
    .unwrap();
    for (person, destination) in [(carer, source), (recipient, refuge)] {
        h.dispatch(game_transport::Move {
            person,
            destination,
        })
        .unwrap();
        let at = h.now().after(MINUTE).unwrap();
        while h.advance(at, 128).unwrap().stop == Stop::Budget {}
    }
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: carer,
        inventory: pack,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: carer })
        .unwrap();
    let hunger = h.get::<game_physiology::Bodies>().unwrap().people[&recipient].hunger;
    assert!(
        h.dispatch(game_care::Assist {
            caregiver: carer,
            beneficiary: recipient
        })
        .unwrap()
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    let at = h.now().after(SimDuration(10 * MINUTE.0)).unwrap();
    for host in [&mut h, &mut loaded] {
        while host.advance(at, 128).unwrap().stop == Stop::Budget {}
        assert_eq!(
            game_transport::area(host.get().unwrap(), carer).unwrap(),
            refuge
        );
        assert!(host.get::<game_physiology::Bodies>().unwrap().people[&recipient].hunger < hunger);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

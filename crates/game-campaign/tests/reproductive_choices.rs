mod support;
use game_core::*;
use game_sim::*;
use support::*;
fn profile(h: &mut Host, p: PersonId, drive: i32, role: i32) {
    let mut profile = h.get::<game_traits::Traits>().unwrap().people[&p].clone();
    profile.insert("reproductive-drive".into(), drive);
    profile.insert("aesthetic-sensitivity".into(), 0);
    profile.insert("empathy".into(), 500);
    profile.insert("rigidity".into(), 1000);
    profile.insert("reproductive_role".into(), role);
    h.dispatch(game_traits::Set { person: p, profile }).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(game_physiology::Express { person: p }).unwrap();
}
fn bond(h: &mut Host, a: PersonId, b: PersonId, kinship: i32) {
    for (observer, other) in [(a, b), (b, a)] {
        set_opinion(h, observer, "kinship", kinship);
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
    assert!(
        h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::CoParent
        })
        .unwrap()
    );
}
#[test]
fn an_incompatible_first_coparent_does_not_hide_a_compatible_later_partner() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (incompatible, _) = person(&mut h, area);
    let (compatible, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    for (p, role) in [(a, 0), (incompatible, 0), (compatible, 2000)] {
        profile(&mut h, p, 500, role);
    }
    bond(&mut h, a, incompatible, 0);
    bond(&mut h, a, compatible, 0);
    let offers = h.dispatch(game_choice::Collect { person: a }).unwrap();
    assert!(
        !offers
            .iter()
            .any(|o| o.key == format!("parenthood:{}", incompatible.0))
    );
    assert!(
        offers
            .iter()
            .any(|o| o.key == format!("parenthood:{}", compatible.0))
    );
    assert!(h.dispatch(game_parenthood::Consider { person: a }).unwrap());
    let pop = h.get::<game_lifecycle::Population>().unwrap();
    assert_eq!(pop.pregnancies.len(), 1);
    let p = pop.pregnancies.values().next().unwrap();
    assert_eq!((p.mother, p.father), (a, compatible));
}
#[test]
fn physiological_drive_can_motivate_consensual_reproduction_and_reward_it_but_cannot_override_refusal()
 {
    for (drive, refuses, expected) in [(0, false, false), (1000, false, true), (1000, true, false)]
    {
        let (mut h, area) = world();
        let (a, _) = person(&mut h, area);
        let (b, _) = person(&mut h, area);
        h.dispatch(game_households::Enable).unwrap();
        profile(&mut h, a, drive, 0);
        profile(&mut h, b, drive, 2000);
        bond(&mut h, a, b, -50);
        if refuses {
            h.dispatch(game_relationships::SetRelationship {
                observer: b,
                other: a,
                relationship: game_relationships::Relationship {
                    affection: 50,
                    trust: 20,
                    fear: 100,
                    ..Default::default()
                },
            })
            .unwrap();
        }
        let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
        for host in [&mut h, &mut loaded] {
            assert_eq!(
                host.dispatch(game_parenthood::Consider { person: a })
                    .unwrap(),
                expected
            );
            assert_eq!(
                !host
                    .get::<game_lifecycle::Population>()
                    .unwrap()
                    .pregnancies
                    .is_empty(),
                expected
            );
            if expected {
                assert!(host.get::<game_vitality::Vitality>().unwrap().people[&a].satisfaction > 0);
            }
        }
        assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    }
}
#[test]
fn family_formation_considers_visible_peers_beyond_the_lowest_sixteen_ids() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let peers: Vec<_> = (0..24).map(|_| person(&mut h, area).0).collect();
    h.dispatch(game_households::Enable).unwrap();
    let target = peers[16];
    for (observer, other) in [(a, target), (target, a)] {
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
    let offers = h.dispatch(game_choice::Collect { person: a }).unwrap();
    assert!(
        offers
            .iter()
            .any(|o| o.key == format!("family:exclusive:{}", target.0))
    );
}

#[test]
fn companions_can_renegotiate_coparenting_when_desire_changes_without_a_forced_breakup() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    profile(&mut h, a, 0, 0);
    profile(&mut h, b, 0, 2000);
    for (observer, other) in [(a, b), (b, a)] {
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
    assert!(
        h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::Companion
        })
        .unwrap()
    );
    assert!(
        !h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::CoParent
        })
        .unwrap()
    );
    for (p, role) in [(a, 0), (b, 2000)] {
        profile(&mut h, p, 1000, role);
    }
    let offers = h.dispatch(game_choice::Collect { person: a }).unwrap();
    assert!(
        offers
            .iter()
            .any(|o| o.key == format!("family:coparent:{}", b.0))
    );
    assert!(
        h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::CoParent
        })
        .unwrap()
    );
    assert!(h.dispatch(game_parenthood::Consider { person: a }).unwrap());
}
#[test]
fn renegotiating_an_exclusive_bond_keeps_primary_household_records_consistent() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    profile(&mut h, a, 0, 0);
    profile(&mut h, b, 0, 2000);
    bond(&mut h, a, b, -50);
    for p in [a, b] {
        set_opinion(&mut h, p, "exclusivity", 100);
    }
    assert!(
        h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::Exclusive
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<game_households::Households>().unwrap().partners[&a],
        b
    );
    for p in [a, b] {
        set_opinion(&mut h, p, "exclusivity", 0);
        set_opinion(&mut h, p, "kinship", 100);
    }
    assert!(
        h.dispatch(game_family_life::Bond {
            person: a,
            other: b,
            kind: game_family_life::Kind::CoParent
        })
        .unwrap()
    );
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .partners
            .contains_key(&a)
    );
    assert_eq!(
        h.get::<game_family_life::Bonds>().unwrap().partners[&a][&b],
        game_family_life::Kind::CoParent
    );
    let loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn a_pregnant_partner_does_not_offer_an_impossible_conception_or_block_other_partners() {
    let (mut h, area) = world();
    let (father, _) = person(&mut h, area);
    let (first, _) = person(&mut h, area);
    let (second, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    for (person, role) in [(father, 2000), (first, 0), (second, 0)] {
        profile(&mut h, person, 1000, role);
    }
    bond(&mut h, father, first, 0);
    bond(&mut h, father, second, 0);
    assert!(
        h.dispatch(game_parenthood::ConsiderWith {
            person: father,
            partner: first
        })
        .unwrap()
    );
    let offers = h.dispatch(game_choice::Collect { person: father }).unwrap();
    assert!(
        !offers
            .iter()
            .any(|o| o.key == format!("parenthood:{}", first.0))
    );
    assert!(
        offers
            .iter()
            .any(|o| o.key == format!("parenthood:{}", second.0))
    );
    assert!(
        h.dispatch(game_parenthood::ConsiderWith {
            person: father,
            partner: second
        })
        .unwrap()
    );
    assert_eq!(
        h.get::<game_lifecycle::Population>()
            .unwrap()
            .pregnancies
            .len(),
        2
    );
}

use game_core::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_conflict::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_pressure::plugin(&mut b).unwrap();
    game_communities::plugin(&mut b).unwrap();
    b
}
fn setup(sharing: i32, enabled: bool) -> (Host, Vec<PersonId>, Vec<InventoryId>) {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Wild grove".into(),
        })
        .unwrap();
    let mut people = vec![];
    let mut packs = vec![];
    for name in ["Mira Ash", "Oren Reed"] {
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
        h.dispatch(game_relationships::DefineBeliefs {
            person,
            beliefs: game_relationships::Beliefs {
                sharing,
                ..Default::default()
            },
        })
        .unwrap();
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(person),
                capacity: 1000,
            })
            .unwrap();
        h.dispatch(game_property::ClaimInventory {
            claimant: person,
            inventory,
            recognized_by: [person].into(),
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity: 100,
        })
        .unwrap();
        h.dispatch(game_knowledge::ObserveStore { person, inventory })
            .unwrap();
        people.push(person);
        packs.push(inventory);
    }
    for (a, b) in [(people[0], people[1]), (people[1], people[0])] {
        h.dispatch(game_relationships::SetRelationship {
            observer: a,
            other: b,
            relationship: game_relationships::Relationship {
                trust: 10,
                affection: 20,
                ..Default::default()
            },
        })
        .unwrap();
    }
    if enabled {
        h.dispatch(game_communities::Enable).unwrap();
    }
    (h, people, packs)
}
fn run(h: &mut Host) {
    while h.advance(SimTime(5 * MINUTE.0), 128).unwrap().stop == Stop::Budget {}
}
#[test]
fn consenting_friends_name_and_adopt_real_reciprocal_access_rules() {
    let (mut h, p, packs) = setup(10, true);
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(p[0], packs[1])
    );
    h.dispatch(game_social_life::Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for h in [&mut h, &mut loaded] {
        run(h);
        let groups = h.get::<game_institutions::Institutions>().unwrap();
        let group = groups.groups.values().next().unwrap();
        assert_eq!(group.name, "Reed Hearth");
        assert_eq!(group.rules.len(), 2);
        assert_eq!(group.procedure, game_institutions::Procedure::Personal);
        assert!(groups.proposals.is_empty());
        assert!(
            h.get::<game_property::Property>()
                .unwrap()
                .allowed(p[0], packs[1])
        );
        assert!(
            h.get::<game_property::Property>()
                .unwrap()
                .allowed(p[1], packs[0])
        );
        assert_eq!(game_communities::view(h, p[0]).unwrap().len(), 1);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn disagreeing_or_player_controlled_people_do_not_acquire_implicit_duties() {
    for (sharing, enabled, player) in [(-10, true, false), (10, false, false), (10, true, true)] {
        let (mut h, p, _) = setup(sharing, enabled);
        if player {
            h.dispatch(game_lineage::Begin { person: p[0] }).unwrap();
        }
        h.dispatch(game_social_life::Talk {
            person: p[0],
            other: p[1],
        })
        .unwrap();
        run(&mut h);
        assert!(
            h.get::<game_institutions::Institutions>()
                .unwrap()
                .groups
                .is_empty()
        );
        assert!(
            h.get::<game_communities::Communities>()
                .unwrap()
                .members
                .is_empty()
        );
    }
}

fn traits(h: &mut Host, person: PersonId, ambition: i32, risk: i32) {
    h.dispatch(game_traits::Set {
        person,
        profile: [
            ("ambition".into(), ambition),
            ("risk-tolerance".into(), risk),
        ]
        .into(),
    })
    .unwrap();
}
fn beliefs(h: &mut Host, person: PersonId, sharing: i32, authority: i32) {
    h.dispatch(game_relationships::DefineBeliefs {
        person,
        beliefs: game_relationships::Beliefs {
            sharing,
            authority,
            honesty: 0,
        },
    })
    .unwrap();
}
fn advance(h: &mut Host, minutes: u64) {
    while h.advance(SimTime(minutes * MINUTE.0), 128).unwrap().stop == Stop::Budget {}
}
#[test]
fn people_can_choose_voting_after_learning_to_value_joint_decisions() {
    let (mut h, p, packs) = setup(10, true);
    for person in &p {
        beliefs(&mut h, *person, 10, -10);
    }
    h.dispatch(game_social_life::Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    advance(&mut h, 5);
    let id = h.get::<game_communities::Communities>().unwrap().members[&p[0]];
    assert_eq!(
        h.get::<game_institutions::Institutions>().unwrap().groups[&id].procedure,
        game_institutions::Procedure::Personal
    );
    advance(&mut h, 15);
    h.dispatch(game_social_life::Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    advance(&mut h, 20);
    assert_eq!(
        h.get::<game_institutions::Institutions>().unwrap().groups[&id].procedure,
        game_institutions::Procedure::Unanimity
    );
    assert!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .attachments
            .values()
            .all(|a| *a == game_communities::Attachment::Voice)
    );
    // Adopting voting does not make a dissenter vote yes.
    let proposal = h
        .dispatch(game_institutions::Propose {
            person: p[0],
            institution: id,
            terms: game_institutions::AccessRule {
                custodian: p[0],
                inventory: packs[0],
                users: [p[1]].into(),
            },
        })
        .unwrap();
    assert!(
        !h.dispatch(game_institutions::Vote {
            person: p[0],
            proposal,
            support: true
        })
        .unwrap()
    );
    assert!(
        !h.dispatch(game_institutions::Vote {
            person: p[1],
            proposal,
            support: false
        })
        .unwrap()
    );
    assert!(
        !h.get::<game_institutions::Institutions>()
            .unwrap()
            .proposals[&proposal]
            .adopted
    );
}
#[test]
fn admiration_can_support_a_leader_without_ballots_or_fear() {
    let (mut h, p, packs) = setup(-10, true);
    traits(&mut h, p[0], 1000, 1000);
    traits(&mut h, p[1], 0, 0);
    for person in &p {
        beliefs(&mut h, *person, -10, 10);
    }
    h.dispatch(game_relationships::SetRelationship {
        observer: p[1],
        other: p[0],
        relationship: game_relationships::Relationship {
            affection: 45,
            trust: 30,
            ..Default::default()
        },
    })
    .unwrap();
    h.dispatch(game_social_life::Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    run(&mut h);
    let s = h.get::<game_communities::Communities>().unwrap();
    let id = s.members[&p[0]];
    assert_eq!(
        s.attachments[&p[1]],
        game_communities::Attachment::Loyalty(p[0])
    );
    let i = h.get::<game_institutions::Institutions>().unwrap();
    assert_eq!(
        i.groups[&id].procedure,
        game_institutions::Procedure::Leader(p[0])
    );
    assert!(i.proposals.is_empty());
    assert!(
        h.get::<game_property::Property>()
            .unwrap()
            .allowed(p[0], packs[1])
    );
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(p[1], packs[0])
    );
    // Losing personal support removes practical power even if the institution keeps its title.
    h.dispatch(game_relationships::Interpret {
        observer: p[1],
        actor: p[0],
        shared: false,
        harmed: true,
        credible: true,
    })
    .unwrap();
    let proposal = h
        .dispatch(game_institutions::Propose {
            person: p[0],
            institution: id,
            terms: game_institutions::AccessRule {
                custodian: p[1],
                inventory: packs[1],
                users: [p[0]].into(),
            },
        })
        .unwrap();
    let save = h.save().unwrap();
    assert!(
        h.dispatch(game_institutions::Decide {
            person: p[0],
            proposal,
            support: true
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), save);
    advance(&mut h, 15);
    h.dispatch(game_social_life::Talk {
        person: p[0],
        other: p[1],
    })
    .unwrap();
    advance(&mut h, 20);
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(p[0], packs[1])
    );
    assert_eq!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .attachments[&p[1]],
        game_communities::Attachment::Independent
    );
}
#[test]
fn actual_force_can_produce_compliance_without_affection_or_votes() {
    let (mut h, p, packs) = setup(-10, true);
    traits(&mut h, p[0], 1000, 1000);
    traits(&mut h, p[1], 0, 0);
    h.dispatch(game_pressure::DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    assert!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .members
            .is_empty()
    );
    advance(&mut h, 1);
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&p[1]].health,
        90
    );
    assert!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .members
            .is_empty()
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for h in [&mut h, &mut loaded] {
        advance(h, 5);
        assert!(
            h.get::<game_pressure::Pressure>()
                .unwrap()
                .active
                .is_empty()
        );
        assert!(h.get::<Actions>().unwrap().0.is_empty());
        let rel = &h.get::<game_relationships::Social>().unwrap().relationships[&p[1]][&p[0]];
        assert!(rel.fear > 0 && rel.trust < 0);
        assert_eq!(rel.affection, 20);
        let state = h.get::<game_communities::Communities>().unwrap();
        assert_eq!(
            state.attachments[&p[1]],
            game_communities::Attachment::Fear(p[0])
        );
        assert!(
            !game_communities::view(h, p[0]).unwrap()[0]
                .attachments
                .contains_key(&p[1])
        );
        assert_eq!(
            game_communities::view(h, p[1]).unwrap()[0].attachments[&p[1]],
            game_communities::Attachment::Fear(p[0])
        );
        assert!(
            h.get::<game_property::Property>()
                .unwrap()
                .allowed(p[0], packs[1])
        );
        assert!(
            h.get::<game_institutions::Institutions>()
                .unwrap()
                .proposals
                .is_empty()
        );
        let subject = h.get::<game_physiology::Bodies>().unwrap().people[&p[1]].health;
        assert!(subject > 0 && subject < 100);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn resistance_can_force_the_aggressor_to_abandon_a_demand() {
    let (mut h, p, packs) = setup(-10, true);
    traits(&mut h, p[0], 1000, 0);
    traits(&mut h, p[1], 0, 1000);
    h.dispatch(game_pressure::DemandSubmission {
        leader: p[0],
        subject: p[1],
    })
    .unwrap();
    advance(&mut h, 5);
    assert!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .members
            .is_empty()
    );
    assert!(
        !h.get::<game_property::Property>()
            .unwrap()
            .allowed(p[0], packs[1])
    );
    assert!(
        h.get::<game_pressure::Pressure>()
            .unwrap()
            .active
            .is_empty()
    );
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&p[0]].health < 100);
}

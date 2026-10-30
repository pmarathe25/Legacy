use super::support;
use game_institutions::{Institution, Institutions, Procedure};
use support::*;
#[test]
fn ambition_alone_does_not_make_every_member_accept_repeated_leadership_reversals() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    h.dispatch(game_communities::Enable).unwrap();
    for (p, other) in [(a, b), (b, a)] {
        let mut profile = h.get::<game_traits::Traits>().unwrap().people[&p].clone();
        profile.insert("ambition".into(), 1000);
        profile.insert("rigidity".into(), 1000);
        h.dispatch(game_traits::Set { person: p, profile }).unwrap();
        set_opinion(&mut h, p, "authority", 10);
        h.dispatch(game_relationships::SetRelationship {
            observer: p,
            other,
            relationship: game_relationships::Relationship {
                affection: 30,
                trust: 30,
                ..Default::default()
            },
        })
        .unwrap();
    }
    let group = h
        .dispatch(game_institutions::Found(Institution {
            name: "A compact".into(),
            jurisdiction: area,
            members: [a, b].into(),
            procedure: Procedure::Leader(a),
            offices: Default::default(),
            rules: vec![],
        }))
        .unwrap();
    for p in [a, b] {
        h.dispatch(game_communities::Membership {
            institution: group,
            person: p,
            attachment: Some(if p == a {
                game_communities::Attachment::Independent
            } else {
                game_communities::Attachment::Loyalty(a)
            }),
        })
        .unwrap();
    }
    for _ in 0..4 {
        for p in [a, b] {
            for offer in h.dispatch(game_choice::Collect { person: p }).unwrap() {
                if offer.key.starts_with("affiliation:") {
                    let _ = h.dispatch(game_choice::Run(offer.request));
                }
            }
        }
        assert_eq!(
            h.get::<Institutions>().unwrap().groups[&group].procedure,
            Procedure::Leader(a)
        );
    }
    // A strong changed preference can still outweigh even maximal rigidity.
    for p in [a, b] {
        set_opinion(&mut h, p, "authority", -10);
        set_opinion(&mut h, p, "independence", 100);
    }
    let proposal = h
        .dispatch(game_choice::Collect { person: a })
        .unwrap()
        .into_iter()
        .find(|o| {
            o.key
                == format!(
                    "affiliation:arrangement:{}:{}",
                    group.0,
                    game_core::fingerprint("Personal")
                )
        })
        .unwrap();
    h.dispatch(game_choice::Run(proposal.request)).unwrap();
    assert_eq!(
        h.get::<Institutions>().unwrap().groups[&group].procedure,
        Procedure::Personal
    );
}

#[test]
fn fearful_compliance_can_enact_a_takeover_but_one_submission_does_not_bind_everyone() {
    let (mut h, area) = world();
    let (claimant, _) = person(&mut h, area);
    let (first, pack) = person(&mut h, area);
    let (second, _) = person(&mut h, area);
    h.dispatch(game_communities::Enable).unwrap();
    // An aggressor need not believe in the legitimacy of authority to seize it.
    set_opinion(&mut h, claimant, "authority", -10);
    for subject in [first, second] {
        set_opinion(&mut h, subject, "independence", 30);
        set_opinion(&mut h, subject, "authority", -10);
    }
    let group = h
        .dispatch(game_institutions::Found(Institution {
            name: "A disputed compact".into(),
            jurisdiction: area,
            members: [claimant, first, second].into(),
            procedure: Procedure::Personal,
            offices: Default::default(),
            rules: vec![],
        }))
        .unwrap();
    for person in [claimant, first, second] {
        h.dispatch(game_communities::Membership {
            institution: group,
            person,
            attachment: Some(game_communities::Attachment::Independent),
        })
        .unwrap();
    }
    h.dispatch(game_relationships::SetRelationship {
        observer: first,
        other: claimant,
        relationship: game_relationships::Relationship {
            fear: 100,
            grievance: 30,
            ..Default::default()
        },
    })
    .unwrap();
    h.dispatch(game_pressure::DemandSubmission {
        leader: claimant,
        subject: first,
    })
    .unwrap();
    assert_eq!(
        h.get::<Institutions>().unwrap().groups[&group].procedure,
        Procedure::Personal
    );
    assert!(
        h.get::<game_property::Property>()
            .unwrap()
            .allowed(claimant, pack)
    );
    assert_eq!(
        h.get::<game_communities::Communities>()
            .unwrap()
            .attachments[&first],
        game_communities::Attachment::Fear(claimant)
    );
    h.dispatch(game_relationships::SetRelationship {
        observer: second,
        other: claimant,
        relationship: game_relationships::Relationship {
            fear: 100,
            grievance: 30,
            ..Default::default()
        },
    })
    .unwrap();
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for h in [&mut h, &mut loaded] {
        h.dispatch(game_pressure::DemandSubmission {
            leader: claimant,
            subject: second,
        })
        .unwrap();
        assert_eq!(
            h.get::<Institutions>().unwrap().groups[&group]
                .procedure
                .sole_decider(),
            Some(claimant)
        );
        for subject in [first, second] {
            assert_eq!(
                h.get::<game_communities::Communities>()
                    .unwrap()
                    .attachments[&subject],
                game_communities::Attachment::Fear(claimant)
            );
            assert_eq!(
                h.get::<game_relationships::Social>()
                    .unwrap()
                    .opinion(subject, "authority"),
                -10
            );
            assert!(
                !h.get::<game_identity::Identity>()
                    .unwrap()
                    .people
                    .get(&subject)
                    .is_some_and(|r| r.contains_key(&format!("institution:{}", group.0)))
            );
        }
        let proposal = h
            .dispatch(game_institutions::Propose {
                person: claimant,
                institution: group,
                terms: game_institutions::AccessRule {
                    custodian: first,
                    inventory: pack,
                    users: [claimant].into(),
                },
            })
            .unwrap();
        assert!(
            h.dispatch(game_institutions::Decide {
                person: claimant,
                proposal,
                support: true
            })
            .unwrap()
        );
        assert!(h.get::<Institutions>().unwrap().proposals[&proposal].enacted);
        assert!(
            h.get::<game_property::Property>()
                .unwrap()
                .allowed(claimant, pack)
        );
        assert!(
            !game_communities::view(h, claimant)
                .unwrap()
                .iter()
                .any(|v| v.attachments.contains_key(&first))
        );
        h.dispatch(game_relationships::SetRelationship {
            observer: first,
            other: claimant,
            relationship: game_relationships::Relationship {
                grievance: 30,
                ..Default::default()
            },
        })
        .unwrap();
        let departure = h
            .dispatch(game_choice::Collect { person: first })
            .unwrap()
            .into_iter()
            .find(|o| o.key == format!("affiliation:leave:{}", group.0))
            .unwrap();
        h.dispatch(game_choice::Run(departure.request)).unwrap();
        assert!(
            !h.get::<game_property::Property>()
                .unwrap()
                .allowed(claimant, pack)
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}

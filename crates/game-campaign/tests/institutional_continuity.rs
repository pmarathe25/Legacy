mod support;
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
        .find(|o| o.key == format!("affiliation:personal:{}", group.0))
        .unwrap();
    h.dispatch(game_choice::Run(proposal.request)).unwrap();
    assert_eq!(
        h.get::<Institutions>().unwrap().groups[&group].procedure,
        Procedure::Personal
    );
}

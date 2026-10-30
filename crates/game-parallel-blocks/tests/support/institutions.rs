use crate::Engine;
use game_parallel::{
    Amount, GoodsDirection, GoodsTransfer, InstitutionAction, InstitutionVote, TradeOutcome,
    TradeProposal, World,
};
fn world() -> World {
    World::from_toml(include_str!(
        "../../../../content/parallel/institutions.toml"
    ))
    .unwrap()
}
#[test]
fn ballot_records_a_trade_rule_without_physically_blocking_trade() {
    let mut initial = world();
    initial.market.as_mut().unwrap().policies.clear();
    let mut engine = Engine::compile(initial).unwrap();
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 2,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    let trade = engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            receive: vec![],
        })
        .unwrap();
    let ballot = engine
        .institutions()
        .open(1, 1, InstitutionAction::Trade { open: false }, 2)
        .unwrap();
    let votes = [
        InstitutionVote {
            ballot,
            voter: 1,
            yes: true,
        },
        InstitutionVote {
            ballot,
            voter: 2,
            yes: true,
        },
    ];
    let before = engine.export();
    assert!(engine.institutions().vote(&[votes[0], votes[0]]).is_err());
    assert_eq!(engine.export(), before);
    engine.institutions().vote(&votes).unwrap();
    engine.step().unwrap();
    assert!(engine.export().institutions.as_ref().unwrap().bodies[0].trade_open);
    let state = engine.export();
    engine = Engine::compile(World::from_json(&serde_json::to_string(&state).unwrap()).unwrap())
        .unwrap();
    engine.step().unwrap();
    assert!(!engine.export().institutions.as_ref().unwrap().bodies[0].trade_open);
    engine
        .decide_trade(trade, 2, TradeOutcome::Accepted)
        .unwrap();
    assert_eq!(
        engine
            .export()
            .goods_cargo
            .unwrap()
            .holdings
            .iter()
            .find(|holding| holding.person == 2)
            .unwrap()
            .items[0]
            .quantity,
        2
    );
    let reopen = engine
        .institutions()
        .open(1, 1, InstitutionAction::Trade { open: true }, 1)
        .unwrap();
    engine
        .institutions()
        .vote(&[
            InstitutionVote {
                ballot: reopen,
                voter: 1,
                yes: true,
            },
            InstitutionVote {
                ballot: reopen,
                voter: 2,
                yes: true,
            },
        ])
        .unwrap();
    engine.step().unwrap();
    assert!(engine.export().institutions.as_ref().unwrap().bodies[0].trade_open);
    engine
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![],
        })
        .unwrap();
}
#[test]
fn ordinary_and_compiled_hosts_resolve_the_same_ballot() {
    let mut reference = world();
    let mut engine = Engine::compile(reference.clone()).unwrap();
    let people = reference
        .production
        .as_ref()
        .unwrap()
        .validate_people(&reference.regions, &reference.travelers, reference.tick)
        .unwrap();
    let a = reference
        .institutions
        .as_mut()
        .unwrap()
        .open(
            &people,
            1,
            1,
            InstitutionAction::Elect { candidate: 2 },
            1,
            0,
        )
        .unwrap();
    let b = engine
        .institutions()
        .open(1, 1, InstitutionAction::Elect { candidate: 2 }, 1)
        .unwrap();
    assert_eq!(a, b);
    let votes = [
        InstitutionVote {
            ballot: a,
            voter: 1,
            yes: true,
        },
        InstitutionVote {
            ballot: a,
            voter: 2,
            yes: true,
        },
    ];
    reference
        .institutions
        .as_mut()
        .unwrap()
        .vote(&people, &votes)
        .unwrap();
    engine.institutions().vote(&votes).unwrap();
    reference.step().unwrap();
    engine.step().unwrap();
    assert_eq!(engine.export(), reference);
    assert_eq!(reference.institutions.unwrap().bodies[0].chair, Some(2));
}
#[test]
fn failed_family_phase_rolls_back_due_institutional_ballot() {
    let mut initial = world();
    let family =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    initial.regions = family.regions;
    initial.routes = family.routes;
    initial.families = family.families;
    initial
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = u64::MAX;
    let mut engine = Engine::compile(initial).unwrap();
    let ballot = engine
        .institutions()
        .open(1, 1, InstitutionAction::Trade { open: false }, 1)
        .unwrap();
    engine
        .institutions()
        .vote(&[
            InstitutionVote {
                ballot,
                voter: 1,
                yes: true,
            },
            InstitutionVote {
                ballot,
                voter: 2,
                yes: true,
            },
        ])
        .unwrap();
    let before = engine.export();
    assert!(engine.step().is_err());
    assert_eq!(engine.export(), before);
}

#[test]
fn recorded_trade_rule_does_not_force_automatic_compliance() {
    let mut reference = world();
    reference
        .transfer_goods(&[GoodsTransfer {
            person: 1,
            good: 1,
            quantity: 2,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    reference
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 2,
            give: vec![Amount {
                good: 1,
                quantity: 2,
            }],
            receive: vec![],
        })
        .unwrap();
    reference.institutions.as_mut().unwrap().bodies[0].trade_open = false;
    reference.autowork =
        World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))
            .unwrap()
            .autowork;
    reference.validate().unwrap();
    let mut engine = Engine::compile(reference.clone()).unwrap();
    reference.step().unwrap();
    engine.step().unwrap();
    let result = engine.export();
    assert_eq!(result, reference);
    assert!(result.market.as_ref().unwrap().offers.is_empty());
}
#[test]
fn ruler_succession_does_not_grant_an_office() {
    let mut initial = world();
    let family =
        World::from_toml(include_str!("../../../../content/parallel/lineage.toml")).unwrap();
    initial.regions = family.regions;
    initial.routes = family.routes;
    initial.families = family.families;
    initial.lineage = family.lineage;
    initial
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    let mut engine = Engine::compile(initial).unwrap();
    for _ in 0..7 {
        engine.step().unwrap();
    }
    let state = engine.export();
    assert!(state.lineage.as_ref().unwrap().current.is_some());
    assert_eq!(state.institutions.as_ref().unwrap().bodies[0].chair, None);
}

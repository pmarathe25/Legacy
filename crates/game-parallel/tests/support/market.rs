use game_parallel::{Amount, GoodsDirection, GoodsTransfer, TradeOutcome, TradeProposal, World};
fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/workshop.toml")).unwrap();
    world.production.as_mut().unwrap().inventories[0].stocks[1].quantity = 2;
    world
        .transfer_goods(&[
            GoodsTransfer {
                person: 1,
                good: 1,
                quantity: 3,
                direction: GoodsDirection::Pickup,
            },
            GoodsTransfer {
                person: 2,
                good: 2,
                quantity: 2,
                direction: GoodsDirection::Pickup,
            },
        ])
        .unwrap();
    world
}
fn proposal() -> TradeProposal {
    TradeProposal {
        proposer: 1,
        recipient: 2,
        give: vec![Amount {
            good: 1,
            quantity: 3,
        }],
        receive: vec![Amount {
            good: 2,
            quantity: 2,
        }],
    }
}
#[test]
fn only_recipient_can_accept_and_possessions_move_only_after_consent() {
    let mut w = world();
    let holdings = w.goods_cargo.clone();
    let id = w.propose_trade(proposal()).unwrap();
    assert_eq!(w.goods_cargo, holdings);
    let before = w.clone();
    assert!(w.decide_trade(id, 1, TradeOutcome::Accepted).is_err());
    assert_eq!(w, before);
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.decide_trade(id, 2, TradeOutcome::Accepted).unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(
        w.market.as_ref().unwrap().events[0].outcome,
        TradeOutcome::Accepted
    );
    assert_eq!(w.goods_cargo.as_ref().unwrap().holdings[0].items[0].good, 2);
    assert_eq!(w.goods_cargo.as_ref().unwrap().holdings[1].items[0].good, 1);
    w.validate().unwrap();
}
#[test]
fn refusal_and_stale_offers_do_not_transfer_goods() {
    let mut w = world();
    let id = w.propose_trade(proposal()).unwrap();
    let holdings = w.goods_cargo.clone();
    w.decide_trade(id, 2, TradeOutcome::Declined).unwrap();
    assert_eq!(w.goods_cargo, holdings);
    let id = w.propose_trade(proposal()).unwrap();
    w.transfer_goods(&[GoodsTransfer {
        person: 1,
        good: 1,
        quantity: 3,
        direction: GoodsDirection::Deposit,
    }])
    .unwrap();
    let before = w.clone();
    assert!(w.decide_trade(id, 2, TradeOutcome::Accepted).is_err());
    assert_eq!(w, before);
    w.decide_trade(id, 1, TradeOutcome::Withdrawn).unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
}

#[test]
fn batched_offers_and_decisions_commit_atomically_across_workers() {
    use game_parallel::{GoodsHolding, Person, TradeDecision};
    let mut initial = world();
    let count = 2_000u64;
    initial.regions[0].people = (100..100 + count * 2)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    initial.goods_cargo.as_mut().unwrap().holdings = (0..count * 2)
        .map(|i| GoodsHolding {
            person: 100 + i,
            items: vec![Amount {
                good: (i % 2 + 1) as u32,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    let proposals: Vec<_> = (0..count)
        .map(|i| TradeProposal {
            proposer: 100 + 2 * i,
            recipient: 101 + 2 * i,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        })
        .collect();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        let ids = pool.install(|| w.propose_trades(&proposals)).unwrap();
        let decisions: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, &id)| TradeDecision {
                id,
                actor: 101 + 2 * i as u64,
                outcome: TradeOutcome::Accepted,
            })
            .collect();
        let mut invalid = decisions.clone();
        invalid.last_mut().unwrap().actor = 100;
        let before = w.clone();
        assert!(pool.install(|| w.decide_trades(&invalid)).is_err());
        assert_eq!(w, before);
        pool.install(|| w.decide_trades(&decisions)).unwrap();
        assert!(w.market.as_ref().unwrap().offers.is_empty());
        assert_eq!(w.market.as_ref().unwrap().events.len(), count as usize);
        assert!(
            w.goods_cargo
                .as_ref()
                .unwrap()
                .holdings
                .iter()
                .enumerate()
                .all(|(index, h)| h.items[0].good == if index % 2 == 0 { 2 } else { 1 })
        );
        w.validate().unwrap();
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
}

fn policy(person: u64, values: &[(u32, u64)]) -> game_parallel::TradePolicy {
    game_parallel::TradePolicy {
        person,
        enabled: true,
        propose: false,
        request_help: false,
        min_gain: 0,
        help_value: 0,
        influences: Vec::new(),
        values: values
            .iter()
            .map(|&(good, value)| game_parallel::TradeValue { good, value })
            .collect(),
    }
}
#[test]
fn opted_in_recipient_accepts_from_snapshot_and_labor_is_exclusive() {
    use game_parallel::CropPhase;
    let mut w = world();
    w.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    w.social = World::from_toml(include_str!("../../../../content/parallel/social.toml"))
        .unwrap()
        .social;
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2), (2, 1)]));
    let id = w.propose_trade(proposal()).unwrap();
    w.step().unwrap();
    let market = w.market.as_ref().unwrap();
    assert!(market.offers.is_empty());
    assert_eq!(market.events[0].offer, id);
    assert_eq!(market.events[0].tick, 1);
    assert_eq!(market.events[0].outcome, TradeOutcome::Accepted);
    assert!(w.social.as_ref().unwrap().events.is_empty());
    assert!(
        w.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|f| f.phase == CropPhase::Fallow)
    );
    assert_eq!(w.goods_cargo.as_ref().unwrap().holdings[0].items[0].good, 2);
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().events.len(), 1);
}
#[test]
fn infeasible_lowest_offer_does_not_block_feasible_offer_and_conflicts_are_deferred() {
    use game_parallel::Person;
    let mut w = world();
    w.regions[0].people.push(Person {
        id: 4,
        health: 100,
        hunger: 0,
    });
    w.market.as_mut().unwrap().policies = vec![policy(2, &[(1, 2), (2, 1)]), policy(4, &[(1, 1)])];
    let mut bad = proposal();
    bad.give[0].quantity = 4;
    let bad_id = w.propose_trade(bad).unwrap();
    let good_id = w.propose_trade(proposal()).unwrap();
    let gift_id = w
        .propose_trade(TradeProposal {
            proposer: 1,
            recipient: 4,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![],
        })
        .unwrap();
    w.step().unwrap();
    let market = w.market.as_ref().unwrap();
    assert_eq!(market.events[0].offer, good_id);
    assert_eq!(
        market
            .offers
            .iter()
            .map(|offer| offer.id)
            .collect::<Vec<_>>(),
        vec![bad_id, gift_id]
    );
}
#[test]
fn missing_valuation_and_busy_participant_do_not_accept() {
    use game_parallel::StartWork;
    let mut w = world();
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2)]));
    let id = w.propose_trade(proposal()).unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().offers[0].id, id);
    w.market.as_mut().unwrap().policies[0]
        .values
        .push(game_parallel::TradeValue { good: 2, value: 1 });
    w.start_work(&[StartWork {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    }])
    .unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().offers[0].id, id);
}
#[test]
fn automatic_trade_rolls_back_on_later_family_failure() {
    let mut w = world();
    let family =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.regions = family.regions;
    w.routes = family.routes;
    w.families = family.families;
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 3)
        .unwrap()
        .age = u64::MAX;
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2), (2, 1)]));
    w.propose_trade(proposal()).unwrap();
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn equal_value_full_capacity_swap_is_feasible() {
    let mut w = world();
    w.goods_cargo.as_mut().unwrap().capacity = 3;
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2), (2, 3)]));
    w.propose_trade(proposal()).unwrap();
    w.step().unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(
        w.goods_cargo.as_ref().unwrap().holdings[0].items[0].quantity,
        2
    );
    assert_eq!(
        w.goods_cargo.as_ref().unwrap().holdings[1].items[0].quantity,
        3
    );
}
#[test]
fn invalid_valuation_overflow_rolls_back_whole_tick() {
    let mut w = world();
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, u64::MAX), (2, u64::MAX)]));
    let p = TradeProposal {
        proposer: 1,
        recipient: 2,
        give: vec![
            Amount {
                good: 1,
                quantity: u64::MAX,
            },
            Amount {
                good: 2,
                quantity: u64::MAX,
            },
        ],
        receive: vec![],
    };
    w.propose_trade(p).unwrap();
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
#[test]
fn arriving_person_cannot_accept_until_the_following_tick() {
    use game_parallel::Move;
    let mut w = world();
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2), (2, 1)]));
    let id = w.propose_trade(proposal()).unwrap();
    w.move_people(&[Move {
        person: 2,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().offers[0].id, id);
    w.move_people(&[Move {
        person: 2,
        destination: 1,
    }])
    .unwrap();
    w.step().unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().offers[0].id, id);
    w.step().unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
}

#[test]
fn automatic_decisions_scale_across_independent_pairs_without_layout_dependence() {
    use game_parallel::{GoodsHolding, Person};
    let mut initial = world();
    let count = 2_000u64;
    initial.regions[0].people = (100..100 + count * 2)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    initial.goods_cargo.as_mut().unwrap().holdings = (0..count * 2)
        .map(|i| GoodsHolding {
            person: 100 + i,
            items: vec![Amount {
                good: (i % 2 + 1) as u32,
                quantity: 1,
            }]
            .into(),
        })
        .collect();
    initial.market.as_mut().unwrap().policies = (0..count)
        .map(|i| policy(101 + 2 * i, &[(1, 1), (2, 1)]))
        .collect();
    let offers: Vec<_> = (0..count)
        .map(|i| TradeProposal {
            proposer: 100 + 2 * i,
            recipient: 101 + 2 * i,
            give: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            receive: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        })
        .collect();
    initial.propose_trades(&offers).unwrap();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if threads == 4 {
            w.regions[0].people.reverse();
            w.market.as_mut().unwrap().offers.reverse();
            w.market.as_mut().unwrap().policies.reverse();
            w.goods_cargo.as_mut().unwrap().holdings.reverse();
        }
        pool.install(|| w.step()).unwrap();
        w.regions[0].people.sort_unstable_by_key(|p| p.id);
        w.market
            .as_mut()
            .unwrap()
            .policies
            .sort_unstable_by_key(|p| p.person);
        assert!(w.market.as_ref().unwrap().offers.is_empty());
        assert_eq!(w.market.as_ref().unwrap().events.len(), count as usize);
        w.validate().unwrap();
        results.push(w);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn accepted_trade_takes_priority_over_new_automatic_work() {
    let mut w = world();
    w.autowork = World::from_toml(include_str!("../../../../content/parallel/autowork.toml"))
        .unwrap()
        .autowork;
    w.market
        .as_mut()
        .unwrap()
        .policies
        .push(policy(2, &[(1, 2), (2, 1)]));
    w.propose_trade(proposal()).unwrap();
    let before = quantity_stock(&w);
    w.step().unwrap();
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(w.autowork.as_ref().unwrap().next_id, 100);
    assert_eq!(quantity_stock(&w), before);
    assert_eq!(
        w.market.as_ref().unwrap().events[0].outcome,
        TradeOutcome::Accepted
    );
}
fn quantity_stock(w: &World) -> u64 {
    w.production.as_ref().unwrap().inventories[0].stocks[0].quantity
}

#[test]
fn people_propose_a_swap_using_their_own_values_then_the_recipient_decides_next_tick() {
    let mut w = world();
    let mut a = policy(1, &[(1, 1), (2, 3)]);
    a.propose = true;
    let b = policy(2, &[(1, 2), (2, 1)]);
    w.market.as_mut().unwrap().policies = vec![a, b];
    let before = w.goods_cargo.clone();
    w.step().unwrap();
    let market = w.market.as_ref().unwrap();
    assert_eq!(market.offers.len(), 1);
    assert_eq!(
        (market.offers[0].proposer, market.offers[0].recipient),
        (1, 2)
    );
    assert_eq!(market.offers[0].proposed_at, 1);
    assert_eq!(w.goods_cargo, before);
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
    assert_eq!(
        w.market.as_ref().unwrap().events[0].outcome,
        TradeOutcome::Accepted
    );
    assert_eq!(w.goods_cargo.as_ref().unwrap().holdings[0].items[1].good, 2);
}
#[test]
fn proactive_offers_do_not_assume_recipient_consent_or_repeat_while_pending() {
    let mut w = world();
    let mut a = policy(1, &[(1, 1), (2, 3)]);
    a.propose = true;
    let mut b = policy(2, &[(1, 2), (2, 1)]);
    b.enabled = false;
    w.market.as_mut().unwrap().policies = vec![a, b];
    let before = w.goods_cargo.clone();
    w.step().unwrap();
    w.step().unwrap();
    w.step().unwrap();
    assert_eq!(w.market.as_ref().unwrap().offers.len(), 1);
    assert_eq!(w.goods_cargo, before);
    w.decide_trade(1, 2, TradeOutcome::Declined).unwrap();
    assert!(w.market.as_ref().unwrap().offers.is_empty());
}
#[test]
fn failed_family_tick_does_not_create_an_autonomous_offer() {
    let mut w = world();
    let mut a = policy(1, &[(1, 1), (2, 3)]);
    a.propose = true;
    w.market.as_mut().unwrap().policies = vec![a, policy(2, &[(1, 2), (2, 1)])];
    let family =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.regions = family.regions;
    w.routes = family.routes;
    w.families = family.families;
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 3)
        .unwrap()
        .age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

use super::*;
use game_parallel_agreements::Policy;
use std::collections::HashSet;

fn contract(id: u64, debtor: u64, creditor: u64, quantity: u64) -> Contract {
    Contract {
        id,
        debtor,
        creditor,
        good: 1,
        quantity,
        delivered: 0,
        due: 10,
        status: Status::Active,
    }
}
fn amount(good: u32, quantity: u64) -> Amount {
    Amount { good, quantity }
}
fn fixture(count: u64, funds: u64) -> (Agreements, GoodsCargo, Vec<PersonSnapshot>) {
    let state = Agreements {
        offering: vec![],
        next_id: count + 1,
        policies: vec![Policy {
            person: 1,
            honor: true,
            accept: false,
            minimum_trust: None,
            minimum_honor_trust: None,
            fear_honor_threshold: None,
            obligation_honor_threshold: None,
            accept_pressure_threshold: None,
        }],
        bulk_debtors: vec![1],
        contracts: (1..=count).map(|id| contract(id, 1, id + 1, 1)).collect(),
        events: vec![],
    };
    let cargo = GoodsCargo {
        capacity: count * 2 + 1,
        holdings: vec![GoodsHolding {
            person: 1,
            items: [amount(1, funds)].into_iter().collect(),
        }],
    };
    let people: Vec<_> = (1..=count + 1)
        .map(|person| (person, Some(1), 100, 0))
        .collect();
    let identities: HashSet<_> = people.iter().map(|row| row.0).collect();
    state.validate(&identities, &HashSet::from([1]), 0).unwrap();
    (state, cargo, people)
}
#[test]
fn bulk_fulfillment_reserves_real_stock_and_each_recipient_once() {
    let (mut state, mut cargo, people) = fixture(4, 5);
    for contract in &mut state.contracts {
        contract.quantity = 2;
    }
    state.contracts[3].creditor = 2;
    cargo.capacity = 10;
    cargo.holdings.push(GoodsHolding {
        person: 4,
        items: [amount(2, 10)].into_iter().collect(),
    });
    let draft = prepare(&state, &cargo, &people, &[3], 1, None).unwrap();
    assert_eq!(draft.actors, vec![1, 2]);
    assert_eq!(
        draft.updates.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(draft.events[0].quantity, 2);
    assert_eq!(draft.holdings[0].1.as_ref().unwrap().items[0].quantity, 3);
    assert_eq!(draft.holdings[1].1.as_ref().unwrap().items[0].quantity, 2);
}
#[test]
fn concentrated_scarcity_crosses_chunks_and_is_worker_invariant() {
    let (state, cargo, people) = fixture(20_000, 10_000);
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let draft = prepare(&state, &cargo, &people, &[], 1, None).unwrap();
                assert_eq!(draft.updates.len(), 10_000);
                assert_eq!(draft.actors.len(), 10_001);
                assert_eq!(draft.updates.first().unwrap().id, 1);
                assert_eq!(draft.updates.last().unwrap().id, 10_000);
                assert!(draft.holdings[0].1.is_none());
                (draft.actors, draft.updates, draft.events, draft.holdings)
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn different_goods_are_debited_once_and_inactive_claims_are_ignored() {
    let (mut state, mut cargo, people) = fixture(4, 4);
    state.contracts[1].good = 2;
    state.contracts[2].status = Status::Proposed;
    state.contracts[3].due = 0;
    cargo.holdings[0].items.push(amount(2, 3));
    let draft = prepare(&state, &cargo, &people, &[], 1, None).unwrap();
    assert_eq!(
        draft.updates.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    let payer = draft.holdings[0].1.as_ref().unwrap();
    assert_eq!(
        payer
            .items
            .iter()
            .find(|item| item.good == 1)
            .unwrap()
            .quantity,
        3
    );
    assert_eq!(
        payer
            .items
            .iter()
            .find(|item| item.good == 2)
            .unwrap()
            .quantity,
        2
    );
}

#[test]
fn overlapping_payers_never_receive_a_simultaneous_bulk_patch() {
    let (mut state, mut cargo, people) = fixture(2, 2);
    state.policies.push(Policy {
        person: 2,
        honor: true,
        accept: false,
        minimum_trust: None,
        minimum_honor_trust: None,
        fear_honor_threshold: None,
        obligation_honor_threshold: None,
        accept_pressure_threshold: None,
    });
    state.bulk_debtors.push(2);
    state.contracts[1].debtor = 2;
    cargo.holdings.push(GoodsHolding {
        person: 2,
        items: [amount(1, 1)].into_iter().collect(),
    });
    let draft = prepare(&state, &cargo, &people, &[], 1, None).unwrap();
    assert_eq!(
        draft.updates.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(draft.actors, vec![2, 3]);
}

#[test]
#[ignore = "manual concentrated bulk allocation benchmark"]
fn benchmark_concentrated_bulk_allocation() {
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let count: u64 = std::env::var("LEGACY_CLAIMS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let (state, cargo, people) = fixture(count, count / 2);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap();
    let start = std::time::Instant::now();
    let draft = pool.install(|| prepare(&state, &cargo, &people, &[], 1, None).unwrap());
    let elapsed = start.elapsed();
    assert_eq!(draft.updates.len(), (count / 2) as usize);
    eprintln!(
        "bulk allocation: {workers} workers, {count} claims, {} transfers: {elapsed:?}",
        count / 2
    );
}

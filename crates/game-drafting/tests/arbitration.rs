use game_drafting::arbitration::{Decision, Demand, Error, Intent, ResourceKey, arbitrate};
use std::collections::BTreeMap;

fn key(owner: u64, item: u64) -> ResourceKey {
    ResourceKey {
        domain: 1,
        owner,
        item,
    }
}
fn intent(id: u64, actor: u64, priority: u8, claims: &[(ResourceKey, u64)]) -> Intent {
    Intent {
        id,
        actor,
        participants: vec![],
        priority,
        demands: claims
            .iter()
            .map(|(resource, quantity)| Demand {
                resource: *resource,
                quantity: *quantity,
            })
            .collect(),
    }
}
fn pool(workers: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
}

#[test]
fn contested_resources_rotate_fairly_and_urgent_claims_go_first() {
    let stock = key(1, 1);
    let available = BTreeMap::from([(stock, 1)]);
    let claims = vec![
        intent(10, 10, 1, &[(stock, 1)]),
        intent(20, 20, 1, &[(stock, 1)]),
        intent(30, 30, 1, &[(stock, 1)]),
    ];
    for (round, winner) in [(0, 10), (1, 20), (2, 30), (3, 10)] {
        let result = arbitrate(&pool(4), &available, &claims, round).unwrap();
        assert_eq!(result.decisions[&winner], Decision::Granted);
        assert_eq!(result.granted[&stock], 1);
    }
    let mut urgent = claims;
    urgent[2].priority = 0;
    for round in 0..3 {
        assert_eq!(
            arbitrate(&pool(4), &available, &urgent, round)
                .unwrap()
                .decisions[&30],
            Decision::Granted
        );
    }
}

#[test]
fn lower_priority_actors_do_not_distort_rotation_of_urgent_claims() {
    let stock = key(1, 1);
    let available = BTreeMap::from([(stock, 1)]);
    let claims = vec![
        intent(1, 1, 0, &[(stock, 1)]),
        intent(2, 2, 1, &[(stock, 1)]),
        intent(3, 3, 1, &[(stock, 1)]),
        intent(4, 4, 0, &[(stock, 1)]),
    ];
    for (round, winner) in [(0, 1), (1, 4), (2, 1), (3, 4)] {
        assert_eq!(
            arbitrate(&pool(4), &available, &claims, round)
                .unwrap()
                .decisions[&winner],
            Decision::Granted
        );
    }
}

#[test]
fn explicit_precedence_is_worker_independent_and_never_overrides_priority() {
    use game_drafting::arbitration::arbitrate_ordered;
    let stock = key(1, 1);
    let available = BTreeMap::from([(stock, 1)]);
    let claims = vec![
        intent(1, 1, 1, &[(stock, 1)]),
        intent(2, 2, 1, &[(stock, 1)]),
        intent(3, 3, 1, &[(stock, 1)]),
    ];
    let mut reversed = claims.clone();
    reversed.reverse();
    let reference = arbitrate_ordered(&pool(1), &available, &claims, &[3, 1, 2]).unwrap();
    assert_eq!(reference.decisions[&3], Decision::Granted);
    assert_eq!(
        reference,
        arbitrate_ordered(&pool(12), &available, &reversed, &[3, 1, 2]).unwrap()
    );
    let mut urgent = claims.clone();
    urgent[0].priority = 0;
    assert_eq!(
        arbitrate_ordered(&pool(4), &available, &urgent, &[3, 1, 2])
            .unwrap()
            .decisions[&1],
        Decision::Granted
    );
    assert_eq!(
        arbitrate_ordered(&pool(1), &available, &claims, &[3, 1]).unwrap_err(),
        Error::MissingPrecedence(2)
    );
    assert_eq!(
        arbitrate_ordered(&pool(1), &available, &claims, &[3, 1, 1, 2]).unwrap_err(),
        Error::DuplicatePrecedence(1)
    );
}

#[test]
fn claims_are_atomic_and_actor_conflicts_cross_resource_groups() {
    let a = key(1, 1);
    let b = key(2, 1);
    let available = BTreeMap::from([(a, 2), (b, 1)]);
    let claims = vec![
        intent(1, 1, 0, &[(a, 1), (b, 2)]),
        intent(2, 2, 1, &[(a, 2)]),
        intent(3, 2, 1, &[(b, 1)]),
    ];
    let result = arbitrate(&pool(4), &available, &claims, 0).unwrap();
    assert_eq!(result.decisions[&1], Decision::Insufficient(b));
    assert_eq!(result.decisions[&2], Decision::Granted);
    assert_eq!(result.decisions[&3], Decision::ActorAlreadyGranted);
    assert_eq!(result.granted, BTreeMap::from([(a, 2)]));
}

#[test]
fn joint_participants_join_conflict_groups_and_are_reserved_atomically() {
    let stock = key(1, 1);
    let unavailable = key(2, 1);
    let available = BTreeMap::from([(stock, 2)]);
    let mut failed = intent(1, 1, 0, &[(stock, 1), (unavailable, 1)]);
    failed.participants = vec![2];
    let mut joint = intent(2, 3, 1, &[(stock, 1)]);
    joint.participants = vec![2, 3, 2];
    let participant_action = intent(3, 2, 1, &[]);
    let mut target_actor = intent(4, 4, 1, &[]);
    target_actor.participants = vec![3];
    let separate = intent(5, 5, 1, &[(stock, 1)]);
    let claims = vec![failed, joint, participant_action, target_actor, separate];
    use game_drafting::arbitration::arbitrate_ordered;
    let precedence = &[1, 3, 2, 4, 5];
    let result = arbitrate_ordered(&pool(1), &available, &claims, precedence).unwrap();
    assert_eq!(result.decisions[&1], Decision::Insufficient(unavailable));
    assert_eq!(result.decisions[&2], Decision::Granted);
    assert_eq!(result.decisions[&3], Decision::ParticipantAlreadyGranted(2));
    assert_eq!(result.decisions[&4], Decision::ParticipantAlreadyGranted(3));
    assert_eq!(result.decisions[&5], Decision::Granted);
    assert_eq!(result.granted, BTreeMap::from([(stock, 2)]));
    let mut reversed = claims.clone();
    reversed.reverse();
    for workers in [1, 4, 12] {
        assert_eq!(
            result,
            arbitrate_ordered(&pool(workers), &available, &reversed, precedence).unwrap()
        );
    }
}

#[test]
fn joint_priority_and_rotation_apply_without_stock_claims() {
    let mut first = intent(1, 1, 1, &[]);
    first.participants = vec![9];
    let mut second = intent(2, 2, 1, &[]);
    second.participants = vec![9];
    let claims = vec![first, second];
    for (round, winner) in [(0, 1), (1, 2), (2, 1)] {
        let result = arbitrate(&pool(4), &BTreeMap::new(), &claims, round).unwrap();
        assert_eq!(result.decisions[&winner], Decision::Granted);
    }
    let mut urgent = claims;
    urgent[1].priority = 0;
    for round in 0..3 {
        let result = arbitrate(&pool(4), &BTreeMap::new(), &urgent, round).unwrap();
        assert_eq!(result.decisions[&2], Decision::Granted);
        assert_eq!(result.decisions[&1], Decision::ParticipantAlreadyGranted(9));
    }
}

#[test]
fn decisions_are_independent_of_input_and_worker_order() {
    let a = key(1, 1);
    let b = key(2, 1);
    let c = key(3, 1);
    let available = BTreeMap::from([(a, 3), (b, 2), (c, 1)]);
    let claims = vec![
        intent(1, 1, 1, &[(a, 1), (b, 1)]),
        intent(2, 2, 1, &[(b, 2)]),
        intent(3, 3, 1, &[(c, 1)]),
        intent(4, 4, 1, &[(c, 1)]),
        intent(5, 5, 1, &[(a, 1), (a, 1)]),
    ];
    let reference = arbitrate(&pool(1), &available, &claims, 17).unwrap();
    let mut reversed = claims.clone();
    reversed.reverse();
    for workers in [1, 4, 12] {
        assert_eq!(
            reference,
            arbitrate(&pool(workers), &available, &reversed, 17).unwrap()
        );
    }
}

#[test]
fn malformed_inputs_reject_without_provisional_grants() {
    let stock = key(1, 1);
    let available = BTreeMap::from([(stock, u64::MAX)]);
    let duplicate = vec![intent(1, 1, 0, &[]), intent(1, 2, 0, &[])];
    assert_eq!(
        arbitrate(&pool(1), &available, &duplicate, 0),
        Err(Error::DuplicateIntent(1))
    );
    assert_eq!(
        arbitrate(&pool(1), &available, &[intent(2, 1, 0, &[(stock, 0)])], 0),
        Err(Error::ZeroQuantity(2))
    );
    assert_eq!(
        arbitrate(
            &pool(1),
            &available,
            &[intent(3, 1, 0, &[(stock, u64::MAX), (stock, 1)])],
            0
        ),
        Err(Error::DemandOverflow(3))
    );
}

use super::*;
use game_parallel::{
    CivicAuto, CivicPolicy as Policy, InstitutionBody as Body,
    InstitutionMembershipRequest as MembershipRequest, InstitutionRecordedVote as Vote,
};
use game_parallel_block_institutions::membership::publish;

#[test]
fn concentrated_applications_use_disjoint_quorums_in_parallel() {
    let policies = CivicAuto {
        founding: None,
        policies: (1..=10_000)
            .map(|person| Policy {
                person,
                enabled: true,
                prefer_open: None,
                trade_trust_target: None,
                trade_trust_threshold: 0,
                minimum_candidate_trust: Some(0),
                propose_trade: false,
                proposal_duration: 1,
                leave_on_trade_disagreement: false,
                seek_membership: false,
            })
            .collect(),
    };
    let people: Vec<_> = (1..=15_000)
        .map(|person| (person, Some(1), 100, 0))
        .collect();
    let institutions = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: (1..=10_000).collect(),
            quorum: 2,
            chair: Some(1),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        admissions: vec![],
        requests: (10_001..=15_000)
            .map(|applicant| MembershipRequest {
                body: 1,
                applicant,
                requested_at: 0,
                votes: vec![],
            })
            .collect(),
    };
    let busy: Vec<_> = (1..=2_000).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut result = institutions.clone();
                let draft = policies
                    .admissions(&result, None, &people, &busy, 1)
                    .unwrap();
                assert_eq!(draft.actors.len(), 8_000);
                let mut actors = draft.actors.clone();
                actors.sort_unstable();
                actors.dedup();
                assert_eq!(actors.len(), 8_000);
                publish(&mut result, draft.into_decisions());
                result
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.admissions.len(), 4_000);
    assert!(one.admissions.iter().all(|event| event.accepted));
    assert_eq!(one.requests.len(), 1_000);
    assert_eq!(one.bodies[0].members.len(), 14_000);
}

#[test]
#[ignore = "manual scaling benchmark"]
fn membership_scaling() {
    let workers: usize = std::env::var("LEGACY_WORKERS")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    let count: u64 = std::env::var("LEGACY_MEMBERS")
        .unwrap_or_else(|_| "200000".into())
        .parse()
        .unwrap();
    assert!(count >= 2 && count.is_multiple_of(2));
    let policies = CivicAuto {
        founding: None,
        policies: (1..=count)
            .map(|person| Policy {
                person,
                enabled: true,
                prefer_open: None,
                trade_trust_target: None,
                trade_trust_threshold: 0,
                minimum_candidate_trust: Some(0),
                propose_trade: false,
                proposal_duration: 1,
                leave_on_trade_disagreement: false,
                seek_membership: false,
            })
            .collect(),
    };
    let people: Vec<_> = (1..=count + count / 2)
        .map(|person| (person, Some(1), 100, 0))
        .collect();
    let mut institutions = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: (1..=count).collect(),
            quorum: 2,
            chair: Some(1),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        admissions: vec![],
        requests: (count + 1..=count + count / 2)
            .map(|applicant| MembershipRequest {
                body: 1,
                applicant,
                requested_at: 0,
                votes: vec![],
            })
            .collect(),
    };
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let started = std::time::Instant::now();
        let draft = policies.admissions(&institutions, None, &people, &[], 1).unwrap();
        let prepared = started.elapsed();
        publish(&mut institutions, draft.into_decisions());
        let committed = started.elapsed();
        assert_eq!(institutions.admissions.len(), (count / 2) as usize);
        println!("workers={workers} members={count} applications={} prepare={prepared:?} total={committed:?}", count / 2);
    });
}

#[test]
fn previously_recorded_votes_are_counted_once_and_do_not_reserve_the_voter_again() {
    let policies = CivicAuto {
        founding: None,
        policies: (1..=2)
            .map(|person| Policy {
                person,
                enabled: true,
                prefer_open: None,
                trade_trust_target: None,
                trade_trust_threshold: 0,
                minimum_candidate_trust: Some(0),
                propose_trade: false,
                proposal_duration: 1,
                leave_on_trade_disagreement: false,
                seek_membership: false,
            })
            .collect(),
    };
    let people = vec![
        (1, Some(1), 100, 0),
        (2, Some(1), 100, 0),
        (3, Some(1), 100, 0),
    ];
    let mut institutions = Institutions {
        next_id: 1,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: vec![1, 2],
            quorum: 2,
            chair: Some(1),
            trade_open: true,
        }],
        ballots: vec![],
        events: vec![],
        admissions: vec![],
        requests: vec![MembershipRequest {
            body: 1,
            applicant: 3,
            requested_at: 0,
            votes: vec![Vote {
                voter: 1,
                yes: false,
            }],
        }],
    };
    let draft = policies
        .admissions(&institutions, None, &people, &[], 1)
        .unwrap();
    assert_eq!(draft.actors, vec![2]);
    publish(&mut institutions, draft.into_decisions());
    assert!(!institutions.admissions[0].accepted);
    assert_eq!(institutions.bodies[0].members, vec![1, 2]);
}

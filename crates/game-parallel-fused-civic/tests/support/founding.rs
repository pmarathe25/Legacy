use super::*;
use game_parallel::{CivicFoundingRules as FoundingRules, CivicPolicy as Policy};

#[test]
fn concentrated_founders_are_deterministic_and_do_not_enroll_busy_people() {
    let policy = CivicAuto {
        founding: Some(FoundingRules {
            minimum_founders: 2,
            quorum: 2,
        }),
        policies: (1..=10_000)
            .map(|person| Policy {
                person,
                enabled: true,
                prefer_open: Some(person % 2 == 0),
                trade_trust_target: None,
                trade_trust_threshold: 0,
                minimum_candidate_trust: None,
                propose_trade: true,
                proposal_duration: 1,
                leave_on_trade_disagreement: false,
                seek_membership: false,
            })
            .collect(),
    };
    let people: Vec<_> = (1..=10_000)
        .map(|person| (person, Some(7), 100, 0))
        .collect();
    let institutions = Institutions {
        next_id: 1,
        bodies: Vec::new(),
        ballots: Vec::new(),
        events: Vec::new(),
        requests: Vec::new(),
        admissions: Vec::new(),
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| prepare(&policy, &institutions, None, &people, &[2]).unwrap())
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one[0].members.len(), 9_999);
    assert!(!one[0].members.contains(&2));
    assert!(!one[0].trade_open);
    assert_eq!(one[0].chair, Some(1));
    let mut formed = institutions;
    formed.bodies = one;
    assert!(
        prepare(&policy, &formed, None, &people, &[])
            .unwrap()
            .is_empty()
    );
}

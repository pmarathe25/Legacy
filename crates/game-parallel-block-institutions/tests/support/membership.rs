use super::*;
use game_parallel::{InstitutionBody, InstitutionMembershipRequest, InstitutionRecordedVote};
use std::collections::BTreeSet;

#[test]
fn unordered_sparse_decisions_merge_across_chunks_without_losing_requests_or_votes() {
    let body = |id, members| InstitutionBody {
        id,
        region: id,
        members,
        quorum: 1,
        chair: None,
        trade_open: true,
    };
    let requests: Vec<_> = (100..12_100)
        .rev()
        .map(|applicant| {
            let body = 1 + applicant % 2;
            InstitutionMembershipRequest {
                body,
                applicant,
                requested_at: applicant % 5,
                votes: vec![InstitutionRecordedVote {
                    voter: if body == 1 { 1 } else { 3 },
                    yes: true,
                }],
            }
        })
        .collect();
    let decisions: Vec<_> = requests
        .iter()
        .filter(|request| request.applicant % 3 == 0)
        .map(|request| InstitutionAdmissionEvent {
            tick: 1,
            body: request.body,
            applicant: request.applicant,
            accepted: request.applicant % 6 == 0,
            yes: 0,
            no: 0,
        })
        .collect();
    let expected: BTreeSet<_> = requests
        .iter()
        .filter(|request| request.applicant % 3 != 0)
        .map(|request| (request.body, request.applicant))
        .collect();
    let initial = Institutions {
        next_id: 1,
        bodies: vec![body(1, vec![1, 2]), body(2, vec![3, 4])],
        ballots: vec![],
        events: vec![],
        requests,
        admissions: vec![],
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut result = initial.clone();
                publish(&mut result, decisions.clone());
                result
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(
        result
            .requests
            .iter()
            .map(|request| (request.body, request.applicant))
            .collect::<BTreeSet<_>>(),
        expected
    );
    assert!(
        result
            .requests
            .iter()
            .all(|request| request.votes.len() == 1 && request.votes[0].yes)
    );
    assert_eq!(result.admissions.len(), decisions.len());
}

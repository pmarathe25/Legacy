use super::*;
use game_parallel::{InstitutionBody, InstitutionMembershipRequest};

#[test]
fn concentrated_deaths_remove_only_affected_members_and_applications() {
    let initial = Some(Institutions {
        next_id: 1,
        bodies: vec![InstitutionBody {
            id: 1,
            region: 1,
            members: (1..=20_000).collect(),
            quorum: 10_000,
            chair: Some(1),
            trade_open: false,
        }],
        ballots: vec![],
        events: vec![],
        admissions: vec![],
        requests: (20_001..=30_000)
            .map(|applicant| InstitutionMembershipRequest {
                body: 1,
                applicant,
                requested_at: 0,
                votes: vec![],
            })
            .collect(),
    });
    let deaths: Vec<_> = (1..=10_000).chain(20_001..=25_000).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut result = initial.clone();
                remove_dead(&mut result, &deaths, 1);
                result.unwrap()
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.bodies[0].members, (10_001..=20_000).collect::<Vec<_>>());
    assert_eq!(one.bodies[0].chair, None);
    assert_eq!(one.requests.len(), 5_000);
    assert_eq!(one.admissions.len(), 5_000);
    assert!(one.admissions.iter().all(|event| !event.accepted));
}

#[test]
fn departures_affect_only_the_selected_institution_across_workers() {
    let body = |id, region| InstitutionBody {
        id,
        region,
        members: (1..=10_000).collect(),
        quorum: 2,
        chair: Some(1),
        trade_open: false,
    };
    let initial = Institutions {
        next_id: 1,
        bodies: vec![body(1, 1), body(2, 2)],
        ballots: vec![],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    let departures: Vec<_> = (1..=5_000).map(|person| (1, person)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut result = initial.clone();
                remove_departures(&mut result, &departures, 1);
                result
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.bodies[0].members, (5_001..=10_000).collect::<Vec<_>>());
    assert_eq!(one.bodies[0].chair, None);
    assert_eq!(one.bodies[1].members.len(), 10_000);
    assert_eq!(one.bodies[1].chair, Some(1));
}

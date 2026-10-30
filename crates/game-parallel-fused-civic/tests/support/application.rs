use super::*;
use game_parallel::{CivicPolicy, World};

#[test]
fn concentrated_applications_are_parallel_and_ignore_members_and_busy_people() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/civic.toml")).unwrap();
    world.civic_auto = World::from_toml(include_str!("../../../../content/parallel/founding.toml"))
        .unwrap()
        .civic_auto;
    let institutions = world.institutions.as_mut().unwrap();
    institutions.bodies.truncate(1);
    institutions.bodies[0].members = (1..=10_000).collect();
    institutions.bodies[0].quorum = 2;
    institutions.bodies[0].trade_open = true;
    world.civic_auto.as_mut().unwrap().policies = (1..=20_000)
        .map(|person| CivicPolicy {
            person,
            enabled: true,
            prefer_open: Some(true),
            trade_trust_target: None,
            trade_trust_threshold: 0,
            minimum_candidate_trust: Some(0),
            propose_trade: false,
            proposal_duration: 1,
            leave_on_trade_disagreement: false,
            seek_membership: true,
        })
        .collect();
    let people: Vec<_> = (1..=20_000)
        .map(|person| (person, Some(1), 100, 0))
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| prepare(&world, &people, &[10_001], 1))
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.len(), 9_999);
    assert_eq!(one[0].applicant, 10_002);
    assert_eq!(one.last().unwrap().applicant, 20_000);
}

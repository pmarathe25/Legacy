use super::*;
use game_parallel_institutions::{Ballot, Body, Vote};
use game_parallel_opinions::{Opinion, Rules};
fn fixture(action: Action) -> (CivicAuto, Institutions, Vec<PersonSnapshot>) {
    let owner = CivicAuto {
        founding: None,
        policies: (1..=3)
            .map(|person| Policy {
                person,
                enabled: true,
                prefer_open: Some(true),
                trade_trust_target: None,
                trade_trust_threshold: 0,
                minimum_candidate_trust: Some(0),
                propose_trade: false,
                proposal_duration: 3,
                leave_on_trade_disagreement: false,
                seek_membership: false,
            })
            .collect(),
    };
    let institutions = Institutions {
        next_id: 2,
        bodies: vec![Body {
            id: 1,
            region: 1,
            members: vec![1, 2, 3],
            quorum: 2,
            chair: Some(1),
            trade_open: false,
        }],
        ballots: vec![Ballot {
            id: 1,
            body: 1,
            proposer: 1,
            action,
            closes_at: 2,
            votes: vec![Vote {
                voter: 1,
                yes: false,
            }],
        }],
        events: vec![],
        requests: vec![],
        admissions: vec![],
    };
    (
        owner,
        institutions,
        vec![
            (1, Some(1), 100, 0),
            (2, Some(1), 100, 0),
            (3, Some(1), 100, 0),
        ],
    )
}
#[test]
fn people_cast_preferences_without_overwriting_votes_or_busy_actors() {
    let (owner, institutions, people) = fixture(Action::Trade { open: true });
    let draft = owner.prepare(&institutions, None, &people, &[3]).unwrap();
    assert_eq!(draft.actors, vec![2]);
    assert_eq!(
        draft.votes,
        vec![Cast {
            ballot: 1,
            voter: 2,
            yes: true
        }]
    );
    let mut staged = institutions.clone();
    staged.vote(&people, &draft.votes).unwrap();
    staged.prepare(&people, 2).commit(&mut staged);
    assert!(!staged.bodies[0].trade_open); // a tie cannot enact the proposal
}
#[test]
fn directed_distrust_affects_only_the_observers_election_vote() {
    let (owner, institutions, people) = fixture(Action::Elect { candidate: 1 });
    let opinions = Opinions {
        sensitivities: Vec::new(),
        rules: Rules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 1,
            default_loss: 1,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        },
        records: vec![Opinion {
            observer: 2,
            target: 1,
            trust: -2,
            fear: 0,
            obligation: 0,
            affinity: 0,
        }],
        events: vec![],
    };
    let draft = owner
        .prepare(&institutions, Some(&opinions), &people, &[])
        .unwrap();
    assert_eq!(
        draft.votes,
        vec![
            Cast {
                ballot: 1,
                voter: 2,
                yes: false
            },
            Cast {
                ballot: 1,
                voter: 3,
                yes: true
            }
        ]
    );
}
#[test]
fn voluntary_local_proposal_uses_available_member_and_retries_after_contention() {
    let (mut owner, mut institutions, people) = fixture(Action::Trade { open: true });
    institutions.ballots.clear();
    for policy in &mut owner.policies {
        policy.propose_trade = true;
        policy.proposal_duration = 2;
    }
    let proposal = owner.prepare(&institutions, None, &people, &[1]).unwrap();
    assert_eq!(proposal.votes.len(), 0);
    assert_eq!(
        proposal.proposals,
        vec![Proposal {
            body: 1,
            proposer: 2,
            open: true,
            duration: 2
        }]
    );
    assert_eq!(proposal.actors, vec![2]);
}

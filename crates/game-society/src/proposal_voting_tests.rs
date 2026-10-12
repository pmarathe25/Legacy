use crate::institutions::*;
use crate::policy_fixtures::{Knowledge, refs, seed_institution, voter, world};
use crate::proposal_voting::*;
use crate::storage::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};
#[test]
fn native_proposal_replies_preserve_order_locality_and_zero_threshold() {
    let (runtime, mut store) = world();
    let mut row = crate::policy_fixtures::institution(10, vec![], InstitutionStatus::Active);
    row.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    seed_institution(&mut store, row);
    for id in [32, 31] {
        insert_proposal(
            &mut store,
            RuleProposalId(id),
            RuleProposal {
                id: RuleProposalId(id),
                institution: InstitutionId(10),
                proposer: PersonId(2),
                change: InstitutionChange::Procedure(
                    crate::policy_fixtures::charter(vec![]).procedure,
                ),
                votes: BTreeMap::new(),
                base_revision: 0,
                status: RuleProposalStatus::Proposed,
            },
        )
        .unwrap();
    }
    let known = Knowledge::default();
    let v = voter(&known);
    let authority = AuthorityFacts {
        alive: refs().people,
    };
    let proposers = [ProposerFacts {
        actor: PersonId(2),
        area: Some(AreaId(3)),
        social_penalty: 1,
    }];
    let replies = runtime
        .proposal_replies(&store, &authority, &[v], &proposers, &[])
        .unwrap();
    assert_eq!(
        replies,
        vec![
            (
                PersonId(1),
                InstitutionAction::Vote {
                    proposal: RuleProposalId(31),
                    support: false
                }
            ),
            (
                PersonId(1),
                InstitutionAction::Vote {
                    proposal: RuleProposalId(32),
                    support: false
                }
            )
        ]
    );
    let remote = [ProposerFacts {
        area: Some(AreaId(4)),
        ..proposers[0]
    }];
    assert!(
        runtime
            .proposal_replies(&store, &authority, &[v], &remote, &[])
            .unwrap()
            .iter()
            .all(|(_, a)| matches!(a, InstitutionAction::Vote { support: true, .. }))
    );
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .proposal_replies(&store, &authority, &[v, v], &remote, &[])
            .is_err()
    );
    assert_eq!(before, store.snapshot().unwrap());
}

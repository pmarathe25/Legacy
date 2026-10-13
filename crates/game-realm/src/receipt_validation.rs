//! Saved receipt references checked against Realm's canonical rows.
use crate::{TermKey, commitment};
use game_core::{Error, RealmCommitmentId};
use game_ecs::Store;

fn require(valid: bool) -> Result<(), Error> {
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

/// Proposed and ratified receipts retain a reference to an existing revision.
pub fn validate_revision(store: &Store, id: RealmCommitmentId, revision: u32) -> Result<(), Error> {
    require(commitment(store, id).is_some_and(|row| row.revision(revision).is_some()))
}

/// Work receipts identify a term in an existing revision.
pub fn validate_work_request(store: &Store, key: TermKey) -> Result<(), Error> {
    require(
        commitment(store, key.commitment)
            .and_then(|row| row.revision(key.revision))
            .is_some_and(|row| usize::from(key.index) < row.proposal.terms.len()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Commitment, Proposal, Revision, Term};
    use game_core::{InstitutionId, PersonId, SimTime};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn receipts_require_the_exact_revision_and_term_bounds() {
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<Commitment>("realm/commitment", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema);
        crate::insert_commitment(
            &mut store,
            Commitment {
                id: RealmCommitmentId(1),
                revisions: vec![Revision {
                    number: 0,
                    proposal: Proposal {
                        proposer: PersonId(1),
                        participants: BTreeMap::new(),
                        terms: vec![Term::Passage {
                            grantor: InstitutionId(1),
                            beneficiary: InstitutionId(2),
                            areas: BTreeSet::new(),
                            until: SimTime(10),
                        }],
                        ratify_by: SimTime(10),
                    },
                    proposed_at: SimTime(0),
                    ratification_proposals: BTreeMap::new(),
                    ratifications: BTreeMap::new(),
                    requested: BTreeSet::new(),
                    backed: BTreeMap::new(),
                    pending: BTreeMap::new(),
                    expired_at: None,
                }],
            },
        )
        .unwrap();
        assert_eq!(validate_revision(&store, RealmCommitmentId(1), 0), Ok(()));
        assert!(validate_revision(&store, RealmCommitmentId(1), u32::MAX).is_err());
        assert!(validate_revision(&store, RealmCommitmentId(2), 0).is_err());
        let key = TermKey {
            commitment: RealmCommitmentId(1),
            revision: 0,
            index: 0,
        };
        assert_eq!(validate_work_request(&store, key), Ok(()));
        assert!(validate_work_request(&store, TermKey { index: 1, ..key }).is_err());
        assert!(validate_work_request(&store, TermKey { revision: 1, ..key }).is_err());
        assert!(
            validate_work_request(
                &store,
                TermKey {
                    commitment: RealmCommitmentId(2),
                    ..key
                }
            )
            .is_err()
        );
    }
}

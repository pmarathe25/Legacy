//! Agreement-owned consistency rules for persisted event receipts.
use crate::{agreement, amendment, obligation};
use game_core::{AgreementId, AmendmentId, Error, ObligationId, Quantity};
use game_ecs::Store;

fn invalid() -> Error {
    Error::InvalidSave("inconsistent committed state".into())
}

/// Amendment receipts must identify the agreement recorded by the amendment.
pub fn validate_amended_receipt(
    store: &Store,
    agreement_id: AgreementId,
    amendment_id: AmendmentId,
) -> Result<(), Error> {
    if agreement(store, agreement_id).is_none()
        || !amendment(store, amendment_id).is_some_and(|row| row.agreement == agreement_id)
    {
        return Err(invalid());
    }
    Ok(())
}

/// Foreign actor existence and runtime presence are facts supplied by the host.
pub fn validate_performed_receipt(
    store: &Store,
    runtime_present: bool,
    obligation_id: ObligationId,
    quantity: Quantity,
    actor_exists: bool,
) -> Result<(), Error> {
    if !actor_exists
        || quantity.0 == 0
        || !runtime_present
        || obligation(store, obligation_id).is_none()
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn fixture() -> Store {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let term = AgreementTerm::Labor {
            debtor: PersonId(1),
            creditor: PersonId(2),
            site: SiteId(3),
            millis: 10,
            deadline: SimTime(20),
        };
        for id in [AgreementId(10), AgreementId(11)] {
            insert_agreement(
                &mut store,
                id,
                Agreement {
                    id,
                    proposer: PersonId(1),
                    parties: BTreeSet::from([PersonId(1), PersonId(2)]),
                    accepted: BTreeSet::from([PersonId(1)]),
                    terms: vec![term.clone()],
                    procedure: None,
                    obligations: vec![],
                    status: AgreementStatus::Proposed,
                    proposed_at: SimTime(0),
                    revision: 0,
                },
            )
            .unwrap();
        }
        insert_amendment(
            &mut store,
            AmendmentId(12),
            Amendment {
                id: AmendmentId(12),
                agreement: AgreementId(10),
                proposer: PersonId(1),
                replacements: BTreeMap::from([(0, term.clone())]),
                votes: BTreeMap::new(),
                status: AmendmentStatus::Proposed,
                base_revision: 0,
            },
        )
        .unwrap();
        insert_obligation(
            &mut store,
            ObligationId(13),
            Obligation {
                id: ObligationId(13),
                agreement: AgreementId(10),
                term,
                performed: 0,
                accrued: 0,
                share_remainder: 0,
                share_scale: 0,
                defaulted: false,
                cargo: BTreeMap::new(),
            },
        )
        .unwrap();
        store
    }

    #[test]
    fn amendment_must_belong_to_the_receipted_agreement() {
        let store = fixture();
        validate_amended_receipt(&store, AgreementId(10), AmendmentId(12)).unwrap();
        assert!(matches!(
            validate_amended_receipt(&store, AgreementId(11), AmendmentId(12)),
            Err(Error::InvalidSave(_))
        ));
    }

    #[test]
    fn performance_requires_nonzero_quantity_and_available_references() {
        let store = fixture();
        validate_performed_receipt(&store, true, ObligationId(13), Quantity(1), true).unwrap();
        for (runtime, id, quantity, actor) in [
            (true, ObligationId(13), Quantity(0), true),
            (false, ObligationId(13), Quantity(1), true),
            (true, ObligationId(99), Quantity(1), true),
            (true, ObligationId(13), Quantity(1), false),
        ] {
            assert!(matches!(
                validate_performed_receipt(&store, runtime, id, quantity, actor),
                Err(Error::InvalidSave(_))
            ));
        }
    }
}

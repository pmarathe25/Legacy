//! Property-owned consistency rules shared by taking attempt and completion receipts.
use crate::incident;
use game_core::{Error, IncidentId, Quantity};
use game_ecs::Store;

/// The host supplies runtime presence and foreign component existence as facts.
pub fn validate_taking_receipt(
    store: &Store,
    runtime_present: bool,
    incident_id: IncidentId,
    quantity: Quantity,
    actor_exists: bool,
    source_exists: bool,
    destination_exists: bool,
) -> Result<(), Error> {
    if quantity.0 == 0
        || !actor_exists
        || !source_exists
        || !destination_exists
        || !runtime_present
        || incident(store, incident_id).is_none()
    {
        return Err(Error::InvalidSave("inconsistent committed state".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use game_core::*;
    use std::collections::BTreeMap;

    #[test]
    fn taking_requires_nonzero_quantity_and_available_references() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_incident(
            &mut store,
            IncidentId(30),
            Incident {
                id: IncidentId(30),
                actor: PersonId(1),
                source: InventoryId(10),
                destination: InventoryId(11),
                quantity: Quantity(5),
                area: AreaId(2),
                started_at: SimTime(0),
                completes: SimTime(10),
                changed_at: SimTime(10),
                outcome: IncidentOutcome::Taken,
                portions: BTreeMap::from([(ClaimId(0), 5)]),
            },
        )
        .unwrap();
        validate_taking_receipt(&store, true, IncidentId(30), Quantity(1), true, true, true)
            .unwrap();
        for (runtime, id, quantity, actor, source, destination) in [
            (true, IncidentId(30), Quantity(0), true, true, true),
            (false, IncidentId(30), Quantity(1), true, true, true),
            (true, IncidentId(99), Quantity(1), true, true, true),
            (true, IncidentId(30), Quantity(1), false, true, true),
            (true, IncidentId(30), Quantity(1), true, false, true),
            (true, IncidentId(30), Quantity(1), true, true, false),
        ] {
            assert!(matches!(
                validate_taking_receipt(&store, runtime, id, quantity, actor, source, destination),
                Err(Error::InvalidSave(_))
            ));
        }
    }
}

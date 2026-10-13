//! Persisted cargo row validation against foreign identity and deadline facts.
use crate::{InventoryLocation, inventories};
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;

pub fn validate_save_rows(
    store: &Store,
    people: &BTreeSet<PersonId>,
    areas: &BTreeSet<AreaId>,
    spoil_deadlines: &BTreeSet<InventoryId>,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent committed state".into());
    for (id, inventory) in inventories(store) {
        if id != inventory.id
            || inventory.quantity > inventory.capacity
            || inventory.access.iter().any(|p| !people.contains(p))
            || inventory.last_granted.is_some_and(|p| !people.contains(&p))
            || inventory.spoils_at.is_some() != spoil_deadlines.contains(&id)
        {
            return Err(fail());
        }
        match inventory.location {
            InventoryLocation::Area(a) if !areas.contains(&a) => return Err(fail()),
            InventoryLocation::Carried(p) if !people.contains(&p) => return Err(fail()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Inventory, insert_inventory, inventory_mut, register_components};
    use game_ecs::Schema;

    #[test]
    fn standalone_validation_checks_foreign_access_and_spoil_deadline() {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let id = InventoryId(1);
        insert_inventory(
            &mut store,
            Inventory {
                id,
                location: InventoryLocation::Area(AreaId(2)),
                quantity: Quantity(1),
                capacity: Quantity(2),
                access: BTreeSet::from([PersonId(3)]),
                last_granted: None,
                good: Good::Food,
                spoils_at: Some(SimTime(10)),
            },
        )
        .unwrap();
        let people = BTreeSet::from([PersonId(3)]);
        let areas = BTreeSet::from([AreaId(2)]);
        let deadlines = BTreeSet::from([id]);
        assert!(validate_save_rows(&store, &people, &areas, &deadlines).is_ok());
        assert!(validate_save_rows(&store, &people, &areas, &BTreeSet::new()).is_err());
        inventory_mut(&mut store, id).unwrap().last_granted = Some(PersonId(99));
        assert!(validate_save_rows(&store, &people, &areas, &deadlines).is_err());
    }
}

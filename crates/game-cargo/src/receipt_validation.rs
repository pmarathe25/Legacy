//! Persisted cargo receipts checked against identity and inventory facts.
//!
//! Callers resolve references; cargo owns the conjunction and quantity rules.
//! Historical receipts do not require current stock, permissions or capacity.
use game_core::{Error, Good, InventoryId, Quantity, SimTime};

fn require(valid: bool) -> Result<(), Error> {
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

pub fn validate_transfer(
    quantity: Quantity,
    actor_exists: bool,
    source_exists: bool,
    destination_exists: bool,
) -> Result<(), Error> {
    require(quantity.0 != 0 && actor_exists && source_exists && destination_exists)
}

pub fn validate_transfer_failed(actor_exists: bool, source_exists: bool) -> Result<(), Error> {
    require(actor_exists && source_exists)
}

/// Shared by consumed and eaten receipts.
pub fn validate_consumption(
    quantity: Quantity,
    actor_exists: bool,
    source_exists: bool,
) -> Result<(), Error> {
    require(quantity.0 != 0 && actor_exists && source_exists)
}

/// Spoilage receipts may record zero stock; only the inventory must exist.
pub fn validate_spoilage(inventory_exists: bool) -> Result<(), Error> {
    require(inventory_exists)
}

/// Shared by permission sharing, access requests and access refusals.
pub fn validate_inventory_access(
    actor_exists: bool,
    recipient_exists: bool,
    inventory_exists: bool,
) -> Result<(), Error> {
    require(actor_exists && recipient_exists && inventory_exists)
}

/// Shared by drinking and water purification receipts.
pub fn validate_water_source(actor_exists: bool, source_good: Option<Good>) -> Result<(), Error> {
    require(actor_exists && source_good == Some(Good::Water))
}

pub fn validate_care(
    quantity: Quantity,
    caregiver_exists: bool,
    child_exists: bool,
    source_exists: bool,
) -> Result<(), Error> {
    require(quantity.0 != 0 && caregiver_exists && child_exists && source_exists)
}

pub fn validate_pending_transfer(
    quantity: Quantity,
    source: InventoryId,
    destination: InventoryId,
    actor_exists: bool,
    source_exists: bool,
    destination_exists: bool,
) -> Result<(), Error> {
    validate_transfer(quantity, actor_exists, source_exists, destination_exists)?;
    if source == destination {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    } else {
        Ok(())
    }
}

pub fn validate_spoil_deadline(
    stock: Option<&crate::Inventory>,
    deadline: SimTime,
) -> Result<(), Error> {
    if stock.is_some_and(|stock| stock.good == Good::Food && stock.spoils_at == Some(deadline)) {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_transfer_rejects_destination_changed_to_source() {
        assert!(
            validate_pending_transfer(
                Quantity(1),
                InventoryId(1),
                InventoryId(2),
                true,
                true,
                true
            )
            .is_ok()
        );
        assert!(
            validate_pending_transfer(
                Quantity(1),
                InventoryId(1),
                InventoryId(1),
                true,
                true,
                true
            )
            .is_err()
        );
    }
    #[test]
    fn transfer_checks_each_reference_and_nonzero_quantity() {
        assert!(validate_transfer(Quantity(u64::MAX), true, true, true).is_ok());
        assert!(validate_transfer(Quantity(0), true, true, true).is_err());
        for missing in 0..3 {
            let mut exists = [true; 3];
            exists[missing] = false;
            assert!(validate_transfer(Quantity(1), exists[0], exists[1], exists[2]).is_err());
        }
        for actor in [false, true] {
            for source in [false, true] {
                assert_eq!(
                    validate_transfer_failed(actor, source).is_ok(),
                    actor && source
                );
                assert_eq!(
                    validate_consumption(Quantity(1), actor, source).is_ok(),
                    actor && source
                );
            }
        }
        assert!(validate_consumption(Quantity(0), true, true).is_err());
    }

    #[test]
    fn access_and_care_require_all_identity_facts() {
        for actor in [false, true] {
            for recipient in [false, true] {
                for inventory in [false, true] {
                    let valid = actor && recipient && inventory;
                    assert_eq!(
                        validate_inventory_access(actor, recipient, inventory).is_ok(),
                        valid
                    );
                    assert_eq!(
                        validate_care(Quantity(1), actor, recipient, inventory).is_ok(),
                        valid
                    );
                }
            }
        }
        assert!(validate_care(Quantity(0), true, true, true).is_err());
    }

    #[test]
    fn water_receipts_require_an_existing_water_source() {
        assert!(validate_water_source(true, Some(Good::Water)).is_ok());
        assert!(validate_water_source(false, Some(Good::Water)).is_err());
        assert!(validate_water_source(true, None).is_err());
        assert!(validate_water_source(true, Some(Good::Food)).is_err());
    }

    #[test]
    fn spoilage_requires_inventory_and_uses_save_error() {
        assert!(validate_spoilage(true).is_ok());
        assert!(matches!(
            validate_spoilage(false),
            Err(Error::InvalidSave(_))
        ));
    }
}

//! Saved production receipts require a real actor, site and output stock.
use game_core::{Error, Quantity};
pub fn validate_produced(
    quantity: Quantity,
    actor_exists: bool,
    site_exists: bool,
    output_exists: bool,
) -> Result<(), Error> {
    if quantity.0 > 0 && actor_exists && site_exists && output_exists {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_production_and_missing_site_are_invalid_receipts() {
        assert!(validate_produced(Quantity(1), true, true, true).is_ok());
        assert!(validate_produced(Quantity(0), true, true, true).is_err());
        assert!(validate_produced(Quantity(1), true, false, true).is_err());
    }
}

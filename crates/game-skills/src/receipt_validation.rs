//! Bounds on published lesson gains retained in saves.
use game_core::Error;
pub fn validate_lesson_gain(gain: u64) -> Result<(), Error> {
    if gain <= 1_000 {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lesson_receipt_gain_bounds_include_endpoints() {
        assert!(validate_lesson_gain(0).is_ok());
        assert!(validate_lesson_gain(1_000).is_ok());
        assert!(validate_lesson_gain(1_001).is_err());
    }
}

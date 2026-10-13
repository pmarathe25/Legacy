//! Resolve a provider's personality while keeping invocation overrides at the
//! boundary and preserving the character-owned fallback behavior.
use crate::decisions::Personality;

/// Delivery provider precedence: invocation override, remembered mind, then
/// the historical restful default when neither source is available.
pub fn delivery_personality(
    invocation: Option<&Personality>,
    mind: Option<&Personality>,
) -> Personality {
    invocation
        .or(mind)
        .cloned()
        .unwrap_or_else(Personality::restful)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_personality_preserves_provider_precedence_and_fallback() {
        let invocation = Personality::industrious();
        let mind = Personality::restful();
        assert_eq!(
            delivery_personality(Some(&invocation), Some(&mind)),
            invocation
        );
        assert_eq!(delivery_personality(None, Some(&mind)), mind);
        assert_eq!(delivery_personality(None, None), Personality::restful());
    }
}

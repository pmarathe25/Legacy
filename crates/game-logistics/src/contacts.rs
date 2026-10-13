//! Contact eligibility facts used by contact deliveries.
use game_core::PersonId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContactFacts {
    pub recipient_alive: bool,
    pub same_place: bool,
}

/// A contact is a usable delivery recipient only when alive and co-located.
pub fn contact_recipient(recipient: PersonId, facts: ContactFacts) -> Option<PersonId> {
    (facts.recipient_alive && facts.same_place).then_some(recipient)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_requires_alive_recipient_at_the_same_place() {
        let recipient = PersonId(7);
        assert_eq!(
            contact_recipient(
                recipient,
                ContactFacts {
                    recipient_alive: true,
                    same_place: true,
                }
            ),
            Some(recipient)
        );
        for facts in [
            ContactFacts {
                recipient_alive: false,
                same_place: true,
            },
            ContactFacts {
                recipient_alive: true,
                same_place: false,
            },
            ContactFacts::default(),
        ] {
            assert_eq!(contact_recipient(recipient, facts), None);
        }
    }
}

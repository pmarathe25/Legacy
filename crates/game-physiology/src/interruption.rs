//! Receiving effects when ongoing physical activity is interrupted.
use game_core::PersonId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptionActivity {
    Other,
    Care { child: PersonId },
    Comfort { child: PersonId },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalInterruption {
    pub idle: PersonId,
    pub deadline: PersonId,
    pub care: Option<PersonId>,
}
pub fn prepare_activity_interruption(
    actor: PersonId,
    activity: InterruptionActivity,
) -> PhysicalInterruption {
    PhysicalInterruption {
        idle: actor,
        deadline: actor,
        care: match activity {
            InterruptionActivity::Care { child } | InterruptionActivity::Comfort { child } => {
                Some(child)
            }
            InterruptionActivity::Other => None,
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interruption_always_ends_physical_activity_and_releases_only_care_links() {
        for stamp in [
            InterruptionActivity::Other,
            InterruptionActivity::Care { child: PersonId(2) },
            InterruptionActivity::Comfort { child: PersonId(2) },
        ] {
            let effect = prepare_activity_interruption(PersonId(1), stamp);
            assert_eq!(effect.idle, PersonId(1));
            assert_eq!(effect.deadline, PersonId(1));
            assert_eq!(
                effect.care,
                if stamp == InterruptionActivity::Other {
                    None
                } else {
                    Some(PersonId(2))
                }
            );
        }
    }
}

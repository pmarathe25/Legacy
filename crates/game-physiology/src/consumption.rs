//! Receive physical consumption receipts at their actual interval boundary.
use game_core::{Good, InventoryId, PersonId, SimTime};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumptionReceipt {
    Ate {
        actor: PersonId,
    },
    Cared {
        child: PersonId,
        source: InventoryId,
    },
    Other,
}
pub fn consumed_food_at(
    actor: PersonId,
    now: SimTime,
    receipts_newest_first: impl IntoIterator<Item = (SimTime, ConsumptionReceipt)>,
    mut good: impl FnMut(InventoryId) -> Good,
) -> bool {
    receipts_newest_first
        .into_iter()
        .take_while(|(at, _)| *at == now)
        .any(|(_, receipt)| match receipt {
            ConsumptionReceipt::Ate { actor: id } => id == actor,
            ConsumptionReceipt::Cared { child, source } => {
                child == actor && good(source) == Good::Food
            }
            ConsumptionReceipt::Other => false,
        })
}
impl crate::IntervalReaction {
    pub fn needs_interruption(self, work_failed: bool) -> bool {
        self.needs_interruption_after_work(work_failed, false)
    }
    /// Successful completion already ends work. It takes precedence over the
    /// coincident exhaustion reaction; mortality has its separate cleanup.
    pub fn needs_interruption_after_work(self, work_failed: bool, work_completed: bool) -> bool {
        !self.died && !work_completed && (self.interrupted || work_failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_relief_requires_actual_food_for_the_receiver_at_current_time() {
        let actor = PersonId(1);
        let now = SimTime(5);
        let care = ConsumptionReceipt::Cared {
            child: actor,
            source: InventoryId(2),
        };
        assert!(consumed_food_at(actor, now, [(now, care)], |_| Good::Food));
        assert!(!consumed_food_at(actor, now, [(now, care)], |_| {
            Good::Water
        }));
        assert!(!consumed_food_at(
            PersonId(9),
            now,
            [(now, care)],
            |_| panic!("unrelated child's source")
        ));
        assert!(!consumed_food_at(
            actor,
            now,
            [(SimTime(4), ConsumptionReceipt::Ate { actor })],
            |_| panic!("no care")
        ));
        assert!(consumed_food_at(
            actor,
            now,
            [(now, ConsumptionReceipt::Ate { actor })],
            |_| panic!("actual meal")
        ));
    }
    #[test]
    fn death_suppresses_interruption_even_when_work_cannot_publish() {
        for died in [false, true] {
            for interrupted in [false, true] {
                let reaction = crate::IntervalReaction {
                    died,
                    interrupted,
                    recovered: false,
                };
                assert_eq!(reaction.needs_interruption(true), !died);
                assert_eq!(reaction.needs_interruption(false), !died && interrupted);
            }
        }
    }
    #[test]
    fn successful_work_ends_once_even_at_the_exhaustion_boundary() {
        for died in [false, true] {
            for interrupted in [false, true] {
                let reaction = crate::IntervalReaction {
                    died,
                    interrupted,
                    recovered: false,
                };
                assert!(!reaction.needs_interruption_after_work(false, true));
                assert_eq!(reaction.needs_interruption_after_work(true, false), !died);
                assert_eq!(
                    reaction.needs_interruption_after_work(false, false),
                    !died && interrupted
                );
            }
        }
    }
}

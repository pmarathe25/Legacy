//! A body's budget for the previous institutional labor admission rule.
use game_core::{SimDuration, Vitals};
pub fn sustainable_work(
    vitals: &Vitals,
    fatigue_limit: u64,
    hunger_harm: u64,
    eating_time: SimDuration,
) -> SimDuration {
    let reserved = vitals.hunger.saturating_add(eating_time.0);
    if vitals.fatigue >= fatigue_limit || reserved >= hunger_harm {
        SimDuration(0)
    } else {
        SimDuration(hunger_harm - reserved - 1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_preserves_strict_hunger_boundary_and_saturating_overflow() {
        let vitals = Vitals {
            hunger: 100,
            fatigue: 5,
            health: 1,
            alive: true,
        };
        assert_eq!(
            sustainable_work(&vitals, 10, 1_000, SimDuration(100)),
            SimDuration(799)
        );
        assert_eq!(
            sustainable_work(
                &Vitals {
                    fatigue: 10,
                    ..vitals.clone()
                },
                10,
                1_000,
                SimDuration(100)
            ),
            SimDuration(0)
        );
        assert_eq!(
            sustainable_work(
                &Vitals {
                    hunger: u64::MAX - 1,
                    ..vitals
                },
                10,
                u64::MAX,
                SimDuration(2)
            ),
            SimDuration(0)
        );
    }
}

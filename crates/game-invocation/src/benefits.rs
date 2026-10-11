//! Invocation owns how historical mastery combines with current ability.
use crate::{Burden, compose_work_rate};
use game_core::{Error, SkillRate};

pub fn borrowed_level(levels: impl IntoIterator<Item = u16>) -> u16 {
    let (maximum, sum) = levels
        .into_iter()
        .fold((0_u16, 0_u64), |(maximum, sum), level| {
            (maximum.max(level), sum.saturating_add(u64::from(level)))
        });
    (u64::from(maximum) + (sum - u64::from(maximum)) / 2).min(200) as u16
}
pub fn borrowed_work_rate(
    own: SkillRate,
    body: SkillRate,
    burden: &Burden,
    borrowed: Option<u16>,
    bonus_per_level: u16,
) -> Result<SkillRate, Error> {
    let borrowed = borrowed.map_or(0, |level| {
        1_000 + u64::from(level) * u64::from(bonus_per_level)
    });
    let penalty = u64::from(burden.temporary_identity) + u64::from(burden.permanent_identity);
    compose_work_rate(own, body, borrowed, penalty)
}
pub fn borrowed_combat_damage(
    base: u64,
    own: u16,
    borrowed: u16,
    bonus_per_level: u16,
    burden: &Burden,
) -> Result<u64, Error> {
    let bonus = u64::from(bonus_per_level);
    let penalty = u64::from(burden.temporary_identity) + u64::from(burden.permanent_identity);
    let denominator = 1_000 + u64::from(own) * bonus;
    let multiplier = (1_000 + u64::from(borrowed.max(own)) * bonus)
        .saturating_sub(penalty * 5)
        .max(1);
    u64::try_from(u128::from(base) * u128::from(multiplier) / u128::from(denominator))
        .map_err(|_| Error::Overflow)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strongest_life_plus_half_the_others_is_order_independent_and_bounded() {
        assert_eq!(borrowed_level([]), 0);
        assert_eq!(borrowed_level([75, 10, 40]), 100);
        assert_eq!(borrowed_level([40, 75, 10]), 100);
        assert_eq!(borrowed_level([100; 8]), 200);
        assert_eq!(borrowed_level([10, 9]), 14);
    }
    #[test]
    fn bodily_costs_reduce_current_and_borrowed_benefits_without_changing_mastery() {
        let burden = Burden {
            temporary_identity: 10,
            permanent_identity: 20,
            ..Burden::default()
        };
        assert_eq!(borrowed_combat_damage(1000, 10, 50, 5, &burden), Ok(1047));
        assert_eq!(
            borrowed_combat_damage(1000, 50, 10, 5, &Burden::default()),
            Ok(1000)
        );
        assert_eq!(
            borrowed_combat_damage(u64::MAX, 0, 200, 100, &Burden::default()),
            Err(Error::Overflow)
        );
        assert_eq!(
            borrowed_work_rate(
                SkillRate {
                    numerator: 1100,
                    denominator: 1000
                },
                SkillRate {
                    numerator: 900,
                    denominator: 1000
                },
                &burden,
                Some(50),
                10
            ),
            compose_work_rate(
                SkillRate {
                    numerator: 1100,
                    denominator: 1000
                },
                SkillRate {
                    numerator: 900,
                    denominator: 1000
                },
                1500,
                30
            )
        );
    }
}

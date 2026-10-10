//! Numerical body rules. External systems supply contributions; this crate has
//! no knowledge of health, invocation, inventories, decisions, or Simulation.
pub mod traits;

use game_core::{Error, Vitals};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodySnapshot {
    pub vitals: Vitals,
    pub fatigue_remainder: u16,
    pub health_remainder: u16,
}
#[derive(Clone, Copy, Debug)]
pub struct BodyLimits {
    pub hunger_harm: u64,
    pub fatigue_limit: u64,
    pub max_health: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ActivityFlags {
    pub exerting: bool,
    pub resting: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Contribution {
    /// Signed changes in thousandths of one body unit over this interval.
    pub health_milli: i128,
    pub fatigue_milli: i128,
}
#[derive(Clone, Copy, Debug)]
pub struct BodyInput<'a> {
    pub elapsed_ms: u64,
    pub hunger_exempt: bool,
    pub immortal: bool,
    pub activity: ActivityFlags,
    pub contributions: &'a [Contribution],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseTerms {
    pub hunger: u64,
    pub hunger_harm: u64,
    pub healing: u64,
    pub fatigue_rate: i128,
}

pub fn base_terms(
    before: &BodySnapshot,
    limits: BodyLimits,
    input: BodyInput<'_>,
) -> Result<BaseTerms, Error> {
    let hunger = if input.hunger_exempt || !before.vitals.alive {
        before.vitals.hunger
    } else {
        before
            .vitals
            .hunger
            .checked_add(input.elapsed_ms)
            .ok_or(Error::Overflow)?
    };
    let hunger_harm = if input.hunger_exempt {
        0
    } else {
        hunger.saturating_sub(limits.hunger_harm)
            - before.vitals.hunger.saturating_sub(limits.hunger_harm)
    };
    let healing = if input.immortal {
        input.elapsed_ms
    } else if input.activity.resting {
        input
            .elapsed_ms
            .min(limits.hunger_harm.saturating_sub(before.vitals.hunger))
    } else {
        0
    };
    Ok(BaseTerms {
        hunger,
        hunger_harm,
        healing,
        fatigue_rate: if input.activity.exerting {
            1_000
        } else if input.activity.resting {
            -2_000
        } else {
            0
        },
    })
}

pub fn integrate(
    before: &BodySnapshot,
    limits: BodyLimits,
    input: BodyInput<'_>,
) -> Result<BodySnapshot, Error> {
    if before.fatigue_remainder >= 1_000
        || before.health_remainder >= 1_000
        || limits.max_health == 0
        || limits.fatigue_limit == 0
        || before.vitals.health > limits.max_health
        || before.vitals.fatigue > limits.fatigue_limit
    {
        return Err(Error::InvalidConfiguration(
            "invalid body snapshot or limits".into(),
        ));
    }
    if !before.vitals.alive {
        return Ok(BodySnapshot {
            vitals: before.vitals.clone(),
            fatigue_remainder: 0,
            health_remainder: 0,
        });
    }
    let terms = base_terms(before, limits, input)?;
    let mut fatigue_milli = i128::from(before.vitals.fatigue) * 1_000
        + i128::from(before.fatigue_remainder)
        + i128::from(input.elapsed_ms) * terms.fatigue_rate;
    let mut health_milli = i128::from(before.vitals.health) * 1_000
        + i128::from(before.health_remainder)
        + i128::from(terms.healing) * 1_000
        - i128::from(terms.hunger_harm) * 1_000;
    for contribution in input.contributions {
        fatigue_milli = fatigue_milli
            .checked_add(contribution.fatigue_milli)
            .ok_or(Error::Overflow)?;
        health_milli = health_milli
            .checked_add(contribution.health_milli)
            .ok_or(Error::Overflow)?;
    }
    let fatigue_milli = fatigue_milli.clamp(0, i128::from(limits.fatigue_limit) * 1_000);
    let health_milli = health_milli.clamp(0, i128::from(limits.max_health) * 1_000);
    let mut vitals = before.vitals.clone();
    vitals.hunger = terms.hunger;
    vitals.fatigue = (fatigue_milli / 1_000) as u64;
    vitals.health = (health_milli / 1_000) as u64;
    if vitals.health == 0 && !input.immortal {
        vitals.alive = false;
    }
    let health_remainder = if vitals.alive {
        (health_milli % 1_000) as u16
    } else {
        0
    };
    Ok(BodySnapshot {
        vitals,
        fatigue_remainder: (fatigue_milli % 1_000) as u16,
        health_remainder,
    })
}

/// Find the first crossing within an interval whose activity and contribution
/// rates are stable. Clinical/calendar/consent transitions clip the upper bound
/// before this search; the supplied projection uses the same integration path.
pub fn first_crossing<F>(limit_ms: u64, mut crossed: F) -> Result<Option<u64>, Error>
where
    F: FnMut(u64) -> Result<bool, Error>,
{
    if limit_ms == 0 || !crossed(limit_ms)? {
        return Ok(None);
    }
    let (mut lower, mut upper) = (1, limit_ms);
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if crossed(middle)? {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    Ok(Some(lower))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(health: u64) -> BodySnapshot {
        BodySnapshot {
            vitals: Vitals {
                hunger: 0,
                fatigue: 0,
                health,
                alive: true,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        }
    }
    fn limits() -> BodyLimits {
        BodyLimits {
            hunger_harm: 100,
            fatigue_limit: 100,
            max_health: 100,
        }
    }
    fn input<'a>(elapsed_ms: u64, contributions: &'a [Contribution]) -> BodyInput<'a> {
        BodyInput {
            elapsed_ms,
            hunger_exempt: false,
            immortal: false,
            activity: ActivityFlags::default(),
            contributions,
        }
    }
    #[test]
    fn healing_and_harm_combine_before_one_clamp() {
        let modifiers = [
            Contribution {
                health_milli: 50_000,
                fatigue_milli: 0,
            },
            Contribution {
                health_milli: -40_000,
                fatigue_milli: 0,
            },
        ];
        let result = integrate(&body(95), limits(), input(1, &modifiers)).unwrap();
        assert_eq!(result.vitals.health, 100);
        let reversed = [modifiers[1], modifiers[0]];
        assert_eq!(
            integrate(&body(95), limits(), input(1, &reversed)).unwrap(),
            result
        );
    }
    #[test]
    fn fractional_harm_and_strain_are_exact_across_slices() {
        let mut whole = body(100);
        whole.vitals.fatigue = 10;
        let rates = |dt| {
            [Contribution {
                health_milli: -333 * i128::from(dt),
                fatigue_milli: 71 * i128::from(dt),
            }]
        };
        let expected = integrate(&whole, limits(), input(37, &rates(37))).unwrap();
        let mut split = whole;
        for dt in [1, 7, 3, 26] {
            split = integrate(&split, limits(), input(dt, &rates(dt))).unwrap();
        }
        assert_eq!(split, expected);
        assert_eq!(expected.vitals.health, 87);
        assert_eq!(expected.health_remainder, 679);
    }
    #[test]
    fn a_dead_mortal_cannot_recover_from_a_later_contribution() {
        let dead = integrate(
            &body(1),
            limits(),
            input(
                1,
                &[Contribution {
                    health_milli: -501,
                    fatigue_milli: 0,
                }],
            ),
        )
        .unwrap();
        assert!(!dead.vitals.alive);
        assert_eq!(dead.health_remainder, 0);
        assert_eq!(
            integrate(
                &dead,
                limits(),
                input(
                    10,
                    &[Contribution {
                        health_milli: 50_000,
                        fatigue_milli: 0
                    }]
                )
            )
            .unwrap(),
            dead
        );
    }
    #[test]
    fn an_immortal_recovers_from_zero_without_a_hunger_requirement() {
        let mut request = input(20, &[]);
        request.immortal = true;
        request.hunger_exempt = true;
        let result = integrate(&body(0), limits(), request).unwrap();
        assert!(result.vitals.alive);
        assert_eq!(result.vitals.health, 20);
        assert_eq!(result.vitals.hunger, 0);
    }
    #[test]
    fn starvation_crossing_is_found_at_the_first_exact_body_boundary() {
        let start = body(30);
        let boundary = first_crossing(200, |dt| {
            Ok(!integrate(&start, limits(), input(dt, &[]))?.vitals.alive)
        })
        .unwrap();
        assert_eq!(boundary, Some(130));
        assert!(
            integrate(&start, limits(), input(129, &[]))
                .unwrap()
                .vitals
                .alive
        );
    }
    #[test]
    fn bad_input_and_numeric_overflow_do_not_change_the_snapshot() {
        let start = body(100);
        let original = start.clone();
        assert_eq!(
            integrate(
                &start,
                limits(),
                input(
                    1,
                    &[Contribution {
                        health_milli: i128::MAX,
                        fatigue_milli: 0
                    }]
                )
            ),
            Err(Error::Overflow)
        );
        assert_eq!(start, original);
        let mut bad = start;
        bad.health_remainder = 1_000;
        assert!(integrate(&bad, limits(), input(1, &[])).is_err());
    }
}

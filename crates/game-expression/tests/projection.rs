use game_core::*;
use game_expression::*;
const YEAR: u64 = 365 * DAY.0;
const ABILITIES: &[&str] = &[
    "intelligence",
    "attention",
    "learning",
    "foresight",
    "resilience",
    "interpretation",
    "hearing",
    "strength",
    "speed",
    "endurance",
    "agility",
    "hardiness",
    "robustness",
    "size",
    "constitution",
    "metabolism",
    "recovery",
    "vocalization",
    "gesturing",
    "custom",
];
#[test]
fn shared_factors_match_reference_at_every_growth_and_recovery_boundary() {
    let mut ages = vec![0, u64::MAX];
    for years in [0, 1, 2, 3, 6, 8, 12, 14, 16, 18, 20, 24, 40, 80, 90] {
        let boundary = years * YEAR;
        ages.extend([boundary.saturating_sub(1), boundary, boundary + 1]);
    }
    // Include exact truncation transitions between knots and during daily recovery.
    for tick in 0..1000 {
        ages.push(tick * DAY.0 + tick * 137);
    }
    for span in [None, Some(SimDuration(80 * YEAR))] {
        for (physical, stress, satisfaction) in [(0, 1000, -1000), (700, 170, 30), (1000, 0, 0)] {
            let life = Life {
                born: SimTime(DAY.0),
                span,
                base_health: 100,
                expressed: true,
                physical,
                stress,
                satisfaction,
                updated: SimTime(3 * DAY.0),
                upbringing: Upbringing {
                    since: SimTime(2 * DAY.0),
                    deprivation: YEAR * 300,
                    distress: YEAR * 100,
                },
            };
            for &age in &ages {
                let at = SimTime(age);
                let mut projected = Projection::new(life, at);
                for raw in [0, 333, 1000, 16000, u64::MAX] {
                    for &key in ABILITIES.iter().rev() {
                        assert_eq!(
                            projected.ability(raw, key),
                            life.ability(raw, key, at),
                            "{age} {key} {raw}"
                        );
                        assert_eq!(projected.maturity(key), life.maturity(at, key));
                    }
                }
                assert_eq!(
                    projected.maturity("language"),
                    life.maturity(at, "language")
                );
                for raw in [-16000, -100, 0, 333, 1000, 16000] {
                    for key in [
                        "patience",
                        "risk-tolerance",
                        "sociability",
                        "empathy",
                        "compassion",
                        "rigidity",
                        "custom",
                    ] {
                        assert_eq!(
                            projected.disposition(raw, key),
                            life.disposition(raw, key, at),
                            "{age} {key} {raw}"
                        );
                    }
                }
            }
        }
    }
}

use serde::{Deserialize, Serialize};
/// A physiological rate in hunger units per explicit elapsed duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HungerRate {
    pub units: u32,
    pub per_millis: u64,
}
/// Starvation damage in health units per explicit elapsed duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarvationRate {
    pub units: u32,
    pub per_millis: u64,
}
/// An incapacitated immortal retains their body and recovers instead of dying.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Immortal {
    pub recovery_per_tick: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: u64,
    pub region: Option<u64>,
    pub health: u32,
    pub hunger: u32,
    /// Sub-unit hunger accumulated against the configured hunger rate period.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub hunger_remainder: u64,
    /// Fractional accumulated starvation damage over its configured period.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub starvation_remainder: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immortal: Option<Immortal>,
    /// A personal preference, not an obligation.
    pub willing_to_work: bool,
    pub seek_meals: bool,
    #[serde(default)]
    pub controlled: bool,
}
fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl Person {
    /// Time until starvation is fatal, including any time before its threshold.
    pub fn starvation_death_after(
        &self,
        threshold: u32,
        hunger_rate: u32,
        hunger_period: u64,
        damage: StarvationRate,
    ) -> Result<Option<u128>, &'static str> {
        if damage.per_millis == 0 || self.starvation_remainder >= damage.per_millis {
            return Err("invalid fractional starvation damage");
        }
        let crossing = self.hunger_threshold_after(threshold, hunger_rate, hunger_period)?;
        if self.health == 0 || self.immortal.is_some() || damage.units == 0 {
            return Ok(None);
        }
        let Some(crossing) = crossing else {
            return Ok(None);
        };
        let needed = u128::from(self.health) * u128::from(damage.per_millis)
            - u128::from(self.starvation_remainder);
        Ok(Some(crossing + needed.div_ceil(u128::from(damage.units))))
    }

    /// Apply elapsed starvation exposure using the opening hunger state.
    pub fn advance_starvation(
        &mut self,
        threshold: u32,
        hunger_rate: u32,
        hunger_period: u64,
        damage: StarvationRate,
        elapsed_millis: u64,
    ) -> Result<(), &'static str> {
        let death = self.starvation_death_after(threshold, hunger_rate, hunger_period, damage)?;
        if self.health == 0 || self.immortal.is_some() {
            return Ok(());
        }
        let Some(crossing) = self.hunger_threshold_after(threshold, hunger_rate, hunger_period)?
        else {
            return Ok(());
        };
        let exposure = u128::from(elapsed_millis).saturating_sub(crossing);
        let credit = u128::from(self.starvation_remainder) + exposure * u128::from(damage.units);
        let amount = credit / u128::from(damage.per_millis);
        if death.is_some_and(|after| after <= u128::from(elapsed_millis)) {
            self.health = 0;
            self.starvation_remainder = 0;
        } else {
            self.health -= amount as u32;
            self.starvation_remainder = (credit % u128::from(damage.per_millis)) as u64;
        }
        Ok(())
    }
    pub(crate) fn valid_hunger_progress(&self, quantum_millis: u64) -> bool {
        quantum_millis != 0
            && self.hunger_remainder < quantum_millis
            && (self.hunger != u32::MAX || self.hunger_remainder == 0)
            && (self.immortal.is_none() || (self.hunger == 0 && self.hunger_remainder == 0))
    }

    /// Earliest whole millisecond at which this person's hunger reaches a
    /// threshold. No threshold is scheduled for a dead or immortal person,
    /// or for a zero rate below the threshold.
    pub fn hunger_threshold_after(
        &self,
        threshold: u32,
        rate: u32,
        quantum_millis: u64,
    ) -> Result<Option<u128>, &'static str> {
        if !self.valid_hunger_progress(quantum_millis) {
            return Err("invalid fractional hunger");
        }
        if self.health == 0 || self.immortal.is_some() {
            return Ok(None);
        }
        if self.hunger >= threshold {
            return Ok(Some(0));
        }
        if rate == 0 {
            return Ok(None);
        }
        let quantum = u128::from(quantum_millis);
        let remaining = u128::from(threshold) * quantum
            - (u128::from(self.hunger) * quantum + u128::from(self.hunger_remainder));
        Ok(Some(remaining.div_ceil(u128::from(rate))))
    }

    /// Advance hunger alone; callers retain responsibility for meals, damage,
    /// decisions and publishing the resulting state.
    pub fn advance_hunger(
        &mut self,
        rate: u32,
        elapsed_millis: u64,
        quantum_millis: u64,
    ) -> Result<(), &'static str> {
        if !self.valid_hunger_progress(quantum_millis) {
            return Err("invalid fractional hunger");
        }
        if self.health == 0 || self.immortal.is_some() {
            return Ok(());
        }
        let quantum = u128::from(quantum_millis);
        let value = (u128::from(self.hunger) * quantum
            + u128::from(self.hunger_remainder)
            + u128::from(rate) * u128::from(elapsed_millis))
        .min(u128::from(u32::MAX) * quantum);
        self.hunger = (value / quantum) as u32;
        self.hunger_remainder = (value % quantum) as u64;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Compatibility rate for older whole-boundary worlds.
    #[serde(default)]
    pub hunger_per_tick: u32,
    /// Explicit elapsed-time rate; absent retains legacy quantum-based behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunger_rate: Option<HungerRate>,
    pub meal_threshold: u32,
    pub starvation_threshold: u32,
    #[serde(default)]
    pub starvation_damage: u32,
    /// Explicit starvation rate; absent retains legacy boundary damage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starvation_rate: Option<StarvationRate>,
    pub max_health: u32,
    pub carrying_capacity: u64,
    pub max_work_hunger: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person() -> Person {
        Person {
            id: 1,
            region: Some(1),
            health: 100,
            hunger: 0,
            hunger_remainder: 0,
            starvation_remainder: 0,
            immortal: None,
            willing_to_work: true,
            seek_meals: false,
            controlled: false,
        }
    }

    #[test]
    fn starvation_exposure_preserves_fraction_across_threshold_and_resume() {
        let damage = StarvationRate {
            units: 3,
            per_millis: 1000,
        };
        let mut whole = person();
        whole.advance_starvation(1, 2, 1000, damage, 1000).unwrap();
        assert_eq!((whole.health, whole.starvation_remainder), (99, 500));
        let mut split = person();
        for elapsed in [200, 333, 467] {
            split
                .advance_starvation(1, 2, 1000, damage, elapsed)
                .unwrap();
            split.advance_hunger(2, elapsed, 1000).unwrap();
            split = serde_json::from_str(&serde_json::to_string(&split).unwrap()).unwrap();
        }
        assert_eq!(
            (split.health, split.starvation_remainder),
            (whole.health, whole.starvation_remainder)
        );
        let mut dying = person();
        dying.health = 2;
        assert_eq!(
            dying.starvation_death_after(1, 2, 1000, damage).unwrap(),
            Some(1167)
        );
        dying.advance_starvation(1, 2, 1000, damage, 1166).unwrap();
        assert_eq!(dying.health, 1);
        let mut invalid = person();
        invalid.starvation_remainder = 1000;
        assert!(invalid.advance_starvation(1, 2, 1000, damage, 1).is_err());
    }

    #[test]
    fn fractional_hunger_is_partition_independent_and_survives_reload() {
        let mut whole = person();
        whole.advance_hunger(3, 1000, 1000).unwrap();
        let mut split = person();
        split.advance_hunger(3, 333, 1000).unwrap();
        assert_eq!((split.hunger, split.hunger_remainder), (0, 999));
        split = serde_json::from_str(&serde_json::to_string(&split).unwrap()).unwrap();
        split.advance_hunger(3, 667, 1000).unwrap();
        assert_eq!(split, whole);
        let mut old = serde_json::to_value(person()).unwrap();
        old.as_object_mut().unwrap().remove("hunger_remainder");
        assert_eq!(
            serde_json::from_value::<Person>(old)
                .unwrap()
                .hunger_remainder,
            0
        );
    }

    #[test]
    fn hunger_threshold_deadlines_are_exact_and_individual() {
        let mut p = person();
        p.hunger = 2;
        p.hunger_remainder = 250;
        assert_eq!(p.hunger_threshold_after(4, 3, 1000).unwrap(), Some(584));
        let mut before = p.clone();
        before.advance_hunger(3, 583, 1000).unwrap();
        assert_eq!(before.hunger, 3);
        before.advance_hunger(3, 1, 1000).unwrap();
        assert_eq!(before.hunger, 4);
        assert_eq!(p.hunger_threshold_after(2, 0, 1000).unwrap(), Some(0));
        assert_eq!(p.hunger_threshold_after(4, 0, 1000).unwrap(), None);
        assert!(p.hunger_threshold_after(4, 3, 0).is_err());
        p.health = 0;
        assert_eq!(p.hunger_threshold_after(4, 3, 1000).unwrap(), None);
        p.health = 100;
        p.hunger = 0;
        p.hunger_remainder = 0;
        p.immortal = Some(Immortal {
            recovery_per_tick: 1,
        });
        assert_eq!(p.hunger_threshold_after(4, 3, 1000).unwrap(), None);
    }

    #[test]
    fn hunger_deadline_reduction_is_deterministic_and_validates_every_person() {
        let mut people: Vec<_> = (1..=4096)
            .map(|id| {
                let mut p = person();
                p.id = id;
                p.hunger = (id % 10) as u32;
                p
            })
            .collect();
        let run = |people: &[Person], workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| crate::next_hunger_threshold(people, 10, 3, 1000))
        };
        assert_eq!(run(&people, 1), Ok(Some(334)));
        assert_eq!(run(&people, 4), Ok(Some(334)));
        people[100].hunger_remainder = 1000;
        assert!(run(&people, 4).is_err());
        assert!(crate::next_hunger_threshold(&[], 10, 3, 0).is_err());
        assert_eq!(crate::next_hunger_threshold(&[], 10, 3, 1000), Ok(None));
    }

    #[test]
    #[ignore = "manual parallel deadline benchmark"]
    fn hunger_deadline_benchmark() {
        let workers = std::env::var("LEGACY_WORKERS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1);
        let count = std::env::var("LEGACY_PEOPLE")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1_000_000u64);
        let people: Vec<_> = (1..=count)
            .map(|id| {
                let mut p = person();
                p.id = id;
                p.hunger = (id % 10) as u32;
                p
            })
            .collect();
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut samples = Vec::new();
                for round in 0..6 {
                    let started = std::time::Instant::now();
                    let deadline = crate::next_hunger_threshold(&people, 10, 3, 1000).unwrap();
                    let elapsed = started.elapsed().as_secs_f64();
                    assert_eq!(deadline, Some(334));
                    if round > 0 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_by(f64::total_cmp);
                println!(
                    "workers={workers} people={count} deadline_ms=334 elapsed={:.6}",
                    samples[2]
                );
            });
    }

    #[test]
    fn fractional_hunger_saturates_and_does_not_affect_immortals_or_dead() {
        let mut p = person();
        p.hunger = u32::MAX - 1;
        p.advance_hunger(u32::MAX, u64::MAX, 1000).unwrap();
        assert_eq!((p.hunger, p.hunger_remainder), (u32::MAX, 0));
        let mut p = person();
        p.immortal = Some(Immortal {
            recovery_per_tick: 1,
        });
        p.advance_hunger(1, 500, 1000).unwrap();
        assert_eq!((p.hunger, p.hunger_remainder), (0, 0));
        p.immortal = None;
        p.health = 0;
        p.advance_hunger(1, 500, 1000).unwrap();
        assert_eq!((p.hunger, p.hunger_remainder), (0, 0));
    }

    #[test]
    fn whole_boundary_meals_preserve_or_clear_fraction_at_zero() {
        let mut p = person();
        p.hunger = 1;
        p.hunger_remainder = 500;
        let rules = Rules {
            hunger_per_tick: 1,
            hunger_rate: None,
            meal_threshold: 1,
            starvation_threshold: 10,
            starvation_damage: 1,
            starvation_rate: None,
            max_health: 100,
            carrying_capacity: 1,
            max_work_hunger: 10,
        };
        let run = |relief| {
            crate::prepare(
                &[p.clone()],
                &rules,
                None,
                &[],
                game_lot_diet::Effects {
                    hunger_relief: vec![(1, relief)],
                    pending: vec![],
                },
                1,
            )
            .people
            .remove(0)
        };
        let exact = run(2);
        assert_eq!((exact.hunger, exact.hunger_remainder), (0, 500));
        let excess = run(3);
        assert_eq!((excess.hunger, excess.hunger_remainder), (0, 0));
    }

    #[test]
    fn batched_fractional_hunger_is_atomic_and_matches_across_workers() {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut people: Vec<_> = (1..=4096)
                        .map(|id| {
                            let mut row = person();
                            row.id = id;
                            if id == 1 {
                                row.immortal = Some(Immortal {
                                    recovery_per_tick: 1,
                                });
                            }
                            row
                        })
                        .collect();
                    let before = people.clone();
                    assert!(crate::progress_hunger(&mut people, 3, 1, 0).is_err());
                    assert_eq!(people, before);
                    people[100].hunger_remainder = 1000;
                    let invalid = people.clone();
                    assert!(crate::progress_hunger(&mut people, 3, 1, 1000).is_err());
                    assert_eq!(people, invalid);
                    people[100].hunger_remainder = 0;
                    crate::progress_hunger(&mut people, 3, 333, 1000).unwrap();
                    crate::progress_hunger(&mut people, 3, 667, 1000).unwrap();
                    assert_eq!(people[0].hunger, 0);
                    assert!(
                        people[1..]
                            .iter()
                            .all(|row| row.hunger == 3 && row.hunger_remainder == 0)
                    );
                    people
                })
        };
        assert_eq!(run(1), run(4));
    }

    #[test]
    fn settlement_matches_whole_boundary_without_double_counting_hunger() {
        let mut initial = person();
        initial.hunger = 1;
        initial.hunger_remainder = 500;
        let rules = Rules {
            hunger_per_tick: 2,
            hunger_rate: None,
            meal_threshold: 1,
            starvation_threshold: 3,
            starvation_damage: 4,
            starvation_rate: None,
            max_health: 100,
            carrying_capacity: 1,
            max_work_hunger: 10,
        };
        for relief in [0, 2, 5] {
            let effects = || game_lot_diet::Effects {
                hunger_relief: vec![(1, relief)],
                pending: vec![],
            };
            let expected = crate::prepare(&[initial.clone()], &rules, None, &[], effects(), 1);
            let mut advanced = vec![initial.clone()];
            crate::progress_hunger(&mut advanced, rules.hunger_per_tick, 400, 1000).unwrap();
            crate::progress_hunger(&mut advanced, rules.hunger_per_tick, 600, 1000).unwrap();
            let actual = crate::settle_elapsed(&advanced, &rules, None, &[], effects(), 1);
            assert_eq!(actual.people, expected.people);
            assert_eq!(actual.pending, expected.pending);
        }
    }
}

use serde::{Deserialize, Serialize};
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
    /// Sub-unit hunger accumulated against the physical time quantum.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub hunger_remainder: u64,
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
    pub(crate) fn valid_hunger_progress(&self, quantum_millis: u64) -> bool {
        quantum_millis != 0
            && self.hunger_remainder < quantum_millis
            && (self.hunger != u32::MAX || self.hunger_remainder == 0)
            && (self.immortal.is_none() || (self.hunger == 0 && self.hunger_remainder == 0))
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
    pub hunger_per_tick: u32,
    pub meal_threshold: u32,
    pub starvation_threshold: u32,
    pub starvation_damage: u32,
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
            immortal: None,
            willing_to_work: true,
            seek_meals: false,
            controlled: false,
        }
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
            meal_threshold: 1,
            starvation_threshold: 10,
            starvation_damage: 1,
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
            meal_threshold: 1,
            starvation_threshold: 3,
            starvation_damage: 4,
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

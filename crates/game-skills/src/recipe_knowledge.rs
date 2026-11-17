//! Individual, bounded knowledge of specific recipes, separate from skill levels.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    /// The specific recipes this person has retained, sorted by ID.
    pub known: Vec<u32>,
    /// Personal order in which this person is willing to explain known recipes.
    pub share: Vec<u32>,
    /// This person's choice to also share other recipes they know.
    #[serde(default)]
    pub share_learned: bool,
    #[serde(default = "default_willing")]
    pub willing_to_share: bool,
    pub willing_to_learn: bool,
    pub capacity: usize,
}

fn default_willing() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Learned {
    pub tick: u64,
    pub teacher: u64,
    pub student: u64,
    pub recipe: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BirthProfile {
    pub willing_to_learn: bool,
    #[serde(default)]
    pub willing_to_share: bool,
    #[serde(default)]
    pub share_learned: bool,
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub minds: Vec<Mind>,
    /// Optional inclinations for new individuals, who begin with no recipes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_profile: Option<BirthProfile>,
    /// Results from the most recent committed encounter boundary only.
    #[serde(default)]
    pub events: Vec<Learned>,
}

impl Knowledge {
    pub fn parallel_copy(&self) -> Self {
        Self {
            minds: self.minds.par_iter().cloned().collect(),
            birth_profile: self.birth_profile,
            events: self.events.par_iter().copied().collect(),
        }
    }
    pub fn enroll_births(&mut self, births: &[u64]) {
        let Some(profile) = self.birth_profile else {
            return;
        };
        self.minds.par_extend(births.par_iter().map(|&person| Mind {
            person,
            known: Vec::new(),
            share: Vec::new(),
            share_learned: profile.share_learned,
            willing_to_share: profile.willing_to_share,
            willing_to_learn: profile.willing_to_learn,
            capacity: profile.capacity,
        }));
        self.minds.par_sort_unstable_by_key(|mind| mind.person);
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        recipes: &HashSet<u32>,
        tick: u64,
    ) -> Result<(), &'static str> {
        if self
            .minds
            .windows(2)
            .any(|rows| rows[0].person >= rows[1].person)
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || mind.known.len() > mind.capacity
                    || mind.known.windows(2).any(|rows| rows[0] >= rows[1])
                    || mind.known.iter().any(|recipe| !recipes.contains(recipe))
                    || mind.share.iter().any(|recipe| !recipes.contains(recipe))
                    || mind.share.iter().collect::<HashSet<_>>().len() != mind.share.len()
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || event.teacher == event.student
                    || !people.contains(&event.teacher)
                    || !people.contains(&event.student)
                    || !recipes.contains(&event.recipe)
            })
        {
            return Err("invalid personal recipe knowledge");
        }
        Ok(())
    }

    /// Interpret disjoint real encounters against the opening knowledge state.
    /// The host supplies each teacher's own willingness or response to pressure.
    pub fn meet(
        &self,
        pairs: &[(u64, u64)],
        tick: u64,
        pressured: impl Fn(u64, u64) -> bool + Sync,
    ) -> Result<Option<Self>, &'static str> {
        let mut participants: Vec<_> = pairs.par_iter().flat_map_iter(|&(a, b)| [a, b]).collect();
        participants.par_sort_unstable();
        if participants.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("recipe learning requires disjoint encounters");
        }
        let mind = |person| {
            self.minds
                .binary_search_by_key(&person, |row| row.person)
                .ok()
                .map(|index| &self.minds[index])
        };
        let mut events: Vec<_> = pairs
            .par_iter()
            .flat_map_iter(|&(a, b)| [(a, b), (b, a)])
            .filter_map(|(teacher, student)| {
                let (source, recipient) = (mind(teacher)?, mind(student)?);
                if !recipient.willing_to_learn
                    || recipient.known.len() >= recipient.capacity
                    || (!source.willing_to_share && !pressured(teacher, student))
                {
                    return None;
                }
                let recipe = source
                    .share
                    .iter()
                    .find(|recipe| {
                        source.known.binary_search(recipe).is_ok()
                            && recipient.known.binary_search(recipe).is_err()
                    })
                    .copied()
                    .or_else(|| {
                        source
                            .share_learned
                            .then(|| {
                                source
                                    .known
                                    .iter()
                                    .find(|recipe| recipient.known.binary_search(recipe).is_err())
                                    .copied()
                            })
                            .flatten()
                    })?;
                Some(Learned {
                    tick,
                    teacher,
                    student,
                    recipe,
                })
            })
            .collect();
        if events.is_empty() {
            return Ok(None);
        }
        events.par_sort_unstable_by_key(|event| (event.student, event.recipe, event.teacher));
        let mut minds: Vec<_> = self.minds.par_iter().cloned().collect();
        minds.par_iter_mut().for_each(|mind| {
            if let Ok(index) = events.binary_search_by_key(&mind.person, |event| event.student) {
                let recipe = events[index].recipe;
                let at = mind.known.binary_search(&recipe).expect_err("new recipe");
                mind.known.insert(at, recipe);
            }
        });
        Ok(Some(Self {
            minds,
            events,
            birth_profile: self.birth_profile,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_encounters_share_specific_recipes_only_when_each_persons_choices_allow_it() {
        let opening = Knowledge {
            minds: vec![
                Mind {
                    person: 1,
                    known: vec![1, 2],
                    share: vec![2, 1],
                    share_learned: false,
                    willing_to_share: false,
                    willing_to_learn: false,
                    capacity: 4,
                },
                Mind {
                    person: 2,
                    known: vec![],
                    share: vec![],
                    share_learned: true,
                    willing_to_share: true,
                    willing_to_learn: true,
                    capacity: 1,
                },
                Mind {
                    person: 3,
                    known: vec![],
                    share: vec![],
                    share_learned: false,
                    willing_to_share: true,
                    willing_to_learn: false,
                    capacity: 2,
                },
            ],
            events: vec![],
            birth_profile: Some(BirthProfile {
                willing_to_learn: true,
                willing_to_share: true,
                share_learned: true,
                capacity: 3,
            }),
        };
        opening
            .validate(&HashSet::from([1, 2, 3]), &HashSet::from([1, 2]), 0)
            .unwrap();
        assert!(opening.meet(&[(1, 2)], 1, |_, _| false).unwrap().is_none());
        assert!(opening.meet(&[(1, 3)], 1, |_, _| true).unwrap().is_none());
        let run = |threads| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| {
                    let learned = opening.meet(&[(1, 2)], 1, |_, _| true).unwrap().unwrap();
                    assert_eq!(learned.minds[1].known, vec![2]);
                    assert_eq!(
                        learned.events,
                        vec![Learned {
                            tick: 1,
                            teacher: 1,
                            student: 2,
                            recipe: 2
                        }]
                    );
                    let restored: Knowledge =
                        serde_json::from_str(&serde_json::to_string(&learned).unwrap()).unwrap();
                    assert_eq!(restored, learned);
                    let mut with_birth = restored.clone();
                    with_birth.enroll_births(&[4]);
                    assert!(with_birth.minds[3].known.is_empty());
                    assert!(with_birth.minds[3].willing_to_learn);
                    let next_generation = with_birth
                        .meet(&[(2, 4)], 2, |_, _| false)
                        .unwrap()
                        .unwrap();
                    assert_eq!(next_generation.minds[3].known, vec![2]);
                    restored
                        .validate(&HashSet::from([1, 2, 3]), &HashSet::from([1, 2]), 1)
                        .unwrap();
                    assert!(learned.meet(&[(1, 2)], 2, |_, _| true).unwrap().is_none());
                    assert!(opening.meet(&[(1, 2), (1, 3)], 1, |_, _| true).is_err());
                    learned
                })
        };
        assert_eq!(run(1), run(4));
    }
}

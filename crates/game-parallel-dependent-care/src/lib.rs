//! Parent-owned attention for dependent children, resolved without shared mutation.
mod inheritance;
use game_parallel_family_columns::Store;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub dependent_until: u64,
    pub max_caregiver_hunger: u32,
    pub neglect_damage: u32,
    /// A selected caregiver may feed the child from personally carried food.
    #[serde(default)]
    pub feed_from_cargo: bool,
    /// A selected caregiver may attempt to take local food directly to the child.
    #[serde(default)]
    pub fetch_local_food: bool,
    /// Personal fear or obligation toward the dependent can motivate care.
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub willing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub child: u64,
    pub caregiver: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependentCare {
    pub rules: Rules,
    /// Newborns can inherit a caregiver preference from one parent.
    #[serde(default)]
    pub inherit_births: bool,
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub events: Vec<Event>,
}
#[derive(Clone, Copy)]
struct Child {
    id: u64,
    rank: (u64, u64),
    first: Option<usize>,
    second: Option<usize>,
}
pub struct Draft {
    pub events: Vec<Event>,
    pub actors: Vec<u64>,
}
impl DependentCare {
    pub fn inherit_preferences(&mut self, births: &[(u64, [u64; 2], usize)]) {
        inheritance::enroll(self, births);
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        adult_age: u64,
        tick: u64,
    ) -> Result<(), &'static str> {
        if self.rules.dependent_until == 0
            || self.rules.dependent_until > adult_age
            || self.rules.pressure_threshold == Some(0)
        {
            return Err("invalid dependent age");
        }
        let mut ids: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        ids.par_sort_unstable();
        if ids.par_windows(2).any(|pair| pair[0] == pair[1])
            || ids.par_iter().any(|id| !people.contains(id))
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || !people.contains(&event.child)
                    || event
                        .caregiver
                        .is_some_and(|id| id == event.child || !people.contains(&id))
            })
        {
            return Err("invalid dependent care state");
        }
        Ok(())
    }
    /// Reserve at most one child per motivated parent, using two deterministic proposal rounds.
    pub fn prepare(
        &self,
        family: &Store,
        adult_age: u64,
        people: &[PersonSnapshot],
        busy: &[u64],
        clock: (u64, u64),
        context: (
            impl Fn(usize, usize) -> bool + Sync,
            impl Fn(u64, u64) -> u64 + Sync,
        ),
    ) -> Draft {
        let (colocated, pressure) = context;
        let (seed, tick) = clock;
        let mut willing: Vec<_> = self
            .policies
            .par_iter()
            .filter(|p| p.willing)
            .map(|p| p.person)
            .collect();
        if willing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            willing.par_sort_unstable();
        }
        let dense_willing = willing.first().and_then(|first| {
            (willing.last()?.checked_sub(*first) == Some((willing.len() - 1) as u64)
                && willing
                    .par_windows(2)
                    .all(|pair| pair[0].checked_add(1) == Some(pair[1])))
            .then_some(*first)
        });
        let mut busy = busy.to_vec();
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        let children: Vec<_> = family
            .metadata
            .par_iter()
            .enumerate()
            .filter_map(|(slot, member)| {
                let child = family.state(slot, false);
                if !child.alive || child.age >= self.rules.dependent_until {
                    return None;
                }
                let index = person_index(people, member.id)?;
                let (_, _, health, _) = people[index];
                if health == 0 {
                    return None;
                }
                let mut candidates = [None, None];
                for (i, &parent) in member.parents[..member.parent_count].iter().enumerate() {
                    let Some(p) = person_index(people, parent) else {
                        continue;
                    };
                    let Some(f) = family.locate(parent) else {
                        continue;
                    };
                    let (_, _, health, hunger) = people[p];
                    if colocated(index, p)
                        && health > 0
                        && hunger <= self.rules.max_caregiver_hunger
                        && family.state(f, false).age >= adult_age
                        && (dense_willing.map_or_else(
                            || willing.binary_search(&parent).is_ok(),
                            |first| parent >= first && parent - first < willing.len() as u64,
                        ) || self
                            .rules
                            .pressure_threshold
                            .is_some_and(|threshold| pressure(parent, member.id) >= threshold))
                        && busy.binary_search(&parent).is_err()
                    {
                        candidates[i] = Some(p);
                    }
                }
                if tick.wrapping_add(member.id) & 1 != 0 {
                    candidates.swap(0, 1);
                }
                Some(Child {
                    id: member.id,
                    rank: (
                        game_parallel_genetics::random(seed, member.id, tick),
                        member.id,
                    ),
                    first: candidates[0],
                    second: candidates[1],
                })
            })
            .collect();
        // Ordered, unique parent slots need no shared arbitration at all.
        if children.par_iter().all(|child| child.first.is_some())
            && children
                .par_windows(2)
                .all(|pair| pair[0].first < pair[1].first)
        {
            let events = children
                .par_iter()
                .map(|child| Event {
                    tick,
                    child: child.id,
                    caregiver: child.first.map(|slot| people[slot].0),
                })
                .collect();
            let actors = children
                .par_iter()
                .map(|child| people[child.first.unwrap()].0)
                .collect();
            return Draft { events, actors };
        }
        let first = minima(&children, people.len(), |child| child.first);
        let accepted: Vec<_> = children
            .par_iter()
            .map(|child| child.first.filter(|&p| first.get(p) == Some(child.rank)))
            .collect();
        let needs_second = children
            .par_iter()
            .zip(accepted.par_iter())
            .any(|(child, parent)| parent.is_none() && child.second.is_some());
        let second = needs_second.then(|| {
            minima(&children, people.len(), |child| {
                let p = child.second?;
                (child
                    .first
                    .is_none_or(|first_parent| first.get(first_parent) != Some(child.rank))
                    && first.get(p).is_none())
                .then_some(p)
            })
        });
        let selected: Vec<_> = children
            .par_iter()
            .zip(accepted.par_iter())
            .map(|(child, &parent)| {
                parent.or_else(|| {
                    child.second.filter(|&p| {
                        second
                            .as_ref()
                            .is_some_and(|minima| minima.get(p) == Some(child.rank))
                    })
                })
            })
            .collect();
        let events = children
            .par_iter()
            .zip(selected.par_iter())
            .map(|(child, &parent)| Event {
                tick,
                child: child.id,
                caregiver: parent.map(|i| people[i].0),
            })
            .collect();
        let mut actors: Vec<_> = selected
            .into_par_iter()
            .flatten()
            .map(|i| people[i].0)
            .collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        Draft { events, actors }
    }
}
type Rank = (u64, u64);
struct Winners {
    base: usize,
    values: Vec<Option<Rank>>,
}
impl Winners {
    fn get(&self, slot: usize) -> Option<Rank> {
        slot.checked_sub(self.base)
            .and_then(|i| self.values.get(i))
            .copied()
            .flatten()
    }
}
type ClaimGroup = (usize, Vec<(usize, Rank)>);
// Candidate chunks publish sparse per-shard minima. Each person shard reduces independently.
fn minima(
    children: &[Child],
    count: usize,
    choose: impl Fn(&Child) -> Option<usize> + Sync,
) -> Winners {
    const CHUNK: usize = 1024;
    const SHARD: usize = 4096;
    let bounds = children
        .par_iter()
        .filter_map(&choose)
        .map(|slot| (slot, slot))
        .reduce_with(|a, b| (a.0.min(b.0), a.1.max(b.1)));
    let Some((first, last)) = bounds else {
        return Winners {
            base: 0,
            values: Vec::new(),
        };
    };
    if last - first < 256 {
        let width = last - first + 1;
        let values = children
            .par_iter()
            .fold(
                || vec![None::<Rank>; width],
                |mut local, child| {
                    if let Some(slot) = choose(child) {
                        let entry = &mut local[slot - first];
                        *entry = Some(entry.map_or(child.rank, |old| old.min(child.rank)));
                    }
                    local
                },
            )
            .reduce(
                || vec![None::<Rank>; width],
                |mut left, right| {
                    for (entry, value) in left.iter_mut().zip(right) {
                        if let Some(value) = value {
                            *entry = Some(entry.map_or(value, |old| old.min(value)));
                        }
                    }
                    left
                },
            );
        return Winners {
            base: first,
            values,
        };
    }
    let batches: Vec<Vec<ClaimGroup>> = children
        .par_chunks(CHUNK)
        .map(|chunk| {
            let mut claims: Vec<_> = chunk
                .iter()
                .filter_map(|child| choose(child).map(|slot| (slot, child.rank)))
                .collect();
            claims.sort_unstable();
            claims.dedup_by_key(|claim| claim.0);
            claims
                .chunk_by(|a, b| a.0 / SHARD == b.0 / SHARD)
                .map(|group| (group[0].0 / SHARD, group.to_vec()))
                .collect()
        })
        .collect();
    let mut groups: Vec<_> = batches
        .into_par_iter()
        .flat_map_iter(|batch| batch)
        .collect();
    if groups.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        groups.par_sort_unstable_by_key(|group| group.0);
    }
    let values = (0..count.div_ceil(SHARD))
        .into_par_iter()
        .flat_map_iter(|shard| {
            let base = shard * SHARD;
            let mut result = vec![None; SHARD.min(count - base)];
            let start = groups.partition_point(|group| group.0 < shard);
            let end = groups.partition_point(|group| group.0 <= shard);
            for group in &groups[start..end] {
                for &(slot, child) in &group.1 {
                    let entry: &mut Option<(u64, u64)> = &mut result[slot - base];
                    *entry = Some(entry.map_or(child, |current| current.min(child)));
                }
            }
            result
        })
        .collect();
    Winners { base: 0, values }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hot_and_sparse_caregivers_choose_the_lowest_rank_across_workers() {
        let children: Vec<_> = (0..30_000)
            .map(|id| Child {
                id: u64::MAX - id,
                rank: (id, u64::MAX - id),
                first: Some(if id % 3 == 0 {
                    9999
                } else {
                    (id as usize * 17) % 10_000
                }),
                second: None,
            })
            .collect();
        let mut expected = vec![None; 10_000];
        for child in &children {
            let slot = child.first.unwrap();
            expected[slot] =
                Some(expected[slot].map_or(child.rank, |old: (u64, u64)| old.min(child.rank)));
        }
        for workers in [1, 4, 6] {
            let actual = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| minima(&children, 10_000, |child| child.first));
            assert_eq!(actual.base, 0);
            assert_eq!(actual.values, expected);
        }
    }
    #[test]
    fn bounded_hot_parent_reduction_handles_empty_and_sparse_ranges() {
        let children: Vec<_> = (0..50000)
            .map(|id| Child {
                id,
                rank: (id ^ 12345, id),
                first: Some(if id % 3 == 0 { 901 } else { 907 }),
                second: None,
            })
            .collect();
        let mut expected = [None::<Rank>; 7];
        for child in &children {
            let slot = child.first.unwrap() - 901;
            expected[slot] = Some(expected[slot].map_or(child.rank, |old| old.min(child.rank)));
        }
        for workers in [1, 4, 6] {
            let result = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| minima(&children, 2000, |child| child.first));
            assert_eq!(result.base, 901);
            assert_eq!(result.values, expected);
            assert_eq!(result.get(900), None);
            assert_eq!(result.get(908), None);
        }
        assert!(minima(&children, 2000, |_| None).values.is_empty());
    }
    #[test]
    fn caregivers_are_optional_local_available_and_reserved_once() {
        use game_parallel_family_model::Member;
        use game_parallel_genetics::{Genome, Sex};
        let member = |id, age, parents: Vec<u64>| Member {
            id,
            born: None,
            age,
            lifespan_loss: 0,
            genome: Genome {
                sex: Sex::Female,
                alleles: [[0; 2]; 4],
            },
            parents: parents.into(),
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: false,
            reconsider_partnership: false,
        };
        let family = Store::compile(&[
            member(1, 20, vec![]),
            member(2, 20, vec![]),
            member(3, 1, vec![1, 2]),
            member(4, 2, vec![1, 2]),
        ])
        .unwrap();
        let people = vec![
            (1, Some(1), 100, 0),
            (2, Some(1), 100, 0),
            (3, Some(1), 100, 0),
            (4, Some(1), 100, 0),
        ];
        let care = DependentCare {
            inherit_births: false,
            rules: Rules {
                dependent_until: 5,
                max_caregiver_hunger: 10,
                neglect_damage: 2,
                feed_from_cargo: false,
                fetch_local_food: false,
                pressure_threshold: None,
            },
            policies: vec![
                Policy {
                    person: 1,
                    willing: true,
                },
                Policy {
                    person: 2,
                    willing: true,
                },
            ],
            events: vec![],
        };
        let both = care.prepare(&family, 12, &people, &[], (42, 1), (|_, _| true, |_, _| 0));
        let parallel = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap()
            .install(|| care.prepare(&family, 12, &people, &[], (42, 1), (|_, _| true, |_, _| 0)));
        assert_eq!(both.events, parallel.events);
        assert_eq!(both.actors, parallel.actors);
        assert_eq!(both.events.len(), 2);
        assert_eq!(both.actors, vec![1, 2]);
        assert!(both.events.iter().all(|event| event.caregiver.is_some()));
        let busy = care.prepare(&family, 12, &people, &[1], (42, 1), (|_, _| true, |_, _| 0));
        assert_eq!(busy.actors, vec![2]);
        assert_eq!(
            busy.events
                .iter()
                .filter(|event| event.caregiver.is_none())
                .count(),
            1
        );
        let none = care.prepare(
            &family,
            12,
            &people,
            &[1, 2],
            (42, 1),
            (|_, _| true, |_, _| 0),
        );
        assert!(none.actors.is_empty());
        assert!(none.events.iter().all(|event| event.caregiver.is_none()));
    }
    #[test]
    fn repeated_contention_rotates_priority_instead_of_always_starving_one_child() {
        use game_parallel_family_model::Member;
        use game_parallel_genetics::{Genome, Sex};
        let member = |id, age, parents: Vec<u64>| Member {
            id,
            born: None,
            age,
            lifespan_loss: 0,
            genome: Genome {
                sex: Sex::Female,
                alleles: [[0; 2]; 4],
            },
            parents: parents.into(),
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: false,
            reconsider_partnership: false,
        };
        let family = Store::compile(&[
            member(1, 20, vec![]),
            member(2, 20, vec![]),
            member(3, 1, vec![1, 2]),
            member(4, 1, vec![1, 2]),
            member(5, 1, vec![1, 2]),
        ])
        .unwrap();
        let people = (1..=5).map(|id| (id, Some(1), 100, 0)).collect::<Vec<_>>();
        let care = DependentCare {
            inherit_births: false,
            rules: Rules {
                dependent_until: 5,
                max_caregiver_hunger: 10,
                neglect_damage: 0,
                feed_from_cargo: false,
                fetch_local_food: false,
                pressure_threshold: None,
            },
            policies: vec![Policy {
                person: 1,
                willing: true,
            }],
            events: vec![],
        };
        let seen: HashSet<_> = (1..=100)
            .flat_map(|tick| {
                care.prepare(
                    &family,
                    12,
                    &people,
                    &[],
                    (42, tick),
                    (|_, _| true, |_, _| 0),
                )
                .events
            })
            .filter_map(|event| event.caregiver.map(|_| event.child))
            .collect();
        assert_eq!(seen, HashSet::from([3, 4, 5]));
    }
}

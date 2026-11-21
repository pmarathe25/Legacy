//! Wild seed banks and competing heritable plant populations.
use crate::genetics::Strain;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub strain: Strain,
    pub adults: u32,
    pub seeds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stand {
    pub node: u64,
    /// Local condition favored by a strain, in the same range as its alleles.
    pub habitat: u16,
    pub adult_capacity: u32,
    pub seed_capacity: u32,
    pub seed_survival: u16,
    pub seed_per_adult: u32,
    pub cohorts: Vec<Cohort>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WildPlants {
    pub period_millis: u64,
    #[serde(default)]
    pub completed_cycles: u64,
    pub stands: Vec<Stand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    pub node: u64,
    pub batch: u64,
    pub adult_deaths: u32,
    pub seed_losses: u32,
    pub established: u32,
    pub seed_production: u32,
    pub adults: u32,
    pub seeds: u32,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Cycle {
    pub at_millis: u128,
    pub changes: Vec<Change>,
}

impl WildPlants {
    pub fn validate(&self, now: u128) -> Result<(), &'static str> {
        if self.period_millis == 0
            || self.stands.is_empty()
            || self
                .stands
                .par_windows(2)
                .any(|rows| rows[0].node >= rows[1].node)
            || self.stands.par_iter().any(|stand| !stand.valid())
            || u128::from(self.completed_cycles) != now / u128::from(self.period_millis)
        {
            return Err("invalid wild plant ecology");
        }
        let mut strains: Vec<_> = self
            .stands
            .par_iter()
            .flat_map_iter(|stand| stand.cohorts.iter().map(|cohort| cohort.strain))
            .collect();
        strains.par_sort_unstable_by_key(|strain| strain.batch);
        if strains
            .par_windows(2)
            .any(|rows| rows[0].batch == rows[1].batch && rows[0].alleles != rows[1].alleles)
        {
            return Err("inconsistent wild seed batch genetics");
        }
        Ok(())
    }

    pub fn next_cycle(&self) -> Option<u128> {
        (u128::from(self.completed_cycles) + 1).checked_mul(u128::from(self.period_millis))
    }

    pub fn stand(&self, node: u64) -> Option<&Stand> {
        self.stands
            .binary_search_by_key(&node, |row| row.node)
            .ok()
            .map(|index| &self.stands[index])
    }

    /// Independent populations advance from the same opening conditions.
    pub fn prepare(
        &self,
        now: u128,
        growth_exposure: u128,
        seed: u64,
    ) -> Result<(Self, Cycle), &'static str> {
        if self.next_cycle() != Some(now) {
            return Err("wild plant ecology advanced outside its lifecycle");
        }
        let cycle = self
            .completed_cycles
            .checked_add(1)
            .ok_or("wild plant cycle overflow")?;
        let growth = (growth_exposure / u128::from(self.period_millis)).min(1000) as u32;
        let (stands, rows): (Vec<_>, Vec<_>) = self
            .stands
            .par_iter()
            .map(|stand| stand.advance(seed, cycle, growth))
            .unzip();
        let changes = rows.into_par_iter().flatten().collect();
        Ok((
            Self {
                completed_cycles: cycle,
                stands,
                period_millis: self.period_millis,
            },
            Cycle {
                at_millis: now,
                changes,
            },
        ))
    }
}

impl Stand {
    pub fn available(&self) -> u64 {
        self.cohorts.iter().map(|row| u64::from(row.seeds)).sum()
    }

    fn valid(&self) -> bool {
        self.node != 0
            && self.habitat <= 1000
            && self.adult_capacity > 0
            && self.seed_capacity > 0
            && self.seed_survival <= 1000
            && self.seed_per_adult > 0
            && !self.cohorts.is_empty()
            && self
                .cohorts
                .windows(2)
                .all(|rows| rows[0].strain.batch < rows[1].strain.batch)
            && self.cohorts.iter().all(|row| {
                row.strain.batch > 0 && row.strain.alleles.iter().all(|&value| value <= 1000)
            })
            && self
                .cohorts
                .iter()
                .map(|row| u64::from(row.adults))
                .sum::<u64>()
                <= u64::from(self.adult_capacity)
            && self.available() <= u64::from(self.seed_capacity)
    }

    fn advance(&self, seed: u64, cycle: u64, growth: u32) -> (Self, Vec<Change>) {
        let signed_rate = |row: &Cohort| -> u32 {
            let expression =
                (u32::from(row.strain.alleles[0]) + u32::from(row.strain.alleles[1])) / 2;
            1000 - expression.abs_diff(u32::from(self.habitat))
        };
        let random = |batch, phase| game_genetics::random(seed ^ self.node, cycle ^ phase, batch);
        let mut next = self.clone();
        let mut potential = Vec::with_capacity(self.cohorts.len());
        let mut production = Vec::with_capacity(self.cohorts.len());
        for row in &mut next.cohorts {
            let fitness = signed_rate(row);
            row.adults = fraction(
                row.adults,
                300 + fitness / 2,
                random(row.strain.batch, 0x11),
            );
            row.seeds = fraction(
                row.seeds,
                u32::from(self.seed_survival),
                random(row.strain.batch, 0x22),
            );
            potential.push(fraction(
                row.seeds,
                fitness * growth / 1000,
                random(row.strain.batch, 0x33),
            ));
            let seeds =
                u128::from(row.adults) * u128::from(self.seed_per_adult) * u128::from(growth)
                    / 1000;
            production.push(seeds.min(u128::from(self.seed_capacity)) as u32);
        }
        let adult_space =
            self.adult_capacity - next.cohorts.iter().map(|row| row.adults).sum::<u32>();
        distribute(&mut potential, adult_space, random(self.node, 0x44));
        for (row, &arrivals) in next.cohorts.iter_mut().zip(&potential) {
            row.adults += arrivals;
            row.seeds -= arrivals;
        }
        let seed_space = self.seed_capacity - next.available() as u32;
        distribute(&mut production, seed_space, random(self.node, 0x55));
        let changes = next
            .cohorts
            .iter_mut()
            .zip(&self.cohorts)
            .zip(potential)
            .zip(production)
            .filter_map(|(((row, previous), established), seed_production)| {
                let adult_deaths = previous.adults - (row.adults - established);
                let seed_losses = previous.seeds - (row.seeds + established);
                row.seeds += seed_production;
                (adult_deaths != 0 || seed_losses != 0 || established != 0 || seed_production != 0)
                    .then_some(Change {
                        node: self.node,
                        batch: row.strain.batch,
                        adult_deaths,
                        seed_losses,
                        established,
                        seed_production,
                        adults: row.adults,
                        seeds: row.seeds,
                    })
            })
            .collect();
        (next, changes)
    }
}

fn fraction(count: u32, per_thousand: u32, key: u64) -> u32 {
    ((u64::from(count) * u64::from(per_thousand) + key % 1000) / 1000) as u32
}

fn distribute(demand: &mut [u32], capacity: u32, key: u64) {
    let total: u128 = demand.iter().map(|&count| u128::from(count)).sum();
    let capacity = u128::from(capacity);
    if total <= capacity {
        return;
    }
    let mut prefix = u128::from(key) % total;
    for count in demand {
        let before = prefix / total;
        prefix += u128::from(*count) * capacity;
        *count = (prefix / total - before) as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locally_adapted_variants_compete_for_the_same_finite_space() {
        let stand = |node, habitat| Stand {
            node,
            habitat,
            adult_capacity: 100,
            seed_capacity: 300,
            seed_survival: 900,
            seed_per_adult: 4,
            cohorts: vec![
                Cohort {
                    strain: Strain {
                        batch: 150,
                        alleles: [0, 0],
                    },
                    adults: 50,
                    seeds: 100,
                },
                Cohort {
                    strain: Strain {
                        batch: 151,
                        alleles: [1000, 1000],
                    },
                    adults: 50,
                    seeds: 100,
                },
            ],
        };
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut owner = WildPlants {
                        period_millis: 120,
                        completed_cycles: 0,
                        stands: vec![stand(1, 0), stand(2, 1000)],
                    };
                    owner.validate(0).unwrap();
                    let mut history = Vec::new();
                    for cycle in 1..=8 {
                        let (next, events) = owner.prepare(cycle * 120, 120_000, 7).unwrap();
                        next.validate(cycle * 120).unwrap();
                        owner = next;
                        history.push(events);
                    }
                    assert!(owner.stands[0].cohorts[0].adults > 90);
                    assert_eq!(owner.stands[0].cohorts[1].adults, 0);
                    assert_eq!(owner.stands[1].cohorts[0].adults, 0);
                    assert!(owner.stands[1].cohorts[1].adults > 90);
                    (owner, history)
                })
        };
        assert_eq!(run(1), run(6));
    }

    #[test]
    fn an_empty_bank_cannot_reproduce_without_adults() {
        let stand = Stand {
            node: 1,
            habitat: 500,
            adult_capacity: 3,
            seed_capacity: 3,
            seed_survival: 1000,
            seed_per_adult: 3,
            cohorts: vec![Cohort {
                strain: Strain {
                    batch: 1,
                    alleles: [500, 500],
                },
                adults: 0,
                seeds: 0,
            }],
        };
        let (next, changes) = stand.advance(2, 1, 1000);
        assert_eq!(next, stand);
        assert!(changes.is_empty());
    }
}

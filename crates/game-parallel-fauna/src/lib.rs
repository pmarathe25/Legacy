//! Independent animals, local grazing and inherited life histories.
pub mod content;
pub mod handling;
use game_parallel_genetics::{Genome, Sex, inherit, random};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Authored biological parameters; all times are simulation boundaries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Species {
    pub id: u32,
    pub name: String,
    pub food_good: u32,
    pub intake: u64,
    pub hunger_per_tick: u32,
    pub relief_per_unit: u32,
    pub starvation_threshold: u32,
    pub starvation_damage: u32,
    pub max_health: u32,
    pub adult_age: u64,
    pub fertile_until: u64,
    pub lifespan: u64,
    pub gestation: u64,
    pub conception_one_in: u64,
    pub breeding_max_hunger: u32,
    pub newborn_health: u32,
    #[serde(default)]
    pub mutation_per_million: u32,
    #[serde(default)]
    pub mutation_step: u16,
    /// Optional response to regional seasonal exposure; absent means no modeled stress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub climate: Option<Climate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunting: Option<HuntYield>,
    /// Optional newborn temperament and capacity for individual experiences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newborn_social_response: Option<SocialResponse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newborn_handling_response: Option<handling::Response>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuntYield {
    pub good: u32,
    pub quantity: u64,
    /// Chance in a thousand that an otherwise viable pursuit succeeds.
    pub success_per_thousand: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Climate {
    pub tolerance: u32,
    /// Additional tolerance at the maximum mean value of the selected diploid locus.
    pub inherited_tolerance: u32,
    pub hardiness_locus: usize,
    pub damage_per_excess: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exposure {
    pub region: u64,
    pub severity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pregnancy {
    pub father: u64,
    pub father_genome: Genome,
    pub due: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animal {
    pub id: u64,
    pub species: u32,
    pub region: u64,
    pub age: u64,
    pub health: u32,
    pub hunger: u32,
    pub genome: Genome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pregnancy: Option<Pregnancy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parents: Option<[u64; 2]>,
    /// Private, individual response to receiving an offering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social_response: Option<SocialResponse>,
    /// This animal's independent willingness and reactions to handling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handling_response: Option<handling::Response>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opinions: Vec<AnimalOpinion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialResponse {
    pub feed_affinity: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimalOpinion {
    pub person: u64,
    pub affinity: i32,
}

/// A physical resource snapshot supplied by its owner, not a copy of stock to create.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Patch {
    pub id: u64,
    pub region: u64,
    pub good: u32,
    pub available: u64,
}

/// Food already consumed in an accepted physical transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feeding {
    pub animal: u64,
    pub person: Option<u64>,
    pub good: u32,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fauna {
    pub species: Vec<Species>,
    pub animals: Vec<Animal>,
    pub next_id: u64,
    pub last_tick: u64,
}

/// Detached results; the host must atomically apply both animal and patch changes.
pub struct Draft {
    pub fauna: Fauna,
    pub consumed: Vec<(u64, u64)>,
    pub births: Vec<u64>,
    pub deaths: Vec<u64>,
}

#[derive(Clone, Copy)]
struct Claim {
    patch: u64,
    animal: u64,
    priority: u64,
    amount: u64,
    available: u64,
}

impl Fauna {
    /// Validate a feeding's target; dietary response remains private to the animal.
    pub fn check_feeding(&self, feeding: Feeding) -> Result<u64, &'static str> {
        let animal = self
            .animals
            .iter()
            .find(|a| a.id == feeding.animal && a.health > 0)
            .ok_or("unknown living animal")?;
        if feeding.quantity == 0 || feeding.person == Some(0) {
            return Err("invalid animal food");
        }
        Ok(animal.region)
    }
    pub fn validate(
        &self,
        regions: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u64,
    ) -> Result<(), &'static str> {
        if self.last_tick != now || self.next_id == 0 {
            return Err("invalid fauna clock or identity");
        }
        let mut species: Vec<_> = self.species.par_iter().collect();
        species.par_sort_unstable_by_key(|s| s.id);
        let mut ids: Vec<_> = self.animals.par_iter().map(|a| a.id).collect();
        ids.par_sort_unstable();
        let mut pedigree: Vec<_> = if self
            .animals
            .par_iter()
            .any(|a| a.parents.is_some() || a.pregnancy.is_some())
        {
            self.animals.par_iter().collect()
        } else {
            Vec::new()
        };
        if pedigree.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            pedigree.par_sort_unstable_by_key(|animal| animal.id);
        }
        let parent = |id| {
            pedigree
                .binary_search_by_key(&id, |animal| animal.id)
                .ok()
                .map(|index| pedigree[index])
        };
        if species.par_windows(2).any(|p| p[0].id == p[1].id)
            || species.par_iter().any(|s| {
                s.id == 0
                    || s.name.is_empty()
                    || !goods.contains(&s.food_good)
                    || s.intake == 0
                    || s.relief_per_unit == 0
                    || s.max_health == 0
                    || s.starvation_threshold == 0
                    || s.adult_age == 0
                    || s.fertile_until <= s.adult_age
                    || s.lifespan <= s.fertile_until
                    || s.gestation == 0
                    || s.conception_one_in == 0
                    || s.newborn_health == 0
                    || s.newborn_health > s.max_health
                    || s.mutation_per_million > 1_000_000
                    || s.mutation_step > 1000
                    || s.climate
                        .as_ref()
                        .is_some_and(|climate| climate.hardiness_locus >= 4)
                    || s.hunting.as_ref().is_some_and(|yield_| {
                        !goods.contains(&yield_.good)
                            || yield_.quantity == 0
                            || yield_.success_per_thousand > 1000
                    })
                    || s.newborn_social_response.as_ref().is_some_and(|response| {
                        response.capacity > 64 || response.minimum > 0 || response.maximum < 0
                    })
            })
            || ids.first() == Some(&0)
            || ids.last().is_some_and(|id| *id >= self.next_id)
            || ids.par_windows(2).any(|p| p[0] == p[1])
            || self.animals.par_iter().any(|a| {
                let s = species
                    .binary_search_by_key(&a.species, |s| s.id)
                    .ok()
                    .map(|i| species[i]);
                s.is_none_or(|s| {
                    !regions.contains(&a.region)
                        || a.health > s.max_health
                        || a.genome.alleles.iter().flatten().any(|value| *value > 1000)
                        || a.social_response
                            .as_ref()
                            .map_or(!a.opinions.is_empty(), |response| {
                                response.capacity > 64
                                    || response.minimum > 0
                                    || response.maximum < 0
                                    || a.opinions.len() > response.capacity
                                    || a.opinions
                                        .windows(2)
                                        .any(|pair| pair[0].person >= pair[1].person)
                                    || a.opinions.iter().any(|opinion| {
                                        opinion.person == 0
                                            || opinion.affinity < response.minimum
                                            || opinion.affinity > response.maximum
                                    })
                            })
                        || a.parents.is_some_and(|parents| {
                            parents[0] == parents[1]
                                || parents.iter().any(|parent| *parent == 0 || *parent >= a.id)
                                || parent(parents[0]).is_none_or(|mother| {
                                    mother.species != a.species || mother.genome.sex != Sex::Female
                                })
                                || parent(parents[1]).is_none_or(|father| {
                                    father.species != a.species || father.genome.sex != Sex::Male
                                })
                        })
                        || a.pregnancy.as_ref().is_some_and(|p| {
                            a.genome.sex != Sex::Female
                                || a.health == 0
                                || p.father == a.id
                                || p.father == 0
                                || p.father >= self.next_id
                                || p.due <= now
                                || p.father_genome.sex != Sex::Male
                                || parent(p.father).is_none_or(|father| {
                                    father.species != a.species || father.genome != p.father_genome
                                })
                                || p.father_genome
                                    .alleles
                                    .iter()
                                    .flatten()
                                    .any(|value| *value > 1000)
                        })
                })
            })
        {
            return Err("invalid fauna state");
        }
        Ok(())
    }

    /// Stage one boundary from a local food snapshot. Grazing and births are deterministic
    /// across worker counts; failed staging does not mutate the animals or the patches.
    pub fn advance(&self, patches: &[Patch], seed: u64, tick: u64) -> Result<Draft, &'static str> {
        let regions = self.animals.par_iter().map(|a| a.region).collect();
        let goods = self
            .species
            .par_iter()
            .flat_map_iter(|s| {
                std::iter::once(s.food_good).chain(s.hunting.iter().map(|yield_| yield_.good))
            })
            .collect();
        self.validate(&regions, &goods, self.last_tick)?;
        self.advance_validated(patches, seed, tick)
    }

    /// The host has already validated the committed generation against its world.
    pub fn advance_validated(
        &self,
        patches: &[Patch],
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        self.advance_with_feeding(patches, &[], seed, tick)
    }

    /// Apply physically accepted feedings before animals choose whether to graze.
    pub fn advance_with_feeding(
        &self,
        patches: &[Patch],
        feeding: &[Feeding],
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        self.advance_with_feeding_and_exposure(patches, feeding, &[], seed, tick)
    }

    /// Apply accepted feed and data-defined regional exposure in the same staged boundary.
    pub fn advance_with_feeding_and_exposure(
        &self,
        patches: &[Patch],
        feeding: &[Feeding],
        exposure: &[Exposure],
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        self.advance_with_hunts(patches, feeding, exposure, &[], seed, tick)
    }

    /// Accepted unique hunt receipts remove prey before grazing, breeding, or birth.
    pub fn advance_with_hunts(
        &self,
        patches: &[Patch],
        feeding: &[Feeding],
        exposure: &[Exposure],
        hunted: &[u64],
        seed: u64,
        tick: u64,
    ) -> Result<Draft, &'static str> {
        if self.last_tick.checked_add(1) != Some(tick) {
            return Err("fauna phase must advance one tick");
        }
        let mut hunted = hunted.to_vec();
        hunted.par_sort_unstable();
        if hunted.first() == Some(&0)
            || hunted.par_windows(2).any(|pair| pair[0] == pair[1])
            || feeding
                .par_iter()
                .any(|food| hunted.binary_search(&food.animal).is_ok())
        {
            return Err("invalid or simultaneously fed hunted animal");
        }
        if !hunted.is_empty() {
            let mut living: Vec<_> = self
                .animals
                .par_iter()
                .filter(|animal| animal.health > 0)
                .map(|animal| animal.id)
                .collect();
            living.par_sort_unstable();
            if hunted
                .par_iter()
                .any(|id| living.binary_search(id).is_err())
            {
                return Err("unknown living hunted animal");
            }
        }
        let mut exposure = exposure.to_vec();
        exposure.par_sort_unstable_by_key(|row| row.region);
        if exposure.first().is_some_and(|row| row.region == 0)
            || exposure
                .par_windows(2)
                .any(|pair| pair[0].region == pair[1].region)
        {
            return Err("invalid animal exposure");
        }
        let mut species: Vec<_> = self.species.par_iter().collect();
        species.par_sort_unstable_by_key(|s| s.id);
        let mut patches: Vec<_> = patches.par_iter().copied().collect();
        patches.par_sort_unstable_by_key(|p| (p.region, p.good, p.id));
        let mut patch_ids: Vec<_> = patches.par_iter().map(|p| p.id).collect();
        patch_ids.par_sort_unstable();
        if patch_ids.first() == Some(&0) || patch_ids.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("invalid fauna food patches");
        }
        let lookup = |id| {
            species
                .binary_search_by_key(&id, |s| s.id)
                .ok()
                .map(|i| species[i])
        };
        let classify =
            |food: &Feeding, animal: Option<&Animal>| -> Result<Option<(u64, u64)>, &'static str> {
                let animal = animal
                    .filter(|a| a.health > 0)
                    .ok_or("unknown living animal")?;
                let species = lookup(animal.species).ok_or("unknown animal species")?;
                if food.quantity == 0 || food.person == Some(0) {
                    return Err("invalid animal food");
                }
                Ok((food.good == species.food_good).then_some((food.animal, food.quantity)))
            };
        let supplied = if feeding.len() <= 1 {
            feeding
                .iter()
                .map(|food| classify(food, self.animals.iter().find(|a| a.id == food.animal)))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut animals: Vec<_> = self.animals.par_iter().collect();
            if animals.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
                animals.par_sort_unstable_by_key(|animal| animal.id);
            }
            feeding
                .par_iter()
                .map(|food| {
                    let animal = animals
                        .binary_search_by_key(&food.animal, |a| a.id)
                        .ok()
                        .map(|i| animals[i]);
                    classify(food, animal)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut supplied: Vec<_> = supplied.into_par_iter().flatten().collect();
        supplied.par_sort_unstable_by_key(|row| row.0);
        let supplied: Vec<_> = supplied
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let total: u128 = group.par_iter().map(|row| u128::from(row.1)).sum();
                (group[0].0, total.min(u128::from(u64::MAX)) as u64)
            })
            .collect();
        let before_grazing = |animal: &Animal, species: &Species| {
            let amount = supplied
                .binary_search_by_key(&animal.id, |row| row.0)
                .ok()
                .map_or(0, |i| supplied[i].1);
            let relief = (u128::from(amount) * u128::from(species.relief_per_unit))
                .min(u128::from(u32::MAX)) as u32;
            animal
                .hunger
                .saturating_sub(relief)
                .saturating_add(species.hunger_per_tick)
        };
        let mut claims: Vec<_> = self
            .animals
            .par_iter()
            .filter_map(|a| {
                if a.health == 0 || hunted.binary_search(&a.id).is_ok() {
                    return None;
                }
                let s = lookup(a.species)?;
                let hunger = before_grazing(a, s);
                if hunger == 0 {
                    return None;
                }
                let start =
                    patches.partition_point(|p| (p.region, p.good) < (a.region, s.food_good));
                let end =
                    patches.partition_point(|p| (p.region, p.good) <= (a.region, s.food_good));
                if start == end {
                    return None;
                }
                let offset =
                    (random(seed ^ 0x6772617a65, a.id, tick) % (end - start) as u64) as usize;
                let patch = patches[start + offset];
                Some(Claim {
                    patch: patch.id,
                    animal: a.id,
                    priority: random(seed ^ 0x666f72616765, a.id, tick),
                    amount: s
                        .intake
                        .min(u64::from(hunger).div_ceil(u64::from(s.relief_per_unit))),
                    available: patch.available,
                })
            })
            .collect();
        claims.par_sort_unstable_by_key(|c| (c.patch, c.priority, c.animal));
        let mut granted: Vec<_> = claims
            .par_chunk_by(|a, b| a.patch == b.patch)
            .flat_map(|group| {
                let amounts: Vec<_> = group.par_iter().map(|c| c.amount).collect();
                let prefixes = game_parallel_lot_allocation::prefixes(&amounts);
                prefixes
                    .into_par_iter()
                    .zip(group.par_iter())
                    .filter_map(|(before, claim)| {
                        let remaining = u128::from(claim.available).saturating_sub(before);
                        let eaten = remaining.min(u128::from(claim.amount)) as u64;
                        (eaten > 0).then_some((claim.patch, claim.animal, eaten))
                    })
            })
            .collect();
        granted.par_sort_unstable_by_key(|row| (row.0, row.1));
        let mut eaten: Vec<_> = granted.par_iter().map(|row| (row.1, row.2)).collect();
        eaten.par_sort_unstable_by_key(|row| row.0);
        let mut consumed: Vec<_> = granted
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| (group[0].0, group.par_iter().map(|row| row.2).sum()))
            .collect();
        consumed.par_sort_unstable_by_key(|row| row.0);
        let mut meetings: Vec<_> = if self
            .animals
            .par_iter()
            .any(|animal| animal.social_response.is_some())
        {
            feeding
                .par_iter()
                .filter_map(|food| food.person.map(|person| (food.animal, person)))
                .collect()
        } else {
            Vec::new()
        };
        if meetings.par_windows(2).any(|p| p[0] > p[1]) {
            meetings.par_sort_unstable();
        }
        let meetings: Vec<_> = meetings
            .par_chunk_by(|a, b| a == b)
            .map(|group| (group[0].0, group[0].1, group.len() as u64))
            .collect();
        let mut animals: Vec<_> = self
            .animals
            .par_iter()
            .map(|a| -> Result<_, &'static str> {
                let mut next = a.clone();
                if a.health == 0 {
                    return Ok(next);
                }
                if hunted.binary_search(&a.id).is_ok() {
                    next.health = 0;
                    next.pregnancy = None;
                    return Ok(next);
                }
                if let Some(response) = &a.social_response {
                    let start = meetings.partition_point(|row| row.0 < a.id);
                    let end = meetings.partition_point(|row| row.0 <= a.id);
                    let local = &meetings[start..end];
                    let adjust = |value: i32, count: u64| {
                        (i128::from(value) + i128::from(response.feed_affinity) * i128::from(count))
                            .clamp(i128::from(response.minimum), i128::from(response.maximum))
                            as i32
                    };
                    for opinion in &mut next.opinions {
                        if let Ok(i) = local.binary_search_by_key(&opinion.person, |row| row.1) {
                            opinion.affinity = adjust(opinion.affinity, local[i].2);
                        }
                    }
                    if response.feed_affinity != 0 {
                        for &(_, person, count) in local
                            .iter()
                            .take(response.capacity.saturating_add(a.opinions.len()))
                        {
                            if next.opinions.len() == response.capacity {
                                break;
                            }
                            if next
                                .opinions
                                .binary_search_by_key(&person, |opinion| opinion.person)
                                .is_err()
                            {
                                next.opinions.push(AnimalOpinion {
                                    person,
                                    affinity: adjust(0, count),
                                });
                                next.opinions.sort_unstable_by_key(|opinion| opinion.person);
                            }
                        }
                    }
                }
                let s = lookup(a.species).ok_or("unknown animal species")?;
                next.age = a.age.checked_add(1).ok_or("animal age overflow")?;
                let food = eaten
                    .binary_search_by_key(&a.id, |row| row.0)
                    .ok()
                    .map_or(0, |i| eaten[i].1);
                next.hunger = before_grazing(a, s).saturating_sub(
                    (u128::from(food) * u128::from(s.relief_per_unit)).min(u128::from(u32::MAX))
                        as u32,
                );
                if next.age >= s.lifespan {
                    next.health = 0;
                } else if next.hunger >= s.starvation_threshold {
                    next.health = next.health.saturating_sub(s.starvation_damage);
                }
                if let Some(climate) = &s.climate {
                    let severity = exposure
                        .binary_search_by_key(&a.region, |row| row.region)
                        .ok()
                        .map_or(0, |i| exposure[i].severity);
                    let alleles = a
                        .genome
                        .alleles
                        .get(climate.hardiness_locus)
                        .ok_or("invalid climate hardiness locus")?;
                    let mean = (u32::from(alleles[0]) + u32::from(alleles[1])) / 2;
                    let inherited = (u64::from(mean) * u64::from(climate.inherited_tolerance)
                        / 1000)
                        .min(u64::from(u32::MAX)) as u32;
                    let excess =
                        severity.saturating_sub(climate.tolerance.saturating_add(inherited));
                    next.health = next
                        .health
                        .saturating_sub(excess.saturating_mul(climate.damage_per_excess));
                }
                if next.health == 0 {
                    next.pregnancy = None;
                }
                Ok(next)
            })
            .collect::<Result<_, _>>()?;
        let mut deaths: Vec<_> = animals
            .par_iter()
            .zip(self.animals.par_iter())
            .filter_map(|(next, old)| (old.health > 0 && next.health == 0).then_some(next.id))
            .collect();
        deaths.par_sort_unstable();
        if animals.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            animals.par_sort_unstable_by_key(|a| a.id);
        }
        let mut mothers: Vec<_> = animals
            .par_iter()
            .filter(|a| a.health > 0 && a.pregnancy.as_ref().is_some_and(|p| p.due <= tick))
            .map(|a| a.id)
            .collect();
        mothers.par_sort_unstable();
        let next_id = self
            .next_id
            .checked_add(mothers.len() as u64)
            .ok_or("animal identity overflow")?;
        let births: Vec<_> = mothers
            .par_iter()
            .enumerate()
            .map(|(index, &id)| -> Result<_, &'static str> {
                let mother = &animals[animals.binary_search_by_key(&id, |a| a.id).expect("mother")];
                let pregnancy = mother.pregnancy.as_ref().expect("pregnant mother");
                let s = lookup(mother.species).ok_or("unknown animal species")?;
                Ok(Animal {
                    id: self.next_id + index as u64,
                    species: mother.species,
                    region: mother.region,
                    age: 0,
                    health: s.newborn_health,
                    hunger: 0,
                    genome: inherit(
                        &mother.genome,
                        &pregnancy.father_genome,
                        seed,
                        mother.id,
                        tick,
                        s.mutation_per_million,
                        s.mutation_step,
                    ),
                    pregnancy: None,
                    parents: Some([mother.id, pregnancy.father]),
                    social_response: s.newborn_social_response.clone(),
                    handling_response: s.newborn_handling_response.clone(),
                    opinions: Vec::new(),
                })
            })
            .collect::<Result<_, _>>()?;
        if !mothers.is_empty() {
            animals.par_iter_mut().for_each(|a| {
                if mothers.binary_search(&a.id).is_ok() {
                    a.pregnancy = None;
                }
            });
        }
        let breeding = animals.par_iter().any(|a| {
            a.health > 0
                && a.genome.sex == Sex::Female
                && a.pregnancy.is_none()
                && mothers.binary_search(&a.id).is_err()
                && lookup(a.species).is_some_and(|s| {
                    a.age >= s.adult_age
                        && a.age < s.fertile_until
                        && a.hunger <= s.breeding_max_hunger
                        && random(seed ^ 0x6272656564, a.id, tick)
                            .is_multiple_of(s.conception_one_in)
                })
        });
        let mut males: Vec<_> = if breeding {
            animals
                .par_iter()
                .filter(|a| a.health > 0 && a.genome.sex == Sex::Male)
                .filter(|a| {
                    lookup(a.species).is_some_and(|s| {
                        a.age >= s.adult_age
                            && a.age < s.fertile_until
                            && a.hunger <= s.breeding_max_hunger
                    })
                })
                .map(|a| (a.region, a.species, a.id, a.genome.clone()))
                .collect()
        } else {
            Vec::new()
        };
        males.par_sort_unstable_by_key(|a| (a.0, a.1, a.2));
        if !males.is_empty() {
            animals
                .par_iter_mut()
                .try_for_each(|a| -> Result<(), &'static str> {
                    let Some(s) = lookup(a.species) else {
                        return Err("unknown animal species");
                    };
                    if a.health == 0
                        || a.genome.sex != Sex::Female
                        || a.pregnancy.is_some()
                        || mothers.binary_search(&a.id).is_ok()
                        || a.age < s.adult_age
                        || a.age >= s.fertile_until
                        || a.hunger > s.breeding_max_hunger
                        || !random(seed ^ 0x6272656564, a.id, tick)
                            .is_multiple_of(s.conception_one_in)
                    {
                        return Ok(());
                    }
                    let first = males.partition_point(|m| (m.0, m.1) < (a.region, a.species));
                    let end = males.partition_point(|m| (m.0, m.1) <= (a.region, a.species));
                    if first == end {
                        return Ok(());
                    }
                    let offset =
                        (random(seed ^ 0x6d617465, a.id, tick) % (end - first) as u64) as usize;
                    let father = &males[first + offset];
                    a.pregnancy = Some(Pregnancy {
                        father: father.2,
                        father_genome: father.3.clone(),
                        due: tick
                            .checked_add(s.gestation)
                            .ok_or("animal gestation overflow")?,
                    });
                    Ok(())
                })?;
        }
        let birth_ids = births.par_iter().map(|a| a.id).collect();
        animals.par_extend(births);
        Ok(Draft {
            fauna: Fauna {
                species: self.species.clone(),
                animals,
                next_id,
                last_tick: tick,
            },
            consumed,
            births: birth_ids,
            deaths,
        })
    }
}

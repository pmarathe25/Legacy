//! Deterministic offspring for physically planted seed cohorts.
use game_lots::Lot;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

fn is_false(value: &bool) -> bool {
    !*value
}

/// Stable physical-allocation owner for cultivated seed lineages.
pub const BATCH_OWNER: u32 = 0x5345_4544;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reproduction {
    /// Reproductive seed units per thousand units actually planted.
    pub seed_per_thousand: u32,
    pub mutation_per_million: u32,
    pub mutation_step: u16,
    /// Permit pollen from actual planted cohorts in the same field.
    #[serde(default, skip_serializing_if = "is_false")]
    pub recombine: bool,
    /// Permit compatible ripe fields in the same region to contribute pollen.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cross_field_pollen: bool,
}
impl Reproduction {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.mutation_per_million > 1_000_000
            || self.mutation_step > 1000
            || (self.cross_field_pollen && !self.recombine)
        {
            return Err("invalid seed reproduction rules");
        }
        Ok(())
    }
}

/// A directed path with a locally authored degree of pollen exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollenCorridor {
    pub source: u64,
    pub destination: u64,
    pub strength: u32,
}

/// Mature physical seed output; the batch identity is reserved in its boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offspring {
    pub batch: u64,
    pub parent: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pollen_parent: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub recombined: bool,
    /// Unknown parental genetics remain unknown, rather than being fabricated.
    pub alleles: Option<[u16; 2]>,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub parent: Option<u64>,
    pub pollen_parent: Option<u64>,
    pub recombined: bool,
    pub alleles: Option<[u16; 2]>,
    pub quantity: u64,
}

/// Derive one representative offspring genotype per physical cohort.
/// Randomness is tied to the actual field, planting time, and cohort position;
/// retrying a harvest never changes a result already saved at maturation.
pub(crate) fn candidates(
    rules: Reproduction,
    genetics: &crate::genetics::Genetics,
    seeds: &[Lot],
    world_seed: u64,
    field: u64,
    planted_at: u64,
) -> Result<Vec<Candidate>, String> {
    candidates_with_pollen(rules, genetics, seeds, None, world_seed, field, planted_at)
}

struct PollenPool {
    key: (u64, u32),
    seeds: Vec<Lot>,
    offsets: Vec<u128>,
}

impl PollenPool {
    fn total(&self) -> u128 {
        self.offsets.last().zip(self.seeds.last()).map_or(0, |(offset, lot)| {
            *offset + u128::from(lot.quantity)
        })
    }

    fn at(&self, ticket: u128) -> &Lot {
        let index = self.offsets.partition_point(|offset| *offset <= ticket).saturating_sub(1);
        &self.seeds[index]
    }
}

struct PollenSources<'a> {
    sources: Vec<(u128, &'a PollenPool, u32)>,
    total: u128,
}

impl<'a> PollenSources<'a> {
    fn from_corridors(
        local: &'a PollenPool,
        all: &'a [PollenPool],
        corridors: &[PollenCorridor],
    ) -> Result<Option<Self>, String> {
        let (region, crop) = local.key;
        let start = corridors.partition_point(|row| row.destination < region);
        let end = corridors.partition_point(|row| row.destination <= region);
        if start == end {
            return Ok(None);
        }
        let sources = corridors[start..end].iter()
            .filter_map(|row| {
                let index = all.binary_search_by_key(&(row.source, crop), |pool| pool.key).ok()?;
                Some((&all[index], row.strength))
            });
        let mut mixture = Self { sources: Vec::new(), total: 0 };
        for (pool, strength) in std::iter::once((local, 1000)).chain(sources) {
            let amount = pool.total().checked_mul(u128::from(strength)).ok_or("pollen weight overflow")?;
            if amount > 0 {
                mixture.sources.push((mixture.total, pool, strength));
                mixture.total = mixture.total.checked_add(amount).ok_or("pollen weight overflow")?;
            }
        }
        Ok((mixture.sources.len() > 1).then_some(mixture))
    }

    fn at(&self, ticket: u128) -> &Lot {
        let index = self.sources.partition_point(|(offset, _, _)| *offset <= ticket) - 1;
        let (offset, pool, strength) = self.sources[index];
        pool.at((ticket - offset) / u128::from(strength))
    }
}

#[derive(Clone, Copy)]
enum Pollen<'a> {
    Shared(&'a PollenPool),
    Routed(&'a PollenSources<'a>),
}

impl<'a> Pollen<'a> {
    fn total(self) -> u128 {
        match self {
            Self::Shared(pool) => pool.total(),
            Self::Routed(sources) => sources.total,
        }
    }

    fn at(self, ticket: u128) -> &'a Lot {
        match self {
            Self::Shared(pool) => pool.at(ticket),
            Self::Routed(sources) => sources.at(ticket),
        }
    }
}

fn candidates_with_pollen(
    rules: Reproduction,
    genetics: &crate::genetics::Genetics,
    seeds: &[Lot],
    pollen: Option<Pollen<'_>>,
    world_seed: u64,
    field: u64,
    planted_at: u64,
) -> Result<Vec<Candidate>, String> {
    rules.validate().map_err(str::to_owned)?;
    let local_offsets = if pollen.is_none() && rules.recombine && seeds.len() > 1 {
        let quantities: Vec<_> = if seeds.len() < 256 {
            seeds.iter().map(|lot| lot.quantity).collect()
        } else {
            seeds.par_iter().map(|lot| lot.quantity).collect()
        };
        game_lot_planning::prefixes(&quantities)
    } else {
        Vec::new()
    };
    let total = pollen.map_or_else(
        || local_offsets.last().zip(seeds.last()).map_or(0, |(offset, lot)| *offset + u128::from(lot.quantity)),
        Pollen::total,
    );
    let derive = |(index, lot, quantity): (usize, &Lot, u128)| {
        let quantity = u64::try_from(quantity).map_err(|_| "offspring seed quantity overflow")?;
        let maternal = lot.batch.and_then(|batch| genetics.alleles(batch));
        let pollen = if rules.recombine && pollen.is_none() && seeds.len() == 1 && seeds[0].quantity > 0 {
            Some(&seeds[0])
        } else if rules.recombine && total > 0 {
            let a = game_genetics::random(
                world_seed ^ 0x706f_6c6c_656e_0001 ^ index as u64,
                field,
                planted_at,
            );
            let b = game_genetics::random(
                world_seed ^ 0x706f_6c6c_656e_0002 ^ index as u64,
                field,
                planted_at,
            );
            let ticket = ((u128::from(a) << 64) | u128::from(b)) % total;
            Some(pollen.map_or_else(
                || &seeds[local_offsets.partition_point(|offset| *offset <= ticket).saturating_sub(1)],
                |pool| pool.at(ticket),
            ))
        } else {
            None
        };
        let alleles = if let Some(pollen) = pollen {
            maternal
                .zip(pollen.batch.and_then(|batch| genetics.alleles(batch)))
                .map(|(mother, father)| {
                    let draw = game_genetics::random(
                        world_seed ^ 0x6761_6d65_7465_0000 ^ index as u64,
                        field,
                        planted_at,
                    );
                    [
                        mother[(draw & 1) as usize],
                        father[((draw >> 1) & 1) as usize],
                    ]
                })
        } else {
            maternal
        };
        let alleles = alleles.map(|mut alleles| {
            for (side, allele) in alleles.iter_mut().enumerate() {
                let draw = game_genetics::random(
                    world_seed ^ (index as u64).rotate_left(17) ^ (side as u64).rotate_left(31),
                    field,
                    planted_at,
                );
                if draw % 1_000_000 < u64::from(rules.mutation_per_million) {
                    *allele = if draw & (1 << 32) == 0 {
                        allele.saturating_sub(rules.mutation_step)
                    } else {
                        allele.saturating_add(rules.mutation_step).min(1000)
                    };
                }
            }
            alleles
        });
        Ok(Candidate {
            parent: lot.batch,
            pollen_parent: pollen.and_then(|row| row.batch),
            recombined: pollen.is_some(),
            alleles,
            quantity,
        })
    };
    if seeds.len() < 256 {
        seeds
            .iter()
            .enumerate()
            .filter_map(|(index, lot)| {
                let quantity =
                    u128::from(lot.quantity) * u128::from(rules.seed_per_thousand) / 1000;
                (quantity > 0).then_some((index, lot, quantity))
            })
            .map(derive)
            .collect()
    } else {
        seeds
            .par_iter()
            .enumerate()
            .filter_map(|(index, lot)| {
                let quantity =
                    u128::from(lot.quantity) * u128::from(rules.seed_per_thousand) / 1000;
                (quantity > 0).then_some((index, lot, quantity))
            })
            .map(derive)
            .collect()
    }
}

/// Unpublished maturation work prepared from one immutable opening generation.
pub struct MaturationDraft {
    entries: Vec<(u64, Vec<Candidate>)>,
    count: u64,
}
impl MaturationDraft {
    pub fn count(&self) -> u64 {
        self.count
    }
}

pub(crate) struct Maturations {
    pub rows: Vec<(u64, Vec<Offspring>)>,
    pub descendants: Vec<crate::genetics::Descendant>,
}

pub(crate) fn prepare_maturation(
    fields: &[crate::Field],
    provenance: &crate::provenance::Provenance,
    world_seed: u64,
) -> Result<Option<MaturationDraft>, String> {
    use rayon::prelude::*;
    let Some((genetics, rules)) = provenance
        .genetics
        .as_ref()
        .and_then(|genetics| genetics.reproduction.map(|rules| (genetics, rules)))
    else {
        return Ok(None);
    };
    let mut phases: Vec<_> = fields
        .par_iter()
        .map(|field| (field.id, field.phase))
        .collect();
    phases.par_sort_unstable_by_key(|row| row.0);
    if !provenance.plantings.par_iter().any(|record| {
        record.offspring.is_none()
            && phases
                .binary_search_by_key(&record.field, |row| row.0)
                .ok()
                .is_some_and(|index| phases[index].1 == crate::Phase::Ripe)
    }) {
        return Ok(None);
    }
    // One immutable, quantity-weighted pollen pool per region and crop. This
    // avoids comparing every field to every other field in crowded regions.
    let pools: Vec<PollenPool> = if rules.cross_field_pollen {
        let mut donors: Vec<_> = fields
            .par_iter()
            .filter(|field| field.phase == crate::Phase::Ripe)
            .flat_map_iter(|field| {
                provenance
                    .plantings
                    .binary_search_by_key(&field.id, |row| row.field)
                    .ok()
                    .into_iter()
                    .flat_map(move |index| {
                        provenance.plantings[index].seeds.iter().enumerate().map(
                            move |(position, lot)| {
                                ((field.region, field.crop), field.id, position, lot.clone())
                            },
                        )
                    })
            })
            .collect();
        donors.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
        donors
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let seeds: Vec<_> = group.par_iter().map(|row| row.3.clone()).collect();
                let quantities: Vec<_> = seeds.par_iter().map(|lot| lot.quantity).collect();
                PollenPool {
                    key: group[0].0,
                    seeds,
                    offsets: game_lot_planning::prefixes(&quantities),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let routed: Vec<_> = if genetics.pollen_corridors.is_empty() {
        Vec::new()
    } else {
        pools.par_iter()
            .map(|pool| PollenSources::from_corridors(pool, &pools, &genetics.pollen_corridors))
            .collect::<Result<_, _>>()?
    };
    let mut locations: Vec<_> = if rules.cross_field_pollen {
        fields
            .par_iter()
            .map(|field| (field.id, (field.region, field.crop)))
            .collect()
    } else {
        Vec::new()
    };
    locations.par_sort_unstable_by_key(|row| row.0);
    let mut entries: Vec<_> = provenance
        .plantings
        .par_iter()
        .filter_map(|record| {
            let phase = phases
                .binary_search_by_key(&record.field, |row| row.0)
                .ok()
                .map(|index| phases[index].1);
            (phase == Some(crate::Phase::Ripe) && record.offspring.is_none()).then_some(record)
        })
        .map(|record| {
            let pool = locations
                .binary_search_by_key(&record.field, |row| row.0)
                .ok()
                .and_then(|index| {
                    pools
                        .binary_search_by_key(&locations[index].1, |row| row.key)
                        .ok()
                })
                .map(|index| {
                    routed.get(index).and_then(Option::as_ref)
                        .map_or(Pollen::Shared(&pools[index]), Pollen::Routed)
                });
            if let Some(pool) = pool {
                candidates_with_pollen(
                    rules,
                    genetics,
                    &record.seeds,
                    Some(pool),
                    world_seed,
                    record.field,
                    record.reproduction_key(),
                )
            } else {
                candidates(
                    rules,
                    genetics,
                    &record.seeds,
                    world_seed,
                    record.field,
                    record.reproduction_key(),
                )
            }
            .map(|children| (record.field, children))
        })
        .collect::<Result<_, _>>()?;
    if entries.is_empty() {
        return Ok(None);
    }
    entries.par_sort_unstable_by_key(|row| row.0);
    let count = entries
        .par_iter()
        .map(|row| row.1.len() as u128)
        .sum::<u128>();
    Ok(Some(MaturationDraft {
        entries,
        count: u64::try_from(count).map_err(|_| "too many seed lineages")?,
    }))
}

pub(crate) fn assign(draft: MaturationDraft, start: Option<u64>) -> Result<Maturations, String> {
    use rayon::prelude::*;
    let first = if draft.count == 0 {
        0
    } else {
        start
            .filter(|id| *id != 0)
            .ok_or("missing seed batch allocation")?
    };
    if draft.count > 0 {
        first
            .checked_add(draft.count)
            .ok_or("seed batch identity overflow")?;
    }
    let counts: Vec<_> = draft
        .entries
        .par_iter()
        .map(|row| row.1.len() as u64)
        .collect();
    let offsets = game_lot_planning::prefixes(&counts);
    let rows: Vec<_> = draft
        .entries
        .into_par_iter()
        .zip(offsets.into_par_iter())
        .map(|((field, candidates), offset)| {
            let offspring = candidates
                .into_iter()
                .enumerate()
                .map(|(index, candidate)| Offspring {
                    batch: first + offset as u64 + index as u64,
                    parent: candidate.parent,
                    pollen_parent: candidate.pollen_parent,
                    recombined: candidate.recombined,
                    alleles: candidate.alleles,
                    quantity: candidate.quantity,
                })
                .collect();
            (field, offspring)
        })
        .collect();
    let descendants = rows
        .par_iter()
        .flat_map_iter(|(_, offspring): &(u64, Vec<Offspring>)| {
            offspring.iter().map(|row| crate::genetics::Descendant {
                batch: row.batch,
                parent: row.parent,
                pollen_parent: row.pollen_parent,
                recombined: row.recombined,
                alleles: row.alleles,
            })
        })
        .collect();
    Ok(Maturations { rows, descendants })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genetics::{Genetics, Strain};
    use std::sync::Arc;

    #[test]
    fn clonal_mutation_is_stable_and_unknown_parent_stays_unknown() {
        let model = Genetics {
            growth_weight: 0,
            min_factor: 1000,
            max_factor: 1000,
            season_effects: vec![],
            strains: Arc::new(vec![Strain {
                batch: 11,
                alleles: [500, 500],
            }]),
            descendants: Arc::default(),
            reproduction: None,
            pollen_corridors: Vec::new(),
        };
        let rules = Reproduction {
            seed_per_thousand: 1000,
            mutation_per_million: 1_000_000,
            mutation_step: 100,
            recombine: false,
            cross_field_pollen: false,
        };
        let seeds = vec![
            Lot {
                good: 1,
                quantity: 10,
                batch: Some(11),
                item: None,
                freshness: Default::default(),
                expires_at: None,
            },
            Lot {
                good: 1,
                quantity: 4,
                batch: Some(99),
                item: None,
                freshness: Default::default(),
                expires_at: None,
            },
        ];
        let first = candidates(rules, &model, &seeds, 7, 2, 3).unwrap();
        assert_eq!(first, candidates(rules, &model, &seeds, 7, 2, 3).unwrap());
        assert_ne!(first[0].alleles, Some([500, 500]));
        assert_eq!(first[1].alleles, None);
        assert_eq!((first[0].quantity, first[1].quantity), (10, 4));
        assert!(
            Reproduction {
                mutation_per_million: 1_000_001,
                ..rules
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn recombination_uses_actual_pollen_and_keeps_unknown_genetics_unknown() {
        let model = Genetics {
            growth_weight: 0,
            min_factor: 1000,
            max_factor: 1000,
            season_effects: vec![],
            strains: Arc::new(vec![
                Strain {
                    batch: 11,
                    alleles: [0, 0],
                },
                Strain {
                    batch: 12,
                    alleles: [1000, 1000],
                },
            ]),
            descendants: Arc::default(),
            reproduction: None,
            pollen_corridors: Vec::new(),
        };
        let rules = Reproduction {
            seed_per_thousand: 1000,
            mutation_per_million: 0,
            mutation_step: 0,
            recombine: true,
            cross_field_pollen: false,
        };
        let seeds = vec![
            Lot {
                good: 1,
                quantity: 1,
                batch: Some(11),
                item: None,
                freshness: Default::default(),
                expires_at: None,
            },
            Lot {
                good: 1,
                quantity: 9,
                batch: Some(12),
                item: None,
                freshness: Default::default(),
                expires_at: None,
            },
        ];
        let draw = (0..100)
            .find(|&seed| {
                candidates(rules, &model, &seeds, seed, 1, 3).unwrap()[0].pollen_parent == Some(12)
            })
            .unwrap();
        let expected = candidates(rules, &model, &seeds, draw, 1, 3).unwrap();
        assert_eq!(expected[0].alleles, Some([0, 1000]));
        assert!(expected.iter().all(|row| row.recombined));
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            assert_eq!(
                pool.install(|| candidates(rules, &model, &seeds, draw, 1, 3).unwrap()),
                expected
            );
        }
        let mut unknown = seeds.clone();
        unknown[1].batch = Some(99);
        assert_eq!(
            candidates(rules, &model, &unknown, draw, 1, 3).unwrap()[0].alleles,
            None
        );
        let pollen_from_larger_cohort = (0..1000)
            .filter(|&seed| {
                candidates(rules, &model, &seeds, seed, 1, 3).unwrap()[0].pollen_parent == Some(12)
            })
            .count();
        assert!((840..=960).contains(&pollen_from_larger_cohort));
    }

    #[test]
    fn ripe_compatible_fields_share_real_pollen_without_crossing_regions_or_crops() {
        use crate::{
            Field, Phase,
            provenance::{Planting, Provenance},
        };
        let rules = Reproduction {
            seed_per_thousand: 1000,
            mutation_per_million: 0,
            mutation_step: 0,
            recombine: true,
            cross_field_pollen: true,
        };
        let genetics = Genetics {
            growth_weight: 0,
            min_factor: 1000,
            max_factor: 1000,
            season_effects: vec![],
            strains: Arc::new(
                (11..=15)
                    .map(|batch| Strain {
                        batch,
                        alleles: [batch as u16 * 10, batch as u16 * 10],
                    })
                    .collect(),
            ),
            descendants: Arc::default(),
            reproduction: Some(rules),
            pollen_corridors: Vec::new(),
        };
        let fields: Vec<_> = [
            (1, 1, 1, Phase::Ripe),
            (2, 1, 1, Phase::Ripe),
            (3, 2, 1, Phase::Ripe),
            (4, 1, 2, Phase::Ripe),
            (5, 1, 1, Phase::Growing),
        ]
        .into_iter()
        .map(|(id, region, crop, phase)| Field {
            id,
            region,
            crop,
            phase,
            remaining: u32::from(phase == Phase::Growing),
        })
        .collect();
        let plantings = (1..=5)
            .map(|field| Planting {
                field,
                tick: 1,
                planted_at_millis: None,
                planter: None,
                seeds: vec![Lot {
                    good: 1,
                    quantity: 10,
                    batch: Some(field + 10),
                    item: None,
                    freshness: Default::default(),
                    expires_at: None,
                }],
                offspring: None,
            })
            .collect();
        let provenance = Provenance {
            plantings: Arc::new(plantings),
            profiles: Arc::default(),
            genetics: Some(Arc::new(genetics)),
        };
        let mut routed = provenance.clone();
        let genetics = Arc::make_mut(routed.genetics.as_mut().unwrap());
        genetics.pollen_corridors = vec![PollenCorridor { source: 1, destination: 2, strength: 1000 }];
        genetics.validate().unwrap();
        let mut crossed = false;
        let mut crossed_region = false;
        for seed in 0..64 {
            let result = |workers, provenance| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap()
                    .install(|| {
                        prepare_maturation(&fields, provenance, seed)
                            .unwrap()
                            .unwrap()
                            .entries
                    })
            };
            let entries = result(1, &provenance);
            assert_eq!(entries, result(4, &provenance));
            for (field, rows) in entries {
                let parent = rows[0].pollen_parent.unwrap();
                match field {
                    1 | 2 => {
                        assert!([11, 12].contains(&parent));
                        crossed |= parent != field + 10;
                    }
                    3 | 4 => assert_eq!(parent, field + 10),
                    _ => panic!("only ripe fields mature"),
                }
            }
            let entries = result(1, &routed);
            assert_eq!(entries, result(4, &routed));
            for (field, rows) in entries {
                let parent = rows[0].pollen_parent.unwrap();
                match field {
                    1 | 2 => assert!([11, 12].contains(&parent)),
                    3 => {
                        assert!([11, 12, 13].contains(&parent));
                        crossed_region |= parent != 13;
                        assert_eq!(rows[0].alleles, Some([130, parent as u16 * 10]));
                    }
                    4 => assert_eq!(parent, 14),
                    _ => panic!("only ripe fields mature"),
                }
            }
        }
        assert!(crossed);
        assert!(crossed_region);
        assert!(
            Reproduction {
                recombine: false,
                ..rules
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    #[ignore = "concentrated within-field pollen scaling measurement"]
    fn concentrated_pollen_scaling() {
        let count = std::env::var("LEGACY_POLLEN_COHORTS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(100_000);
        let model = Genetics {
            growth_weight: 0,
            min_factor: 1000,
            max_factor: 1000,
            season_effects: vec![],
            strains: Arc::new(
                (1..=count)
                    .map(|batch| Strain {
                        batch,
                        alleles: [(batch % 1001) as u16, 500],
                    })
                    .collect(),
            ),
            descendants: Arc::default(),
            reproduction: None,
            pollen_corridors: Vec::new(),
        };
        let rules = Reproduction {
            seed_per_thousand: 1000,
            mutation_per_million: 1000,
            mutation_step: 10,
            recombine: true,
            cross_field_pollen: false,
        };
        let seeds: Vec<_> = (1..=count)
            .map(|batch| Lot {
                good: 1,
                quantity: 1,
                batch: Some(batch),
                item: None,
                freshness: Default::default(),
                expires_at: None,
            })
            .collect();
        let mut reference = None;
        for workers in [1, 2, 4, 6] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut times = Vec::new();
            for _ in 0..3 {
                let start = std::time::Instant::now();
                let result = pool.install(|| candidates(rules, &model, &seeds, 7, 1, 3).unwrap());
                times.push(start.elapsed());
                assert_eq!(result.len(), count as usize);
                if let Some(expected) = &reference {
                    assert_eq!(&result, expected);
                } else {
                    reference = Some(result);
                }
            }
            times.sort_unstable();
            eprintln!(
                "{workers} workers: {:?} median for {count} pollen cohorts",
                times[1]
            );
        }
    }
}

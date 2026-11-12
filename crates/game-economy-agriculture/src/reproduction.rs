//! Deterministic clonal offspring for physically planted seed cohorts.
use game_lots::Lot;
use serde::{Deserialize, Serialize};

/// Stable physical-allocation owner for cultivated seed lineages.
pub const BATCH_OWNER: u32 = 0x5345_4544;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reproduction {
    /// Reproductive seed units per thousand units actually planted.
    pub seed_per_thousand: u32,
    pub mutation_per_million: u32,
    pub mutation_step: u16,
}
impl Reproduction {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.mutation_per_million > 1_000_000 || self.mutation_step > 1000 {
            return Err("invalid seed reproduction rules");
        }
        Ok(())
    }
}

/// Mature physical seed output; the batch identity is reserved in its boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offspring {
    pub batch: u64,
    pub parent: Option<u64>,
    /// Unknown parental genetics remain unknown, rather than being fabricated.
    pub alleles: Option<[u16; 2]>,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub parent: Option<u64>,
    pub alleles: Option<[u16; 2]>,
    pub quantity: u64,
}

/// Derive one representative clonal offspring genotype per physical cohort.
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
    rules.validate().map_err(str::to_owned)?;
    seeds
        .iter()
        .enumerate()
        .filter_map(|(index, lot)| {
            let quantity = u128::from(lot.quantity) * u128::from(rules.seed_per_thousand) / 1000;
            if quantity == 0 {
                return None;
            }
            Some((index, lot, quantity))
        })
        .map(|(index, lot, quantity)| {
            let quantity =
                u64::try_from(quantity).map_err(|_| "offspring seed quantity overflow")?;
            let alleles = lot
                .batch
                .and_then(|batch| genetics.alleles(batch))
                .map(|mut alleles| {
                    for (side, allele) in alleles.iter_mut().enumerate() {
                        let draw = game_genetics::random(
                            world_seed
                                ^ (index as u64).rotate_left(17)
                                ^ (side as u64).rotate_left(31),
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
                alleles,
                quantity,
            })
        })
        .collect()
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
            candidates(
                rules,
                genetics,
                &record.seeds,
                world_seed,
                record.field,
                record.tick,
            )
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
            strains: Arc::new(vec![Strain {
                batch: 11,
                alleles: [500, 500],
            }]),
            descendants: Arc::default(),
            reproduction: None,
        };
        let rules = Reproduction {
            seed_per_thousand: 1000,
            mutation_per_million: 1_000_000,
            mutation_step: 100,
        };
        let seeds = vec![
            Lot {
                good: 1,
                quantity: 10,
                batch: Some(11),
                item: None,
                expires_at: None,
            },
            Lot {
                good: 1,
                quantity: 4,
                batch: Some(99),
                item: None,
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
}

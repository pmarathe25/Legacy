//! Data-defined diploid loci with sparse, individually inherited variants.
use super::{Genome, inherit, random};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locus {
    pub id: u32,
    pub name: String,
    pub default_alleles: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedAlleles {
    pub locus: u32,
    pub alleles: [u16; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitCatalog {
    pub loci: Vec<Locus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate_desire_locus: Option<u32>,
}

impl TraitCatalog {
    pub fn validate(&self) -> bool {
        let mut names = HashSet::with_capacity(self.loci.len());
        (1..=128).contains(&self.loci.len())
            && self.loci.windows(2).all(|pair| pair[0].id < pair[1].id)
            && self.loci.iter().all(|row| {
                row.id > 0
                    && !row.name.trim().is_empty()
                    && names.insert(row.name.as_str())
                    && row.default_alleles.iter().all(|&allele| allele <= 1000)
            })
            && self
                .mate_desire_locus
                .is_none_or(|locus| self.locus(locus).is_some())
    }

    pub fn validate_variants(&self, genome: &Genome) -> bool {
        genome.named_loci.as_ref().is_none_or(|rows| {
            rows.windows(2).all(|pair| pair[0].locus < pair[1].locus)
                && rows.iter().all(|row| {
                    self.locus(row.locus).is_some()
                        && row.alleles.iter().all(|&allele| allele <= 1000)
                })
        })
    }

    fn locus(&self, id: u32) -> Option<&Locus> {
        self.loci
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|index| &self.loci[index])
    }

    pub fn expression(&self, genome: &Genome, id: u32) -> Option<u16> {
        let pair = alleles(genome, self.locus(id)?);
        Some(((u32::from(pair[0]) + u32::from(pair[1])) / 2) as u16)
    }

    pub fn mate_desire(&self, genome: &Genome) -> u16 {
        self.mate_desire_locus
            .and_then(|id| self.expression(genome, id))
            .unwrap_or_else(|| genome.expressed_mate_desire())
    }
}

fn alleles(genome: &Genome, locus: &Locus) -> [u16; 2] {
    genome
        .named_loci
        .as_ref()
        .and_then(|rows| {
            rows.binary_search_by_key(&locus.id, |row| row.locus)
                .ok()
                .map(|index| rows[index].alleles)
        })
        .unwrap_or(locus.default_alleles)
}

#[derive(Clone, Copy)]
pub struct Inheritance {
    pub seed: u64,
    pub mother: u64,
    pub time: u64,
    pub mutation_per_million: u32,
    pub mutation_step: u16,
}

impl Inheritance {
    pub fn child(self, mother: &Genome, father: &Genome, catalog: Option<&TraitCatalog>) -> Genome {
        let mut child = inherit(
            mother,
            father,
            self.seed,
            self.mother,
            self.time,
            self.mutation_per_million,
            self.mutation_step,
        );
        if let Some(catalog) = catalog {
            let variants: Vec<_> = catalog
                .loci
                .iter()
                .filter_map(|locus| {
                    let pair = [mother, father].map(|genome| alleles(genome, locus));
                    let alleles =
                        std::array::from_fn(|side| self.inherit_allele(pair[side], locus.id, side));
                    (alleles != locus.default_alleles).then_some(NamedAlleles {
                        locus: locus.id,
                        alleles,
                    })
                })
                .collect();
            if !variants.is_empty() {
                child.named_loci = Some(Arc::new(variants));
            }
        }
        child
    }

    fn inherit_allele(self, alleles: [u16; 2], locus: u32, side: usize) -> u16 {
        let key = 0x4e414d45444c4f43 ^ (u64::from(locus) << 1) ^ side as u64;
        let draw = random(self.seed ^ key, self.mother, self.time);
        let allele = alleles[(draw & 1) as usize];
        if draw.rotate_left(23) % 1_000_000 >= u64::from(self.mutation_per_million) {
            allele
        } else if draw & 2 == 0 {
            allele.saturating_sub(self.mutation_step)
        } else {
            allele.saturating_add(self.mutation_step).min(1000)
        }
    }
}

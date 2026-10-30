//! Deterministic diploid inheritance for the parallel engine.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Female,
    Male,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genome {
    pub sex: Sex,
    /// Four diploid quantitative loci: fertility, vigor, sociability, longevity.
    pub alleles: [[u16; 2]; 4],
}
impl Genome {
    pub fn expressed(&self, locus: usize) -> u16 {
        ((u32::from(self.alleles[locus][0]) + u32::from(self.alleles[locus][1])) / 2) as u16
    }
}
pub fn random(seed: u64, id: u64, tick: u64) -> u64 {
    let mut n = seed ^ id.wrapping_mul(0x9e3779b97f4a7c15) ^ tick.wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}

pub fn inherit(
    mother: &Genome,
    father: &Genome,
    seed: u64,
    mother_id: u64,
    tick: u64,
    mutation_per_million: u32,
    mutation_step: u16,
) -> Genome {
    let mut alleles = [[0; 2]; 4];
    for (locus, pair) in alleles.iter_mut().enumerate() {
        for (side, parent) in [mother, father].into_iter().enumerate() {
            let n = random(
                seed ^ (locus as u64 * 2 + side as u64 + 101),
                mother_id,
                tick,
            );
            let allele = parent.alleles[locus][(n & 1) as usize];
            pair[side] = if n.rotate_left(23) % 1_000_000 < u64::from(mutation_per_million) {
                if n & 2 == 0 {
                    allele.saturating_sub(mutation_step)
                } else {
                    allele.saturating_add(mutation_step).min(1000)
                }
            } else {
                allele
            };
        }
    }
    Genome {
        sex: if random(seed ^ 73, mother_id, tick) & 1 == 0 {
            Sex::Female
        } else {
            Sex::Male
        },
        alleles,
    }
}

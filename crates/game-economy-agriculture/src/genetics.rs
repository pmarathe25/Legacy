//! Data-defined physical expression of a diploid seed growth trait.
use game_lots::Lot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strain {
    pub batch: u64,
    /// Two expressed alleles, each in the inclusive range 0..=1000.
    pub alleles: [u16; 2],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Genetics {
    /// Signed effect on duration per thousand units away from neutral 500.
    pub growth_weight: i32,
    /// Physical limits for the duration multiplier, in thousandths.
    pub min_factor: u32,
    pub max_factor: u32,
    #[serde(default)]
    pub strains: Vec<Strain>,
}

impl Genetics {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.min_factor == 0
            || self.min_factor > self.max_factor
            || self
                .strains
                .windows(2)
                .any(|rows| rows[0].batch >= rows[1].batch)
            || self
                .strains
                .iter()
                .any(|strain| strain.batch == 0 || strain.alleles.iter().any(|value| *value > 1000))
        {
            return Err("invalid seed genetics");
        }
        Ok(())
    }

    fn scaled(base: u32, factor: u32) -> u32 {
        let value = (u128::from(base) * u128::from(factor)).div_ceil(1000);
        value.clamp(1, u128::from(u32::MAX)) as u32
    }
    pub fn maximum_duration(&self, base: u32) -> u32 {
        Self::scaled(base, self.max_factor)
    }
    /// Express only the physical seed cohorts actually planted. Unknown genetic
    /// batches use neutral expression rather than inventing a known genotype.
    pub fn duration(&self, base: u32, seeds: &[Lot]) -> u32 {
        let mut sum = 0u128;
        let mut count = 0u128;
        for lot in seeds {
            let expression = lot
                .batch
                .and_then(|batch| {
                    self.strains
                        .binary_search_by_key(&batch, |strain| strain.batch)
                        .ok()
                })
                .map_or(500, |index| {
                    (u32::from(self.strains[index].alleles[0])
                        + u32::from(self.strains[index].alleles[1]))
                        / 2
                });
            sum += u128::from(expression) * u128::from(lot.quantity);
            count += u128::from(lot.quantity);
        }
        if count == 0 {
            return base;
        }
        let expression = (sum / count) as i128;
        let factor = 1000i128 + (expression - 500) * i128::from(self.growth_weight) / 1000;
        Self::scaled(
            base,
            factor.clamp(i128::from(self.min_factor), i128::from(self.max_factor)) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lot(batch: Option<u64>, quantity: u64) -> Lot {
        Lot {
            good: 1,
            quantity,
            batch,
            item: None,
            expires_at: None,
        }
    }
    #[test]
    fn physical_growth_uses_weighted_planted_genotypes_and_unknown_is_neutral() {
        let model = Genetics {
            growth_weight: 1000,
            min_factor: 500,
            max_factor: 1500,
            strains: vec![
                Strain {
                    batch: 11,
                    alleles: [0, 0],
                },
                Strain {
                    batch: 12,
                    alleles: [1000, 1000],
                },
            ],
        };
        assert!(model.validate().is_ok());
        assert_eq!(model.duration(96, &[lot(Some(11), 10)]), 48);
        assert_eq!(model.duration(96, &[lot(Some(12), 10)]), 144);
        assert_eq!(
            model.duration(96, &[lot(Some(11), 5), lot(Some(12), 5)]),
            96
        );
        assert_eq!(model.duration(96, &[lot(Some(999), 10)]), 96);
        let reversed = Genetics {
            growth_weight: -1000,
            ..model
        };
        assert_eq!(reversed.duration(96, &[lot(Some(11), 10)]), 144);
    }
}

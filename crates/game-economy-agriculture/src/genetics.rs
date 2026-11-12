//! Data-defined physical expression of a diploid seed growth trait.
use game_lots::Lot;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strain {
    pub batch: u64,
    /// Two expressed alleles, each in the inclusive range 0..=1000.
    pub alleles: [u16; 2],
}

/// A generated lineage record; unknown parent genetics remain unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descendant {
    pub batch: u64,
    pub parent: Option<u64>,
    pub alleles: Option<[u16; 2]>,
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
    pub strains: Arc<Vec<Strain>>,
    /// Generated lineages are retained in bounded immutable chunks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descendants: Arc<Vec<Arc<Vec<Descendant>>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reproduction: Option<crate::reproduction::Reproduction>,
}

impl Genetics {
    pub fn highest_batch(&self) -> Option<u64> {
        self.descendants
            .last()
            .and_then(|chunk| chunk.last())
            .map(|row| row.batch)
            .or_else(|| self.strains.last().map(|row| row.batch))
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(rules) = self.reproduction {
            rules.validate()?;
        }
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
        let mut previous = self.strains.last().map_or(0, |row| row.batch);
        for chunk in self.descendants.iter() {
            if chunk.is_empty() || chunk.len() > 1024 {
                return Err("invalid seed lineage chunks");
            }
            for row in chunk.iter() {
                if row.batch <= previous
                    || row
                        .parent
                        .is_some_and(|parent| parent == 0 || parent >= row.batch)
                    || row
                        .alleles
                        .is_some_and(|alleles| alleles.iter().any(|allele| *allele > 1000))
                {
                    return Err("invalid seed lineage");
                }
                previous = row.batch;
            }
        }
        Ok(())
    }

    pub fn alleles(&self, batch: u64) -> Option<[u16; 2]> {
        if let Ok(index) = self.strains.binary_search_by_key(&batch, |row| row.batch) {
            return Some(self.strains[index].alleles);
        }
        self.descendant(batch).and_then(|row| row.alleles)
    }

    pub(crate) fn descendant(&self, batch: u64) -> Option<Descendant> {
        let chunk = self
            .descendants
            .partition_point(|rows| rows[0].batch <= batch)
            .checked_sub(1)?;
        let rows = &self.descendants[chunk];
        rows.binary_search_by_key(&batch, |row| row.batch)
            .ok()
            .map(|index| rows[index])
    }

    /// Append monotonically allocated lineages without copying older chunks.
    pub(crate) fn with_descendants(&self, rows: &[Descendant]) -> Result<Self, &'static str> {
        if rows.is_empty() {
            return Ok(self.clone());
        }
        let highest = self
            .descendants
            .last()
            .and_then(|chunk| chunk.last())
            .map_or_else(
                || self.strains.last().map_or(0, |row| row.batch),
                |row| row.batch,
            );
        if rows[0].batch <= highest
            || rows.windows(2).any(|pair| pair[0].batch >= pair[1].batch)
            || rows.iter().any(|row| {
                row.parent
                    .is_some_and(|parent| parent == 0 || parent >= row.batch)
                    || row
                        .alleles
                        .is_some_and(|alleles| alleles.iter().any(|allele| *allele > 1000))
            })
        {
            return Err("invalid new seed lineage ordering");
        }
        let mut chunks = self.descendants.as_ref().clone();
        let mut offset = 0;
        if let Some(last) = chunks.last_mut()
            && last.len() < 1024
        {
            let count = (1024 - last.len()).min(rows.len());
            Arc::make_mut(last).extend_from_slice(&rows[..count]);
            offset = count;
        }
        chunks.extend(
            rows[offset..]
                .chunks(1024)
                .map(|part| Arc::new(part.to_vec())),
        );
        let mut next = self.clone();
        next.descendants = Arc::new(chunks);
        Ok(next)
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
                .and_then(|batch| self.alleles(batch))
                .map_or(500, |alleles| {
                    (u32::from(alleles[0]) + u32::from(alleles[1])) / 2
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

    #[test]
    fn lineage_chunks_preserve_lookup_order_and_share_old_history() {
        let model = Genetics {
            growth_weight: 0,
            min_factor: 1000,
            max_factor: 1000,
            strains: Arc::new(vec![]),
            descendants: Arc::default(),
            reproduction: None,
        };
        let records: Vec<_> = (1..=2050)
            .map(|batch| Descendant {
                batch,
                parent: None,
                alleles: Some([batch as u16 % 1001, 500]),
            })
            .collect();
        let first = model.with_descendants(&records[..2048]).unwrap();
        let second = first.with_descendants(&records[2048..]).unwrap();
        assert!(Arc::ptr_eq(&first.descendants[0], &second.descendants[0]));
        assert!(second.validate().is_ok());
        for record in &records {
            assert_eq!(second.descendant(record.batch), Some(*record));
        }
        assert!(second.with_descendants(&records[..1]).is_err());
        assert_eq!(second.descendant(2051), None);
    }
}

//! Bounded field fertility, independent of claims or institutions.
use crate::{Field, Phase};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSoil {
    pub field: u64,
    pub fertility: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Soil {
    pub capacity: u64,
    pub loss_per_harvest: u64,
    pub recovery_per_fallow: u64,
    pub fields: Vec<FieldSoil>,
}
impl Clone for Soil {
    fn clone(&self) -> Self {
        Self {
            capacity: self.capacity,
            loss_per_harvest: self.loss_per_harvest,
            recovery_per_fallow: self.recovery_per_fallow,
            fields: self.fields.par_iter().cloned().collect(),
        }
    }
}
impl Soil {
    pub fn validate(&self, fields: &[u64]) -> Result<(), String> {
        if self.capacity == 0
            || self.fields.par_windows(2).any(|p| p[0].field >= p[1].field)
            || self.fields.par_iter().any(|row| {
                row.fertility > self.capacity || fields.binary_search(&row.field).is_err()
            })
        {
            return Err("invalid field soil".into());
        }
        Ok(())
    }
    /// Unconfigured fields keep their ordinary yield; positive crops yield at least one unit.
    pub fn yield_quantity(&self, field: u64, base: u64) -> u64 {
        self.fields
            .binary_search_by_key(&field, |row| row.field)
            .ok()
            .map_or(base, |i| {
                ((u128::from(base) * u128::from(self.fields[i].fertility)
                    / u128::from(self.capacity)) as u64)
                    .max(1)
            })
    }
    pub fn advance(&self, opening: &[Field], harvested: &[u64]) -> Self {
        let mut fields: Vec<_> = opening.par_iter().collect();
        if fields.par_windows(2).any(|p| p[0].id > p[1].id) {
            fields.par_sort_unstable_by_key(|row| row.id);
        }
        let rows = self
            .fields
            .par_iter()
            .map(|row| {
                let mut next = row.clone();
                if harvested.binary_search(&row.field).is_ok() {
                    next.fertility = next.fertility.saturating_sub(self.loss_per_harvest);
                } else if fields
                    .binary_search_by_key(&row.field, |field| field.id)
                    .ok()
                    .is_some_and(|i| fields[i].phase == Phase::Fallow)
                {
                    next.fertility = next
                        .fertility
                        .saturating_add(self.recovery_per_fallow)
                        .min(self.capacity);
                }
                next
            })
            .collect();
        Self {
            capacity: self.capacity,
            loss_per_harvest: self.loss_per_harvest,
            recovery_per_fallow: self.recovery_per_fallow,
            fields: rows,
        }
    }
}

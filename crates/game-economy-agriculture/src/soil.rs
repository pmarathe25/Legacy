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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryClock {
    /// Duration represented by one recovery_per_fallow quantity.
    pub period_millis: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remainders: Vec<(u64, u64)>,
}

pub struct Recovery {
    updates: Vec<(usize, u64)>,
    remainders: Vec<(u64, u64)>,
}

impl Recovery {
    pub fn publish(self, soil: &mut Soil) {
        if !self.updates.is_empty() {
            const FIELDS_PER_SHARD: usize = 256;
            soil.fields
                .par_chunks_mut(FIELDS_PER_SHARD)
                .enumerate()
                .for_each(|(shard, fields)| {
                    let start = shard * FIELDS_PER_SHARD;
                    let first = self.updates.partition_point(|row| row.0 < start);
                    let last = self
                        .updates
                        .partition_point(|row| row.0 < start + fields.len());
                    for &(slot, fertility) in &self.updates[first..last] {
                        fields[slot - start].fertility = fertility;
                    }
                });
        }
        soil.recovery_clock
            .as_mut()
            .expect("prepared soil clock")
            .remainders = self.remainders;
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Soil {
    pub capacity: u64,
    pub loss_per_harvest: u64,
    pub recovery_per_fallow: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_clock: Option<RecoveryClock>,
    pub fields: Vec<FieldSoil>,
    #[serde(default)]
    pub amendment: Option<crate::amendment::Amendment>,
}
impl Clone for Soil {
    fn clone(&self) -> Self {
        Self {
            capacity: self.capacity,
            loss_per_harvest: self.loss_per_harvest,
            recovery_per_fallow: self.recovery_per_fallow,
            recovery_clock: self.recovery_clock.clone(),
            fields: self.fields.par_iter().cloned().collect(),
            amendment: self.amendment.clone(),
        }
    }
}
impl Soil {
    pub fn validate(&self, fields: &[u64]) -> Result<(), String> {
        if self.capacity == 0
            || self.fields.par_windows(2).any(|p| p[0].field >= p[1].field)
            || self.recovery_clock.as_ref().is_some_and(|clock| {
                clock.period_millis == 0
                    || clock
                        .remainders
                        .par_windows(2)
                        .any(|rows| rows[0].0 >= rows[1].0)
                    || clock.remainders.par_iter().any(|&(field, remainder)| {
                        remainder == 0
                            || remainder >= clock.period_millis
                            || self
                                .fields
                                .binary_search_by_key(&field, |row| row.field)
                                .is_err()
                    })
            })
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
    /// Accumulate only the time that a field was actually fallow.
    pub fn prepare_recovery(&self, opening: &[Field], millis: u64) -> Option<Recovery> {
        let clock = self.recovery_clock.as_ref()?;
        if millis == 0 || self.recovery_per_fallow == 0 {
            return None;
        }
        let mut fields: Vec<_> = opening
            .par_iter()
            .filter(|row| row.phase == Phase::Fallow)
            .map(|row| row.id)
            .collect();
        if fields.par_windows(2).any(|rows| rows[0] > rows[1]) {
            fields.par_sort_unstable();
        }
        if fields.is_empty() {
            return None;
        }
        let quantity = u128::from(millis) * u128::from(self.recovery_per_fallow);
        let period = u128::from(clock.period_millis);
        let staged: Vec<_> = self
            .fields
            .par_iter()
            .enumerate()
            .map(|(slot, row)| {
                let previous = clock
                    .remainders
                    .binary_search_by_key(&row.field, |entry| entry.0)
                    .ok()
                    .map_or(0, |i| clock.remainders[i].1);
                if fields.binary_search(&row.field).is_err() {
                    return (None, (row.field, previous));
                }
                let accrued = quantity + u128::from(previous);
                let room = u128::from(self.capacity - row.fertility);
                let added = (accrued / period).min(room);
                let remainder = if added == room {
                    0
                } else {
                    (accrued % period) as u64
                };
                let update = (added > 0).then_some((slot, row.fertility + added as u64));
                (update, (row.field, remainder))
            })
            .collect();
        let updates: Vec<_> = staged.par_iter().filter_map(|row| row.0).collect();
        let remainders: Vec<_> = staged
            .into_par_iter()
            .map(|row| row.1)
            .filter(|row| row.1 > 0)
            .collect();
        (!updates.is_empty() || remainders != clock.remainders).then_some(Recovery {
            updates,
            remainders,
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
                } else if self.recovery_clock.is_none()
                    && fields
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
            recovery_clock: self.recovery_clock.clone(),
            fields: rows,
            amendment: self.amendment.clone(),
        }
    }
}

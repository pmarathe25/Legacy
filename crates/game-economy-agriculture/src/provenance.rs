//! Physical seed cohorts consumed by each currently cultivated field.
use crate::{Crop, Field, Phase};
use game_lot_runtime::Events;
use game_lots::Lot;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Planting {
    pub field: u64,
    pub tick: u64,
    /// Actual consumed physical cohorts, including any unknown batch.
    pub seeds: Vec<Lot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Sorted by field; records are removed only on successful harvest.
    #[serde(default)]
    pub plantings: Arc<Vec<Planting>>,
}

impl Provenance {
    pub fn validate(&self, fields: &[Field], crops: &[Crop], now: u64) -> Result<(), String> {
        if self
            .plantings
            .par_windows(2)
            .any(|rows| rows[0].field >= rows[1].field)
        {
            return Err("invalid seed provenance ordering".into());
        }
        let mut index: Vec<_> = fields.par_iter().map(|field| (field.id, field)).collect();
        index.par_sort_unstable_by_key(|row| row.0);
        let mut crop_index: Vec<_> = crops.par_iter().map(|crop| (crop.id, crop)).collect();
        crop_index.par_sort_unstable_by_key(|row| row.0);
        if self.plantings.par_iter().any(|record| {
            let Some(field) = index
                .binary_search_by_key(&record.field, |row| row.0)
                .ok()
                .map(|i| index[i].1)
            else {
                return true;
            };
            let Some(crop) = crop_index
                .binary_search_by_key(&field.crop, |row| row.0)
                .ok()
                .map(|i| crop_index[i].1)
            else {
                return true;
            };
            record.tick > now
                || field.phase == Phase::Fallow
                || record.seeds.is_empty()
                || record.seeds.iter().any(|lot| {
                    lot.good != crop.seed
                        || lot.quantity == 0
                        || lot.batch == Some(0)
                        || (lot.batch.is_some() && lot.item.is_some())
                        || lot.item.is_some_and(|id| {
                            id == 0 || lot.quantity != 1 || lot.expires_at.is_some()
                        })
                })
                || record
                    .seeds
                    .iter()
                    .try_fold(0u64, |sum, lot| sum.checked_add(lot.quantity))
                    != Some(crop.seed_quantity)
        }) {
            return Err("invalid seed provenance".into());
        }
        Ok(())
    }

    /// Stage from accepted physical receipts; no simulator state is mutated.
    pub(crate) fn stage(
        &self,
        changes: &[(u64, u64, Phase, u32)],
        events: &Events,
        decisions: &[&game_lots::TransactionOutcome],
    ) -> Result<Self, String> {
        if changes.is_empty() {
            return Ok(self.clone());
        }
        let mut receipts: Vec<_> = if changes.par_iter().any(|row| row.2 == Phase::Growing) {
            events.physical_movements.par_iter().collect()
        } else {
            Vec::new()
        };
        if receipts
            .par_windows(2)
            .any(|rows| rows[0].action > rows[1].action)
        {
            receipts.par_sort_unstable_by_key(|row| row.action);
        }
        let mut updates: Vec<_> = changes
            .par_iter()
            .filter_map(|&(field, action, phase, _)| {
                let accepted = decisions
                    .binary_search_by_key(&action, |row| row.id)
                    .ok()
                    .is_some_and(|i| decisions[i].accepted);
                accepted.then_some((field, action, phase))
            })
            .map(|(field, action, phase)| -> Result<_, String> {
                if phase == Phase::Fallow {
                    return Ok((field, None));
                }
                if phase != Phase::Growing {
                    return Err("invalid planting phase".into());
                }
                let i = receipts
                    .binary_search_by_key(&action, |row| row.action)
                    .map_err(|_| "missing seed receipt")?;
                let receipt = receipts[i];
                if !receipt.movement.accepted
                    || receipt.request.to.is_some()
                    || receipt.movement.lots.is_empty()
                {
                    return Err("invalid seed receipt".into());
                }
                Ok((
                    field,
                    Some(Planting {
                        field,
                        tick: events.tick,
                        seeds: receipt.movement.lots.to_vec(),
                    }),
                ))
            })
            .collect::<Result<_, _>>()?;
        updates.par_sort_unstable_by_key(|row| row.0);
        if updates.is_empty() {
            return Ok(self.clone());
        }
        if updates.par_windows(2).any(|rows| rows[0].0 == rows[1].0) {
            return Err("duplicate field provenance update".into());
        }
        let mut plantings: Vec<_> = self
            .plantings
            .par_iter()
            .filter(|row| {
                updates
                    .binary_search_by_key(&row.field, |update| update.0)
                    .is_err()
            })
            .cloned()
            .collect();
        plantings.par_extend(updates.into_par_iter().filter_map(|row| row.1));
        plantings.par_sort_unstable_by_key(|row| row.field);
        Ok(Self {
            plantings: Arc::new(plantings),
        })
    }
}

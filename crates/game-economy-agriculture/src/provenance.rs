//! Physical seed cohorts consumed by each currently cultivated field.
use crate::{Crop, Field, Phase};
use game_lot_runtime::Events;
use game_lots::Lot;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedPreference {
    pub batch: u64,
    pub enabled: bool,
    pub value: i64,
}

/// A person's stated preferences, not an observation of actual stock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedProfile {
    pub person: u64,
    pub idle: i64,
    #[serde(default)]
    pub act_on_tie: bool,
    #[serde(default)]
    pub preferences: Vec<SeedPreference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Planting {
    pub field: u64,
    pub tick: u64,
    /// Actual consumed physical cohorts, including any unknown batch.
    pub seeds: Vec<Lot>,
    /// Assigned once when the field matures; absent before maturation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offspring: Option<Vec<crate::reproduction::Offspring>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Sorted by field; records are removed only on successful harvest.
    #[serde(default)]
    pub plantings: Arc<Vec<Planting>>,
    /// Optional personal batch preferences, sorted by person.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Arc<Vec<SeedProfile>>,
    /// Optional physical growth genetics, never exposed as a decision input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genetics: Option<Arc<crate::genetics::Genetics>>,
}

impl Provenance {
    pub(crate) fn mature(
        &self,
        maturations: crate::reproduction::Maturations,
    ) -> Result<Self, String> {
        if maturations.rows.is_empty() {
            return Ok(self.clone());
        }
        if maturations
            .rows
            .par_windows(2)
            .any(|rows| rows[0].0 >= rows[1].0)
            || maturations.rows.par_iter().any(|(field, _)| {
                self.plantings
                    .binary_search_by_key(field, |record| record.field)
                    .ok()
                    .is_none_or(|i| self.plantings[i].offspring.is_some())
            })
        {
            return Err("invalid seed maturation".into());
        }
        let genetics = self
            .genetics
            .as_ref()
            .ok_or("maturation requires genetics")?;
        let genetics = Arc::new(
            genetics
                .with_descendants(&maturations.descendants)
                .map_err(str::to_owned)?,
        );
        let plantings: Vec<_> = self
            .plantings
            .par_iter()
            .map(|record| {
                let mut next = record.clone();
                if let Ok(index) = maturations
                    .rows
                    .binary_search_by_key(&record.field, |row| row.0)
                {
                    next.offspring = Some(maturations.rows[index].1.clone());
                }
                next
            })
            .collect();
        Ok(Self {
            plantings: Arc::new(plantings),
            profiles: Arc::clone(&self.profiles),
            genetics: Some(genetics),
        })
    }
    /// None is an unconfigured person; Some(None) is a personal refusal.
    pub(crate) fn choose(&self, person: u64) -> Option<Option<u64>> {
        let index = self
            .profiles
            .binary_search_by_key(&person, |row| row.person)
            .ok()?;
        let profile = &self.profiles[index];
        Some(
            profile
                .preferences
                .iter()
                .filter(|row| {
                    row.enabled
                        && (row.value > profile.idle
                            || (row.value == profile.idle && profile.act_on_tie))
                })
                .max_by_key(|row| (row.value, std::cmp::Reverse(row.batch)))
                .map(|row| row.batch),
        )
    }
    pub fn validate(
        &self,
        fields: &[Field],
        crops: &[Crop],
        people: &std::collections::HashSet<u64>,
        now: u64,
    ) -> Result<(), String> {
        if let Some(genetics) = &self.genetics {
            genetics.validate().map_err(str::to_owned)?;
        }
        if self
            .profiles
            .par_windows(2)
            .any(|rows| rows[0].person >= rows[1].person)
            || self.profiles.par_iter().any(|profile| {
                !people.contains(&profile.person)
                    || profile.preferences.iter().any(|row| row.batch == 0)
                    || profile
                        .preferences
                        .windows(2)
                        .any(|rows| rows[0].batch >= rows[1].batch)
            })
        {
            return Err("invalid personal seed preferences".into());
        }
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
            let reproduction = self
                .genetics
                .as_ref()
                .and_then(|genetics| genetics.reproduction);
            let offspring_invalid = match (&record.offspring, reproduction) {
                (None, Some(_)) if field.phase == Phase::Ripe => true,
                (Some(_), None) => true,
                (Some(rows), Some(_)) => {
                    field.phase != Phase::Ripe
                        || rows.windows(2).any(|pair| pair[0].batch >= pair[1].batch)
                        || rows.iter().any(|row| {
                            row.quantity == 0
                                || self
                                    .genetics
                                    .as_ref()
                                    .and_then(|genetics| genetics.descendant(row.batch))
                                    .is_none_or(|lineage| {
                                        lineage.parent != row.parent
                                            || lineage.alleles != row.alleles
                                    })
                        })
                }
                _ => false,
            };
            record.tick > now
                || field.phase == Phase::Fallow
                || offspring_invalid
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
                        offspring: None,
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
            profiles: Arc::clone(&self.profiles),
            genetics: self.genetics.clone(),
        })
    }
}

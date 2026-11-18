//! Elapsed growth of planted fields and atomic seed maturation.
use crate::{Agriculture, Field, Phase, irrigation::Irrigation, reproduction};
use game_environment::Season;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthClock {
    /// Duration of one unit of growth under neutral conditions.
    pub unit_millis: u64,
    /// Fractional growth over unit_millis * 1,000,000 for each growing field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remainders: Vec<(u64, u64)>,
}

impl GrowthClock {
    fn denominator(&self) -> Result<u64, String> {
        self.unit_millis
            .checked_mul(1_000_000)
            .filter(|value| *value != 0)
            .ok_or_else(|| "invalid elapsed crop growth unit".into())
    }

    pub(crate) fn validate(&self, fields: &[Field]) -> Result<(), String> {
        let denominator = self.denominator()?;
        let growing: std::collections::HashSet<_> = fields
            .par_iter()
            .filter(|field| field.phase == Phase::Growing)
            .map(|field| field.id)
            .collect();
        if self
            .remainders
            .par_windows(2)
            .any(|pair| pair[0].0 >= pair[1].0)
            || self.remainders.par_iter().any(|&(field, value)| {
                value == 0 || value >= denominator || !growing.contains(&field)
            })
        {
            return Err("invalid saved crop growth fractions".into());
        }
        Ok(())
    }
}

pub struct Rates<'a> {
    opening: &'a Agriculture,
    rows: Vec<(u64, u64, u128)>,
}

impl Rates<'_> {
    pub fn next_maturation(&self) -> Option<u128> {
        self.rows
            .par_iter()
            .map(|&(_, rate, remaining)| remaining.div_ceil(u128::from(rate)))
            .min()
    }
}

pub struct GrowthDraft<'a> {
    opening: &'a Agriculture,
    fields: Vec<Field>,
    clock: GrowthClock,
    maturation: Option<reproduction::MaturationDraft>,
}

impl GrowthDraft<'_> {
    pub fn batch_count(&self) -> u64 {
        self.maturation.as_ref().map_or(0, |draft| draft.count())
    }

    pub fn resolve(self, first_batch: Option<u64>) -> Result<Growth, String> {
        let owner = self.opening;
        let provenance = match self.maturation {
            Some(draft) => Some(
                owner
                    .provenance
                    .as_ref()
                    .ok_or("missing planted crop provenance")?
                    .mature(reproduction::assign(draft, first_batch)?)?,
            ),
            None => None,
        };
        Ok(Growth {
            opening_fields: owner.fields.par_iter().cloned().collect(),
            opening_clock: owner.growth_clock.clone(),
            opening_provenance: owner.provenance.clone(),
            fields: self.fields,
            clock: self.clock,
            provenance,
        })
    }
}

pub struct Growth {
    opening_fields: Vec<Field>,
    opening_clock: Option<GrowthClock>,
    opening_provenance: Option<crate::provenance::Provenance>,
    fields: Vec<Field>,
    clock: GrowthClock,
    provenance: Option<crate::provenance::Provenance>,
}

impl Growth {
    pub fn validate(&self, owner: &Agriculture) -> Result<(), String> {
        if owner.fields.len() != self.opening_fields.len()
            || owner
                .fields
                .par_iter()
                .zip(self.opening_fields.par_iter())
                .any(|(current, opening)| current != opening)
            || owner.growth_clock != self.opening_clock
            || owner.provenance != self.opening_provenance
        {
            return Err("stale elapsed crop growth draft".into());
        }
        Ok(())
    }

    /// The host must validate this draft before committing the associated
    /// physical operation, without editing crops between those two steps.
    pub fn publish_validated(self, owner: &mut Agriculture) {
        owner.fields = self.fields;
        owner.growth_clock = Some(self.clock);
        if let Some(provenance) = self.provenance {
            owner.provenance = Some(provenance);
        }
    }
}

impl Agriculture {
    /// Prepare concurrent rates against the opening field, season, and moisture.
    /// The host ends the segment before an earlier change in those conditions.
    pub fn elapsed_growth_rates(
        &self,
        season: Option<&Season>,
        irrigation: Option<&Irrigation>,
    ) -> Result<Option<Rates<'_>>, String> {
        let Some(clock) = &self.growth_clock else {
            return Ok(None);
        };
        let denominator = u128::from(clock.denominator()?);
        let base = season.map_or(1000, |season| {
            if season.tending_allowed {
                season.food_growth_per_thousand
            } else {
                0
            }
        });
        if base == 0 {
            return Ok(None);
        }
        let mut rows: Vec<_> = self
            .fields
            .par_iter()
            .filter_map(|field| {
                if field.phase != Phase::Growing
                    || irrigation.is_some_and(|owner| owner.dry(field.id))
                {
                    return None;
                }
                let genetic = season
                    .and_then(|season| {
                        let owner = self.provenance.as_ref()?;
                        let genetics = owner.genetics.as_ref()?;
                        let index = owner
                            .plantings
                            .binary_search_by_key(&field.id, |row| row.field)
                            .ok()?;
                        Some(genetics.seasonal_factor(&season.name, &owner.plantings[index].seeds))
                    })
                    .unwrap_or(1000);
                let rate = u64::from(base) * u64::from(genetic);
                if rate == 0 {
                    return None;
                }
                let fraction = clock
                    .remainders
                    .binary_search_by_key(&field.id, |row| row.0)
                    .ok()
                    .map_or(0, |index| clock.remainders[index].1);
                let remaining = u128::from(field.remaining) * denominator - u128::from(fraction);
                Some((field.id, rate, remaining))
            })
            .collect();
        if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            rows.par_sort_unstable_by_key(|row| row.0);
        }
        Ok((!rows.is_empty()).then_some(Rates {
            opening: self,
            rows,
        }))
    }

    /// Advance exact growth and prepare any newly ripe seed cohorts.
    pub fn stage_elapsed_growth<'a>(
        &'a self,
        rates: Rates<'a>,
        millis: u64,
        seed: u64,
    ) -> Result<GrowthDraft<'a>, String> {
        if !std::ptr::eq(self, rates.opening) {
            return Err("crop rates belong to a different generation".into());
        }
        let clock = self
            .growth_clock
            .as_ref()
            .ok_or("missing elapsed crop growth clock")?;
        let denominator = u128::from(clock.denominator()?);
        let rows: Vec<_> = self
            .fields
            .par_iter()
            .map(|field| {
                let mut field = field.clone();
                let fraction = if let Ok(index) =
                    rates.rows.binary_search_by_key(&field.id, |row| row.0)
                {
                    let (_, rate, remaining) = rates.rows[index];
                    let remaining = remaining.saturating_sub(u128::from(rate) * u128::from(millis));
                    let units = remaining.div_ceil(denominator);
                    field.remaining = units as u32;
                    if field.remaining == 0 {
                        field.phase = Phase::Ripe;
                    }
                    (units * denominator - remaining) as u64
                } else {
                    clock
                        .remainders
                        .binary_search_by_key(&field.id, |row| row.0)
                        .ok()
                        .map_or(0, |index| clock.remainders[index].1)
                };
                (field, fraction)
            })
            .collect();
        let mut remainders: Vec<_> = rows
            .par_iter()
            .filter_map(|(field, fraction)| (*fraction > 0).then_some((field.id, *fraction)))
            .collect();
        if remainders.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            remainders.par_sort_unstable_by_key(|row| row.0);
        }
        let ripened = rows
            .par_iter()
            .zip(self.fields.par_iter())
            .any(|((staged, _), old)| staged.phase == Phase::Ripe && old.phase == Phase::Growing);
        let fields: Vec<_> = rows.into_par_iter().map(|(field, _)| field).collect();
        let maturation = if ripened {
            self.provenance
                .as_ref()
                .map(|provenance| reproduction::prepare_maturation(&fields, provenance, seed))
                .transpose()?
                .flatten()
        } else {
            None
        };
        Ok(GrowthDraft {
            opening: self,
            fields,
            clock: GrowthClock {
                unit_millis: clock.unit_millis,
                remainders,
            },
            maturation,
        })
    }
}

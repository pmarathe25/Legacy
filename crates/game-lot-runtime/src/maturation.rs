//! Passive maturation uses the same indexed deadlines and custody as food spoilage.
use super::Runtime;
use game_lots::{Creation, ExpiryDraft, Place};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaturationOutcome {
    Matured,
    UnsuitableTemperature,
    StorageUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaturationEvent {
    pub place: Place,
    pub preparation: u32,
    pub product: u32,
    pub quantity: u64,
    pub batch: Option<u64>,
    pub destination: Place,
    pub celsius: i16,
    pub outcome: MaturationOutcome,
}

impl Runtime {
    pub(super) fn prepare_maturation(
        &self,
        draft: &mut ExpiryDraft,
        at: u64,
    ) -> Result<Vec<MaturationEvent>, String> {
        let plans: Vec<_> = draft
            .spoiled()
            .par_iter()
            .enumerate()
            .filter_map(|(index, source)| {
                let profile = self.catalog.maturation(source.good)?;
                let destination = match source.place {
                    Place::Stock { region, .. } => Place::Stock {
                        region,
                        good: profile.product,
                    },
                    elsewhere => elsewhere,
                };
                let celsius = self.store.storage_celsius(source.place);
                let outcome = if !(profile.min_celsius..=profile.max_celsius).contains(&celsius) {
                    MaturationOutcome::UnsuitableTemperature
                } else if self.store.container(destination).is_none() {
                    MaturationOutcome::StorageUnavailable
                } else {
                    MaturationOutcome::Matured
                };
                Some((
                    index,
                    MaturationEvent {
                        place: source.place,
                        preparation: source.good,
                        product: profile.product,
                        quantity: source.quantity,
                        batch: source.batch,
                        destination,
                        celsius,
                        outcome,
                    },
                ))
            })
            .collect();
        if plans.is_empty() {
            return Ok(Vec::new());
        }
        let creations: Vec<Creation> = plans
            .par_iter()
            .filter(|(_, event)| event.outcome == MaturationOutcome::Matured)
            .map(|(index, event)| {
                let id = u64::try_from(*index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or("maturation identity overflow")?;
                let mut creation = self.catalog.creation(
                    id,
                    event.destination,
                    event.product,
                    event.quantity,
                    at,
                )?;
                creation.lot.batch = event.batch;
                Ok::<_, String>(creation)
            })
            .collect::<Result<_, _>>()?;
        let admissions = self.store.stage_matured(draft, &creations, |good| {
            self.catalog.maturation(good).is_some()
        })?;
        Ok(plans
            .into_par_iter()
            .map(|(index, mut event)| {
                if event.outcome == MaturationOutcome::Matured {
                    let admitted = admissions
                        .binary_search_by_key(&(index as u64 + 1), |row| row.0)
                        .expect("planned maturation receipt");
                    if !admissions[admitted].1 {
                        event.outcome = MaturationOutcome::StorageUnavailable;
                    }
                }
                event
            })
            .collect())
    }
}

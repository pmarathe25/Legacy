//! Reproject exposed food when the ambient season changes.
use super::Store;
use game_lot_model::{Freshness, key};
use game_lot_planning::{Changes, Draft};
use rayon::prelude::*;

impl Store {
    pub fn ambient_celsius(&self) -> Option<i16> {
        self.state
            .storage_climate
            .as_ref()
            .map(|climate| climate.ambient_celsius)
    }

    pub fn change_ambient_temperature(&mut self, celsius: i16) -> Result<(), String> {
        let climate = self
            .state
            .storage_climate
            .as_ref()
            .ok_or("no storage climate")?;
        if celsius == climate.ambient_celsius {
            return Ok(());
        }
        let at = self.state.expiry_millis.ok_or("no elapsed storage clock")?;
        if self.expiry.next().is_some_and(|deadline| deadline <= at) {
            return Err("publish due lots before changing storage temperature".into());
        }
        let old_rate = climate.rate_at(climate.ambient_celsius);
        let new_rate = climate.rate_at(celsius);
        let containers = &self.state.containers;
        let changes = if old_rate != new_rate {
            let exposed: Vec<_> = containers
                .par_iter()
                .enumerate()
                .filter(|(_, row)| {
                    row.next_expiry.is_some()
                        && climate.override_at(row.place).is_none()
                        && row.lots.iter().any(|lot| {
                            lot.expires_at.is_some() && lot.freshness != Freshness::Preparation
                        })
                })
                .collect();
            exposed
                .into_par_iter()
                .map(|(index, row)| {
                    let mut next = row.clone();
                    next.lots
                        .par_iter_mut()
                        .with_min_len(128)
                        .try_for_each(|lot| lot.change_aging_rate(at, old_rate, new_rate))?;
                    // Preparations use their own clock and may change order relative to food.
                    if next
                        .lots
                        .iter()
                        .any(|lot| lot.freshness == Freshness::Preparation)
                    {
                        next.lots.par_sort_unstable_by_key(key);
                    }
                    next.next_expiry = next
                        .lots
                        .par_iter()
                        .with_min_len(128)
                        .filter_map(|lot| lot.expires_at)
                        .min();
                    Ok::<_, String>((index, next))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let draft = self.item_draft(
            Draft {
                base_instance: self.instance,
                base_tick: self.state.tick,
                base_revision: self.state.revision,
                next_tick: self.state.tick,
                changes: Changes::Flat(changes),
                outcomes: Default::default(),
                spoiled: Vec::new(),
                creations: Vec::new(),
                transactions: Vec::new(),
            },
            Vec::new(),
            false,
        )?;
        self.commit(draft)?;
        self.state
            .storage_climate
            .as_mut()
            .expect("existing climate")
            .ambient_celsius = celsius;
        Ok(())
    }
}

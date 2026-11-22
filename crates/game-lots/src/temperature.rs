//! Reproject exposed food when the ambient season changes.
use super::{Draft, Store};
use game_lot_model::{Container, Freshness, Place, StorageTemperature, key};
use game_lot_planning::{Changes, Draft as PlanDraft};
use rayon::prelude::*;

impl Store {
    pub fn storage_override_celsius(&self, place: Place) -> Option<i16> {
        self.state.storage_climate.as_ref()?.override_at(place)
    }

    pub fn prepare_holder_temperatures(
        &self,
        temperatures: &[(Place, Option<i16>)],
    ) -> Result<Draft, String> {
        let mut draft = self.item_draft(
            PlanDraft {
                base_instance: self.instance,
                base_tick: self.state.tick,
                base_revision: self.state.revision,
                next_tick: self.state.tick,
                changes: Changes::Empty,
                outcomes: Default::default(),
                spoiled: Vec::new(),
                creations: Vec::new(),
                transactions: Vec::new(),
            },
            Vec::new(),
            false,
        )?;
        self.set_holder_temperatures_in(&mut draft, temperatures)?;
        Ok(draft)
    }

    /// Reproject affected holders in the same draft as their creation or physical changes.
    /// A missing temperature exposes the holder to the current ambient climate.
    pub fn set_holder_temperatures_in(
        &self,
        draft: &mut Draft,
        temperatures: &[(Place, Option<i16>)],
    ) -> Result<(), String> {
        if temperatures.is_empty() {
            return Ok(());
        }
        self.validate_commit(draft)?;
        let climate = self
            .state
            .storage_climate
            .as_ref()
            .ok_or("no storage climate")?;
        let at = self.state.expiry_millis.ok_or("no elapsed storage clock")?;
        if draft.temperatures.is_some()
            || draft.inner.next_tick != self.state.tick
            || temperatures
                .par_windows(2)
                .any(|pair| pair[0].0 >= pair[1].0)
            || self.expiry.next().is_some_and(|deadline| deadline <= at)
        {
            return Err("invalid holder temperature change".into());
        }
        let opening = &self.state.containers;
        let additions = &draft.additions;
        let retire = &draft.retire;
        let changes = draft.inner.changes.flat_mut();
        let updates: Vec<_> = temperatures
            .par_iter()
            .map(|&(place, degrees)| {
                if retire.binary_search(&place).is_ok() {
                    return Err("cannot change temperature of retired custody".into());
                }
                let before = opening.partition_point(|row| row.place < place);
                let added = additions.partition_point(|row| row.place < place);
                let container = opening
                    .get(before)
                    .filter(|row| row.place == place)
                    .or_else(|| additions.get(added).filter(|row| row.place == place))
                    .ok_or("no holder at temperature location")?;
                let index = before + added;
                let holder = changes
                    .binary_search_by_key(&index, |row| row.0)
                    .map_or(container, |found| &changes[found].1);
                let old_rate = climate.rate_at(climate.temperature(place));
                let new_rate = climate.rate_at(degrees.unwrap_or(climate.ambient_celsius));
                if old_rate == new_rate || !has_thermal_lots(holder) {
                    return Ok(None);
                }
                reproject(holder, at, old_rate, new_rate).map(|next| Some((index, next)))
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .flatten()
            .collect();
        if !updates.is_empty() {
            let mut combined: Vec<_> = std::mem::take(changes)
                .into_par_iter()
                .filter(|(index, _)| updates.binary_search_by_key(index, |row| row.0).is_err())
                .collect();
            combined.par_extend(updates);
            combined.par_sort_unstable_by_key(|row| row.0);
            *changes = combined;
        }
        let mut next: Vec<_> = climate
            .places
            .par_iter()
            .copied()
            .filter(|row| {
                temperatures
                    .binary_search_by_key(&row.place, |edit| edit.0)
                    .is_err()
            })
            .chain(temperatures.par_iter().filter_map(|&(place, celsius)| {
                celsius.map(|celsius| StorageTemperature { place, celsius })
            }))
            .collect();
        next.par_sort_unstable_by_key(|row| row.place);
        draft.temperatures = Some(next);
        Ok(())
    }

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
                    climate.override_at(row.place).is_none() && has_thermal_lots(row)
                })
                .collect();
            exposed
                .into_par_iter()
                .map(|(index, row)| {
                    reproject(row, at, old_rate, new_rate).map(|next| (index, next))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let draft = self.item_draft(
            PlanDraft {
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

fn has_thermal_lots(holder: &Container) -> bool {
    holder.next_expiry.is_some()
        && holder
            .lots
            .iter()
            .any(|lot| lot.expires_at.is_some() && lot.freshness != Freshness::Preparation)
}

fn reproject(
    holder: &Container,
    at: u64,
    old_rate: u16,
    new_rate: u16,
) -> Result<Container, String> {
    let mut next = holder.clone();
    next.lots
        .par_iter_mut()
        .with_min_len(128)
        .try_for_each(|lot| lot.change_aging_rate(at, old_rate, new_rate))?;
    // Preparation clocks remain fixed and can change order relative to food.
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
    Ok(next)
}

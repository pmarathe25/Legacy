//! Wildlife deadlines and physical encounters between ecological events.
use super::{Animal, AnimalDeath, DeathCause, Fauna, Feeding, apply_feeding_meetings};
use rayon::prelude::*;

pub struct Encounters {
    changes: Vec<(usize, Animal)>,
    deaths: Vec<AnimalDeath>,
}

impl Encounters {
    pub fn deaths(&self) -> impl Iterator<Item = u64> + '_ {
        self.deaths.iter().map(|death| death.animal)
    }
}

impl Fauna {
    /// Time until the next biological update, in milliseconds from the opening state.
    pub fn next_lifecycle_after(&self, now: u128) -> Option<u64> {
        let period = u128::from(self.lifecycle_period_millis?);
        let ordinal = u128::from(self.last_tick.checked_add(1)?);
        u64::try_from((ordinal * period).checked_sub(now)?).ok()
    }

    pub fn lifecycle_ordinal(&self, now: u128, settlement: u64) -> Result<u64, &'static str> {
        match self.lifecycle_period_millis {
            None => Ok(settlement),
            Some(0) => Err("invalid fauna lifecycle period"),
            Some(period) => u64::try_from(now / u128::from(period))
                .map_err(|_| "fauna lifecycle clock overflow"),
        }
    }

    /// Stage accepted hunts and feeding when they occur between lifecycle events.
    pub fn prepare_encounters(
        &self,
        hunted: &[u64],
        feeding: &[Feeding],
    ) -> Result<Encounters, &'static str> {
        if hunted.is_empty() && feeding.is_empty() {
            return Ok(Encounters {
                changes: Vec::new(),
                deaths: Vec::new(),
            });
        }
        let by_id = if self
            .animals
            .par_windows(2)
            .any(|pair| pair[0].id >= pair[1].id)
        {
            let mut index: Vec<_> = self
                .animals
                .par_iter()
                .enumerate()
                .map(|(i, row)| (row.id, i))
                .collect();
            index.par_sort_unstable();
            Some(index)
        } else {
            None
        };
        let index = |id| {
            if let Some(index) = &by_id {
                index
                    .binary_search_by_key(&id, |row| row.0)
                    .ok()
                    .map(|i| index[i].1)
            } else {
                self.animals
                    .binary_search_by_key(&id, |animal| animal.id)
                    .ok()
            }
        };
        let mut touches: Vec<_> = hunted
            .par_iter()
            .map(|&id| (id, None))
            .chain(feeding.par_iter().map(|food| (food.animal, Some(food))))
            .collect();
        touches.par_sort_unstable_by_key(|row| (row.0, row.1.and_then(|food| food.person)));
        let mut species: Vec<_> = self.species.par_iter().collect();
        species.par_sort_unstable_by_key(|row| row.id);
        let mut changes = touches
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let id = group[0].0;
                let i = index(id)
                    .filter(|&i| self.animals[i].health > 0)
                    .ok_or("unknown living animal")?;
                let mut animal = self.animals[i].clone();
                if group.iter().any(|row| row.1.is_none()) {
                    if group.len() != 1 {
                        return Err("invalid or simultaneously fed hunted animal");
                    }
                    animal.health = 0;
                    animal.pregnancy = None;
                    return Ok((i, animal));
                }
                let rules = species
                    .binary_search_by_key(&animal.species, |row| row.id)
                    .map(|i| species[i])
                    .map_err(|_| "unknown animal species")?;
                let mut meetings: Vec<((), u64, u64)> = Vec::new();
                for food in group.iter().filter_map(|row| row.1) {
                    if food.quantity == 0 || food.person == Some(0) {
                        return Err("invalid animal food");
                    }
                    if food.good == rules.food_good {
                        let relief = u128::from(food.quantity) * u128::from(rules.relief_per_unit);
                        animal.hunger = animal
                            .hunger
                            .saturating_sub(relief.min(u128::from(u32::MAX)) as u32);
                    }
                    if let Some(person) = food.person {
                        if let Some(last) = meetings.last_mut().filter(|row| row.1 == person) {
                            last.2 = last.2.saturating_add(1);
                        } else {
                            meetings.push(((), person, 1));
                        }
                    }
                }
                apply_feeding_meetings(&mut animal, &meetings);
                Ok((i, animal))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let deaths = changes
            .iter()
            .filter_map(|(_, animal)| {
                (animal.health == 0).then_some(AnimalDeath {
                    animal: animal.id,
                    tick: self.last_tick,
                    cause: DeathCause::Hunted,
                })
            })
            .collect();
        changes.par_sort_unstable_by_key(|row| row.0);
        Ok(Encounters { changes, deaths })
    }

    /// Publish against the same owner from which these changes were staged.
    pub fn publish_encounters(&mut self, draft: Encounters) {
        self.recent_deaths = self.publish_encounter_receipts(draft);
    }

    /// Publish encounters already reported with their own elapsed timestamp.
    pub fn publish_encounter_receipts(&mut self, draft: Encounters) -> Vec<AnimalDeath> {
        for (index, animal) in draft.changes {
            self.animals[index] = animal;
        }
        let mut deaths = draft.deaths;
        deaths.par_sort_unstable_by_key(|row| row.animal);
        deaths
    }
}

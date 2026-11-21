//! Independent habitat choices along physically authored wildlife corridors.
use crate::{Animal, Fauna, QuietDraft, Species};
use game_genetics::random;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dispersal {
    pub corridors: Vec<Corridor>,
    pub policies: Vec<Policy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corridor {
    pub from: u64,
    pub to: u64,
    pub hunger_cost: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub species: u32,
    pub minimum_hunger: u32,
    pub minimum_age: u64,
    /// Per-thousand chance of exploring at neutral genetic expression.
    pub initiative: u16,
    pub locus: usize,
    pub inherited_drive: i32,
    pub effort_weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Migration {
    pub animal: u64,
    pub species: u32,
    pub from: u64,
    pub to: u64,
    pub hunger_cost: u32,
}

impl Dispersal {
    pub(crate) fn normalize(&mut self) {
        self.corridors
            .sort_unstable_by_key(|row| (row.from, row.to));
        self.policies.sort_unstable_by_key(|row| row.species);
    }

    pub(crate) fn validate(
        &self,
        regions: &HashSet<u64>,
        species: &[&Species],
    ) -> Result<(), &'static str> {
        if self.corridors.is_empty()
            || self.policies.is_empty()
            || self
                .corridors
                .windows(2)
                .any(|rows| (rows[0].from, rows[0].to) >= (rows[1].from, rows[1].to))
            || self.corridors.iter().any(|row| {
                row.from == row.to || !regions.contains(&row.from) || !regions.contains(&row.to)
            })
            || self
                .policies
                .windows(2)
                .any(|rows| rows[0].species >= rows[1].species)
            || self.policies.iter().any(|row| {
                row.initiative > 1000
                    || row.locus >= 4
                    || species
                        .binary_search_by_key(&row.species, |entry| entry.id)
                        .is_err()
            })
        {
            return Err("invalid wildlife dispersal");
        }
        Ok(())
    }

    fn choose(
        &self,
        animal: &Animal,
        age: u64,
        hunger: u32,
        seed: u64,
        round: u64,
    ) -> Option<Migration> {
        let index = self
            .policies
            .binary_search_by_key(&animal.species, |row| row.species)
            .ok()?;
        let policy = self.policies[index];
        if age < policy.minimum_age || hunger < policy.minimum_hunger {
            return None;
        }
        let start = self
            .corridors
            .partition_point(|row| row.from < animal.region);
        let end = self
            .corridors
            .partition_point(|row| row.from <= animal.region);
        let corridors = &self.corridors[start..end];
        if corridors.is_empty() {
            return None;
        }
        let expression = i64::from(animal.genome.expressed(policy.locus)) - 500;
        let initiative =
            i64::from(policy.initiative) + expression * i64::from(policy.inherited_drive) / 500;
        if random(seed ^ 0x6469737065727365, animal.id, round) % 1000
            >= initiative.clamp(0, 1000) as u64
        {
            return None;
        }
        let chosen = corridors.iter().max_by_key(|row| {
            let encounter = random(seed ^ round, animal.id, row.to) % 1000;
            (
                encounter as i64 - i64::from(row.hunger_cost) * i64::from(policy.effort_weight),
                row.to,
            )
        })?;
        Some(Migration {
            animal: animal.id,
            species: animal.species,
            from: animal.region,
            to: chosen.to,
            hunger_cost: chosen.hunger_cost,
        })
    }
}

impl Fauna {
    /// Stage migration with the actual health and food left after this lifecycle event.
    pub fn disperse_quiet(
        &self,
        draft: &mut QuietDraft,
        seed: u64,
        restrained: impl Fn(u64) -> bool + Sync,
    ) -> Vec<Migration> {
        let Some(dispersal) = &self.dispersal else {
            return Vec::new();
        };
        let indexed: Vec<_> = self
            .animals
            .par_iter()
            .zip(draft.updates.par_iter_mut())
            .enumerate()
            .filter_map(|(index, (animal, staged))| {
                if staged.health == 0 || restrained(animal.id) {
                    return None;
                }
                let movement =
                    dispersal.choose(animal, staged.age, staged.hunger, seed, draft.tick)?;
                staged.hunger = staged.hunger.saturating_add(movement.hunger_cost);
                Some((index, movement))
            })
            .collect();
        draft.migrations = indexed
            .par_iter()
            .map(|&(index, row)| (index, row.to))
            .collect();
        indexed.into_par_iter().map(|(_, row)| row).collect()
    }

    /// The full lifecycle fallback uses the same decision. Newborns remain where delivered.
    pub fn disperse_living(
        &mut self,
        first_newborn: u64,
        seed: u64,
        restrained: impl Fn(u64) -> bool + Sync,
    ) -> Vec<Migration> {
        let Some(dispersal) = &self.dispersal else {
            return Vec::new();
        };
        let mut movements: Vec<_> = self
            .animals
            .par_iter_mut()
            .filter_map(|animal| {
                if animal.id >= first_newborn || animal.health == 0 || restrained(animal.id) {
                    return None;
                }
                let movement =
                    dispersal.choose(animal, animal.age, animal.hunger, seed, self.last_tick)?;
                animal.region = movement.to;
                animal.hunger = animal.hunger.saturating_add(movement.hunger_cost);
                Some(movement)
            })
            .collect();
        movements.par_sort_unstable_by_key(|row| row.animal);
        movements
    }
}

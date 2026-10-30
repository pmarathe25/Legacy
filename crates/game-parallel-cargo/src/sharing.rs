//! General individual inclinations to share, independent of particular contacts.
use super::Cargo;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodSharingPolicy {
    pub person: u64,
    pub amount: u32,
    pub retain: u32,
    pub threshold: u64,
    pub interval: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
pub(super) fn validate<F>(cargo: &Cargo, exists: F) -> Result<(), &'static str>
where
    F: Fn(u64) -> bool + Sync,
{
    if (!cargo.food_sharing.is_empty() || cargo.inherit_food_sharing) && !cargo.personal_food_access
    {
        return Err("invalid food sharing preference");
    }
    let mut people: Vec<_> = cargo
        .food_sharing
        .par_iter()
        .map(|row| row.person)
        .collect();
    people.par_sort_unstable();
    if people.par_windows(2).any(|pair| pair[0] == pair[1])
        || cargo.food_sharing.par_iter().any(|row| {
            row.amount == 0 || row.interval == 0 || row.threshold == 0 || !exists(row.person)
        })
    {
        return Err("invalid food sharing preference");
    }
    Ok(())
}

impl Cargo {
    pub fn inherits_birth_preferences(&self) -> bool {
        self.inherit_food_sharing || self.inherit_store_building
    }

    /// Give a newborn a personal starting inclination copied from one parent.
    pub fn inherit_sharing(&mut self, births: &[(u64, [u64; 2], usize)]) {
        if !self.inherit_food_sharing || births.is_empty() {
            return;
        }
        let mut parents: Vec<_> = self.food_sharing.par_iter().collect();
        if parents
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            parents.par_sort_unstable_by_key(|row| row.person);
        }
        let additions: Vec<_> = births
            .par_iter()
            .filter_map(|&(person, ids, count)| {
                if count == 0 {
                    return None;
                }
                let parent = ids[(person % count as u64) as usize];
                let index = parents
                    .binary_search_by_key(&parent, |row| row.person)
                    .ok()?;
                let mut preference = parents[index].clone();
                preference.person = person;
                Some(preference)
            })
            .collect();
        self.food_sharing.par_extend(additions);
    }
}

//! Personal associations between audible forms and visible marks.
//!
//! This owner does not create physical pages or imply that another person
//! understands a mark. Hosts must ground observations in actual encounters.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Association {
    pub sounds: Vec<u8>,
    pub marks: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    #[serde(default)]
    pub associations: Vec<Association>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub capacity: usize,
    #[serde(default)]
    pub minds: Vec<Mind>,
}

impl Writing {
    /// Validate an owner's saved associations against the actual sound palette.
    pub fn validate(&self, sounds: usize, people: &[u64]) -> Result<(), &'static str> {
        if self.capacity == 0
            || self.capacity > 1024
            || people.windows(2).any(|row| row[0] >= row[1])
            || self
                .minds
                .par_windows(2)
                .any(|row| row[0].person >= row[1].person)
            || self.minds.par_iter().any(|mind| {
                people.binary_search(&mind.person).is_err()
                    || mind.associations.len() > self.capacity
                    || mind
                        .associations
                        .windows(2)
                        .any(|row| row[0].sounds >= row[1].sounds)
                    || mind.associations.iter().any(|row| !valid(row, sounds))
            })
        {
            return Err("invalid personal writing associations");
        }
        Ok(())
    }

    /// Record one person's chosen mapping, replacing only their own earlier form.
    pub fn assign(
        &mut self,
        person: u64,
        association: Association,
        sounds: usize,
    ) -> Result<(), &'static str> {
        if person == 0 || !valid(&association, sounds) || self.capacity == 0 || self.capacity > 1024
        {
            return Err("invalid writing choice");
        }
        let index = match self.minds.binary_search_by_key(&person, |mind| mind.person) {
            Ok(index) => index,
            Err(index) => {
                self.minds.insert(
                    index,
                    Mind {
                        person,
                        associations: Vec::new(),
                    },
                );
                index
            }
        };
        let mind = &mut self.minds[index];
        match mind
            .associations
            .binary_search_by(|row| row.sounds.cmp(&association.sounds))
        {
            Ok(index) => mind.associations[index] = association,
            Err(index) if mind.associations.len() < self.capacity => {
                mind.associations.insert(index, association)
            }
            Err(_) => return Err("writing association capacity reached"),
        }
        Ok(())
    }

    /// Look up the visible form a person has chosen for a spoken form.
    pub fn marks(&self, person: u64, sounds: &[u8]) -> Option<&[u32]> {
        let index = self
            .minds
            .binary_search_by_key(&person, |mind| mind.person)
            .ok()?;
        let associations = &self.minds[index].associations;
        let index = associations
            .binary_search_by(|row| row.sounds.as_slice().cmp(sounds))
            .ok()?;
        Some(&associations[index].marks)
    }

    /// Interpret only through this reader's own associations; homographs remain ambiguous.
    pub fn interpretations(&self, person: u64, marks: &[u32]) -> Vec<Vec<u8>> {
        self.minds
            .binary_search_by_key(&person, |mind| mind.person)
            .ok()
            .map_or_else(Vec::new, |index| {
                self.minds[index]
                    .associations
                    .iter()
                    .filter(|row| row.marks == marks)
                    .map(|row| row.sounds.clone())
                    .collect()
            })
    }
}

fn valid(row: &Association, sounds: usize) -> bool {
    !row.sounds.is_empty()
        && row.sounds.len() <= 16
        && row.sounds.iter().all(|&sound| usize::from(sound) < sounds)
        && !row.marks.is_empty()
        && row.marks.len() <= 16
        && row.marks.iter().all(|&mark| mark != 0)
}

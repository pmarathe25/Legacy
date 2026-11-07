//! Change and inspect personal meeting preferences.
use crate::{Economy, OpinionSensitivity, RelationshipCase};

impl Economy {
    /// Change a person's own general or relationship-specific meeting preference.
    pub fn set_social_affinity_weight(
        &mut self,
        person: u64,
        target: Option<u64>,
        weight: Option<i32>,
    ) -> Result<(), String> {
        if target.is_some_and(|id| {
            id == person || self.people.binary_search_by_key(&id, |row| row.id).is_err()
        }) {
            return Err("meeting preference requires another known person".into());
        }
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        let mut sensitivity = owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .cloned()
            .unwrap_or_else(|| OpinionSensitivity {
                person,
                ..Default::default()
            });
        if let Some(target) = target {
            if let Some(index) = sensitivity
                .relationship_cases
                .iter()
                .position(|case| case.target == target)
            {
                sensitivity.relationship_cases[index].social_affinity_weight = weight;
                if sensitivity.relationship_cases[index]
                    == (RelationshipCase {
                        target,
                        ..Default::default()
                    })
                {
                    sensitivity.relationship_cases.remove(index);
                }
            } else if let Some(weight) = weight {
                sensitivity.relationship_cases.push(RelationshipCase {
                    target,
                    social_affinity_weight: Some(weight),
                    ..Default::default()
                });
            }
        } else {
            sensitivity.social_affinity_weight = weight;
        }
        self.set_opinion_sensitivity(sensitivity)
    }
    /// Return the explicit setting; `None` means use the general or default weight.
    pub fn social_affinity_weight(
        &self,
        person: u64,
        target: Option<u64>,
    ) -> Result<Option<i32>, String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .is_err()
            || target.is_some_and(|id| {
                id == person || self.people.binary_search_by_key(&id, |row| row.id).is_err()
            })
        {
            return Err("unknown person or meeting target".into());
        }
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        Ok(owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .and_then(|row| {
                target.map_or(row.social_affinity_weight, |target| {
                    row.relationship_cases
                        .iter()
                        .find(|case| case.target == target)
                        .and_then(|case| case.social_affinity_weight)
                })
            }))
    }
}

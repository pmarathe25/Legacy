//! Only an attributed visit can change a child's feelings about a caregiver.
use crate::{Opinions, PersonalChange, Sensitivity};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub child: u64,
    pub caregiver: u64,
    pub age: u64,
}

pub(crate) fn has_reaction(row: &Sensitivity) -> bool {
    row.care_reaction.is_some()
        || row
            .relationship_cases
            .iter()
            .any(|case| case.care_reaction.is_some())
}

pub(crate) fn apply(owner: &mut Opinions, experiences: &[Experience], tick: u64) {
    if experiences.is_empty() {
        return;
    }
    let mut profiles: Vec<_> = owner
        .sensitivities
        .par_iter()
        .filter(|row| has_reaction(row))
        .collect();
    if profiles.par_windows(2).any(|p| p[0].person > p[1].person) {
        profiles.par_sort_unstable_by_key(|row| row.person);
    }
    if profiles.is_empty() {
        return;
    }
    let changes: Vec<_> = experiences
        .par_iter()
        .filter_map(|experience| {
            let profile = profiles
                .binary_search_by_key(&experience.child, |row| row.person)
                .ok()?;
            let profile = profiles[profile];
            let particular = profile
                .relationship_cases
                .iter()
                .find(|row| row.target == experience.caregiver)
                .and_then(|row| row.care_reaction);
            let reaction = particular.or(profile.care_reaction)?;
            if experience.age >= reaction.until_age || experience.child == experience.caregiver {
                return None;
            }
            let value = reaction.visit;
            Some(PersonalChange {
                observer: experience.child,
                target: experience.caregiver,
                trust: value.trust,
                affinity: value.affinity,
                fear: value.fear,
                obligation: value.obligation,
            })
        })
        .collect();
    crate::personal_change::apply_owned(owner, changes, tick);
}

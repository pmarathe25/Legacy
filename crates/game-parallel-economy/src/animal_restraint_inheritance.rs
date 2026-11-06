//! Optional inheritance of general animal-related inclinations, without relationships.
use crate::{AnimalCapturePolicy, AnimalReleasePolicy, AnimalRestraints, Families};
use rayon::prelude::*;

pub fn enroll(owner: Option<&mut AnimalRestraints>, family: Option<&Families>, tick: u64) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let births: Vec<_> = family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .collect();
    if births.is_empty() {
        return;
    }
    let capture: Vec<_> = births
        .par_iter()
        .filter_map(|member| {
            if owner
                .policies
                .binary_search_by_key(&member.id, |policy| policy.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                owner
                    .policies
                    .binary_search_by_key(id, |policy| policy.person)
                    .ok()
                    .map(|index| &owner.policies[index])
            })?;
            Some(AnimalCapturePolicy {
                person: member.id,
                animal: None,
                willing: parent.willing,
                affinity_at_least: parent.affinity_at_least,
                affinity_at_most: parent.affinity_at_most,
                effort: owner.default_effort,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect();
    let release: Vec<_> = births
        .par_iter()
        .filter_map(|member| {
            if owner
                .release_policies
                .binary_search_by_key(&member.id, |policy| policy.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                owner
                    .release_policies
                    .binary_search_by_key(id, |policy| policy.person)
                    .ok()
                    .map(|index| &owner.release_policies[index])
            })?;
            Some(AnimalReleasePolicy {
                person: member.id,
                animal: None,
                willing: parent.willing,
                effort: owner.default_effort,
                affinity_at_least: parent.affinity_at_least,
                affinity_at_most: parent.affinity_at_most,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect();
    owner.policies.par_extend(capture);
    owner.release_policies.par_extend(release);
    if owner
        .policies
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        owner
            .policies
            .par_sort_unstable_by_key(|policy| policy.person);
    }
    if owner
        .release_policies
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        owner
            .release_policies
            .par_sort_unstable_by_key(|policy| policy.person);
    }
}

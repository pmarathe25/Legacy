//! Seed an independent future caregiver preference from one recorded parent.
use crate::{DependentCare, Policy};
use rayon::prelude::*;
pub fn enroll(care: &mut DependentCare, births: &[(u64, [u64; 2], usize)]) {
    if !care.inherit_births || births.is_empty() {
        return;
    }
    let mut parents: Vec<_> = care
        .policies
        .par_iter()
        .map(|p| (p.person, p.willing))
        .collect();
    if parents.par_windows(2).any(|p| p[0].0 > p[1].0) {
        parents.par_sort_unstable_by_key(|p| p.0);
    }
    let additions: Vec<_> = births
        .par_iter()
        .map(|&(person, ids, count)| {
            let parent = if count == 0 {
                None
            } else {
                ids.get((person % count as u64) as usize).copied()
            };
            let willing = parent
                .and_then(|id| parents.binary_search_by_key(&id, |p| p.0).ok())
                .is_some_and(|i| parents[i].1);
            Policy { person, willing }
        })
        .collect();
    care.policies.par_extend(additions);
}

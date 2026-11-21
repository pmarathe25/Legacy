//! Individual, configurable development from actual care and unattended periods.
use game_economy_family::Families;
use game_economy_goals::Dreams;
use rayon::prelude::*;

/// Transfer only a general learning inclination to actual newborns, not aspirations or memories.
pub fn enroll_dream_learning(dreams: &mut Option<Dreams>, family: Option<&Families>, tick: u64) {
    let Some((dreams, family)) = dreams.as_mut().zip(family).filter(|(dreams, family)| {
        dreams.inherit_births && family.rules.enroll_births && !dreams.learning.is_empty()
    }) else {
        return;
    };
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .collect();
    if newborns.is_empty() {
        return;
    }
    let mut parents: Vec<_> = dreams.learning.par_iter().collect();
    if parents
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        parents.par_sort_unstable_by_key(|row| row.person);
    }
    let inherited: Vec<_> = newborns
        .into_par_iter()
        .filter_map(|child| {
            if parents
                .binary_search_by_key(&child.id, |row| row.person)
                .is_ok()
            {
                return None;
            }
            let parent = child.parents.iter().find_map(|id| {
                parents
                    .binary_search_by_key(id, |row| row.person)
                    .ok()
                    .map(|i| parents[i])
            })?;
            let mut learning = parent.clone();
            learning.person = child.id;
            Some(learning)
        })
        .collect();
    dreams.learning.par_extend(inherited);
}

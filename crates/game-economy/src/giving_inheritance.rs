//! Optional development of general helping inclinations for actual newborns.
use crate::{Families, Giving};
use rayon::prelude::*;
use std::collections::HashSet;

pub fn enroll(owner: Option<&mut Giving>, family: Option<&Families>, tick: u64) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
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
    let mut responses: Vec<_> = owner.responses.par_iter().collect();
    responses.par_sort_unstable_by_key(|row| row.person);
    let mut learning: Vec<_> = owner.learning.par_iter().collect();
    learning.par_sort_unstable_by_key(|row| row.person);
    let mut asking: Vec<_> = owner.acquaintance_asking.par_iter().collect();
    asking.par_sort_unstable_by_key(|row| row.person);
    let existing: HashSet<_> = owner
        .policies
        .par_iter()
        .map(|row| row.person)
        .chain(owner.responses.par_iter().map(|row| row.person))
        .collect();
    let inherited_responses: Vec<_> = newborns
        .par_iter()
        .filter_map(|member| {
            if existing.contains(&member.id) {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                responses
                    .binary_search_by_key(id, |row| row.person)
                    .ok()
                    .map(|i| responses[i])
            })?;
            let mut response = parent.clone();
            response.person = member.id;
            Some(response)
        })
        .collect();
    let mut response_people = existing;
    response_people.par_extend(inherited_responses.par_iter().map(|row| row.person));
    let inherited_learning: Vec<_> = newborns
        .par_iter()
        .filter_map(|member| {
            if !response_people.contains(&member.id)
                || learning
                    .binary_search_by_key(&member.id, |row| row.person)
                    .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                learning
                    .binary_search_by_key(id, |row| row.person)
                    .ok()
                    .map(|i| learning[i])
            })?;
            let mut rule = parent.clone();
            rule.person = member.id;
            rule.experiences = 0;
            rule.last_helper = None;
            if let Some(asking) = &mut rule.ask_helper {
                asking.next_review = tick;
            }
            Some(rule)
        })
        .collect();
    let inherited_asking: Vec<_> = newborns
        .into_par_iter()
        .filter_map(|member| {
            if asking
                .binary_search_by_key(&member.id, |row| row.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                asking
                    .binary_search_by_key(id, |row| row.person)
                    .ok()
                    .map(|i| asking[i])
            })?;
            let mut rule = parent.clone();
            rule.person = member.id;
            rule.next_review = tick;
            rule.last_target = None;
            Some(rule)
        })
        .collect();
    owner.responses.par_extend(inherited_responses);
    owner.learning.par_extend(inherited_learning);
    owner.acquaintance_asking.par_extend(inherited_asking);
}

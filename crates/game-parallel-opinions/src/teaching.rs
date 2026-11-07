//! Each participant interprets a lesson using their own directed disposition.
use crate::{Opinions, PersonalChange, SubjectiveReaction, TeachingReaction};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Gave,
    Received,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub observer: u64,
    pub other: u64,
    pub role: Role,
}

fn reaction(rule: TeachingReaction, role: Role) -> Option<SubjectiveReaction> {
    match role {
        Role::Gave => rule.given,
        Role::Received => rule.received,
    }
}

pub fn apply(owner: &mut Opinions, experiences: &[Experience], tick: u64) {
    if experiences.is_empty() {
        return;
    }
    let mut general: Vec<_> = owner
        .sensitivities
        .par_iter()
        .filter_map(|row| {
            row.teaching_reaction
                .as_deref()
                .map(|rule| (row.person, rule))
        })
        .collect();
    if general.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        general.par_sort_unstable_by_key(|row| row.0);
    }
    let mut specific: Vec<_> = owner
        .sensitivities
        .par_iter()
        .flat_map_iter(|row| {
            row.relationship_cases.iter().filter_map(move |case| {
                case.teaching_reaction
                    .as_deref()
                    .map(|rule| ((row.person, case.target), rule))
            })
        })
        .collect();
    if specific.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        specific.par_sort_unstable_by_key(|row| row.0);
    }
    if general.is_empty() && specific.is_empty() {
        return;
    }
    let dense = general
        .first()
        .zip(general.last())
        .and_then(|(first, last)| {
            (last.0.checked_sub(first.0) == Some((general.len() - 1) as u64)).then_some(first.0)
        });
    let changes: Vec<_> = experiences
        .par_iter()
        .filter_map(|experience| {
            if experience.observer == experience.other {
                return None;
            }
            let particular = specific
                .binary_search_by_key(&(experience.observer, experience.other), |row| row.0)
                .ok()
                .and_then(|index| reaction(*specific[index].1, experience.role));
            let general_rule = || {
                let index = dense.map_or_else(
                    || {
                        general
                            .binary_search_by_key(&experience.observer, |row| row.0)
                            .ok()
                    },
                    |first| {
                        experience
                            .observer
                            .checked_sub(first)
                            .and_then(|offset| usize::try_from(offset).ok())
                            .filter(|&index| index < general.len())
                    },
                )?;
                reaction(*general[index].1, experience.role)
            };
            let value = particular.or_else(general_rule)?;
            Some(PersonalChange {
                observer: experience.observer,
                target: experience.other,
                trust: value.trust,
                affinity: value.affinity,
                fear: value.fear,
                obligation: value.obligation,
            })
        })
        .collect();
    crate::personal_change::apply_owned(owner, changes, tick);
}

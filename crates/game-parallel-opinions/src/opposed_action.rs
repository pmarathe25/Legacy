//! Individual interpretations of an action directly opposed by another person.
use crate::{Opinions, OpposedActionReaction, PersonalChange, SubjectiveReaction};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Blocked,
    Overcame,
    BlockedOther,
    WasOvercome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub observer: u64,
    pub other: u64,
    pub outcome: Outcome,
}

fn interpretation(rule: OpposedActionReaction, outcome: Outcome) -> Option<SubjectiveReaction> {
    match outcome {
        Outcome::Blocked => rule.blocked,
        Outcome::Overcame => rule.overcame,
        Outcome::BlockedOther => rule.blocked_other,
        Outcome::WasOvercome => rule.was_overcome,
    }
}

pub fn apply(owner: &mut Opinions, experiences: &[Experience], tick: u64) {
    if experiences.is_empty() {
        return;
    }
    let mut general: Vec<_> = owner
        .sensitivities
        .par_iter()
        .filter(|row| row.opposed_action_reaction.is_some())
        .collect();
    if general
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        general.par_sort_unstable_by_key(|row| row.person);
    }
    let mut specific: Vec<_> = owner
        .sensitivities
        .par_iter()
        .flat_map_iter(|row| {
            row.relationship_cases.iter().filter_map(move |case| {
                case.opposed_action_reaction
                    .as_ref()
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
            (last.person.checked_sub(first.person) == Some((general.len() - 1) as u64))
                .then_some(first.person)
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
                .and_then(|index| interpretation(**specific[index].1, experience.outcome));
            let general_rule = || {
                let index = dense.map_or_else(
                    || {
                        general
                            .binary_search_by_key(&experience.observer, |row| row.person)
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
                general[index]
                    .opposed_action_reaction
                    .as_deref()
                    .and_then(|rule| interpretation(*rule, experience.outcome))
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
    owner.apply_personal_changes(&changes, tick);
}

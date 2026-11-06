//! Personal interpretations of directly experienced capture and release.
use crate::{ConfinementReaction, Opinions, PersonalChange};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperienceKind {
    Captured,
    Resisted,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    pub observer: u64,
    pub actor: u64,
    pub kind: ExperienceKind,
}

pub fn apply(owner: &mut Opinions, experiences: &[Experience], tick: u64) {
    if experiences.is_empty() {
        return;
    }
    let mut general: Vec<_> = owner
        .sensitivities
        .par_iter()
        .filter_map(|row| {
            row.confinement_reaction
                .map(|reaction| (row.person, reaction))
        })
        .collect();
    if general.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        general.par_sort_unstable_by_key(|row| row.0);
    }
    let mut cases: Vec<_> = owner
        .sensitivities
        .par_iter()
        .flat_map_iter(|row| {
            row.relationship_cases.iter().filter_map(move |case| {
                case.confinement_reaction
                    .map(|reaction| ((row.person, case.target), reaction))
            })
        })
        .collect();
    if cases.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        cases.par_sort_unstable_by_key(|row| row.0);
    }
    if general.is_empty() && cases.is_empty() {
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
        .filter_map(|event| {
            if event.observer == event.actor {
                return None;
            }
            let specific = cases
                .binary_search_by_key(&(event.observer, event.actor), |row| row.0)
                .ok()
                .map(|i| cases[i].1);
            let reaction: ConfinementReaction = specific.or_else(|| {
                let index = dense
                    .and_then(|base| event.observer.checked_sub(base))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .filter(|&i| i < general.len())
                    .or_else(|| {
                        dense
                            .is_none()
                            .then(|| {
                                general
                                    .binary_search_by_key(&event.observer, |row| row.0)
                                    .ok()
                            })
                            .flatten()
                    })?;
                Some(general[index].1)
            })?;
            let value = match event.kind {
                ExperienceKind::Captured => reaction.captured,
                ExperienceKind::Resisted => reaction.resisted,
                ExperienceKind::Released => reaction.released,
            };
            Some(PersonalChange {
                observer: event.observer,
                target: event.actor,
                trust: value.trust,
                affinity: value.affinity,
                fear: value.fear,
                obligation: value.obligation,
            })
        })
        .collect();
    owner.apply_personal_changes(&changes, tick);
}

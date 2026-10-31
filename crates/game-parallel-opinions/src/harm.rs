//! Directed responses to actual personal experience.
use crate::{Event, Opinions};
use game_parallel_opinions_model::Change;
use rayon::prelude::*;
pub fn apply(state: &mut Opinions, harm: &[(u64, u64, u32)]) {
    if harm.is_empty() || !state.tracks_harm() {
        return;
    }
    let mut sensitivities: Vec<_> = state.sensitivities.par_iter().collect();
    if sensitivities
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        sensitivities.par_sort_unstable_by_key(|row| row.person);
    }
    let mut rows: Vec<_> = harm
        .par_iter()
        .filter(|row| row.0 != row.1 && row.2 > 0)
        .copied()
        .collect();
    rows.par_sort_unstable_by_key(|row| (row.0, row.1));
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
        .filter_map(|group| {
            let gain = sensitivities
                .binary_search_by_key(&group[0].0, |row| row.person)
                .ok()
                .and_then(|index| sensitivities[index].fear_per_damage)
                .unwrap_or(state.rules.fear_per_damage);
            if gain == 0 {
                return None;
            }
            let total = group
                .par_iter()
                .map(|row| u64::from(row.2))
                .reduce(|| 0, u64::saturating_add);
            Some(((group[0].0, group[0].1), total.saturating_mul(gain)))
        })
        .collect();
    merge(state, &updates, Change::Fear);
}

pub fn help(state: &mut Opinions, help: &[(u64, u64)]) {
    if help.is_empty() || !state.tracks_help() {
        return;
    }
    let mut sensitivities: Vec<_> = state.sensitivities.par_iter().collect();
    if sensitivities
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        sensitivities.par_sort_unstable_by_key(|row| row.person);
    }
    let mut cases: Vec<_> = state
        .sensitivities
        .par_iter()
        .flat_map_iter(|p| {
            p.relationship_cases
                .iter()
                .map(move |case| (p.person, case))
        })
        .collect();
    cases.par_sort_unstable_by_key(|row| (row.0, row.1.target));
    let mut rows: Vec<_> = help
        .par_iter()
        .filter(|row| row.0 != row.1)
        .copied()
        .collect();
    if rows.par_windows(2).any(|pair| pair[0] > pair[1]) {
        rows.par_sort_unstable();
    }
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| a == b)
        .filter_map(|group| {
            let gain = cases
                .binary_search_by_key(&group[0], |row| (row.0, row.1.target))
                .ok()
                .and_then(|i| cases[i].1.obligation_gain)
                .or_else(|| {
                    sensitivities
                        .binary_search_by_key(&group[0].0, |row| row.person)
                        .ok()
                        .and_then(|index| sensitivities[index].gratitude_gain)
                })
                .unwrap_or(state.rules.gratitude_gain);
            (gain > 0).then_some((group[0], gain.saturating_mul(group.len() as u64)))
        })
        .collect();
    let can_relieve = (state.rules.obligation_relief > 0
        || sensitivities.par_iter().any(|row| {
            row.obligation_relief.is_some_and(|rate| rate > 0)
                || row
                    .relationship_cases
                    .iter()
                    .any(|case| case.obligation_relief.is_some_and(|rate| rate > 0))
        }))
        && state.records.par_iter().any(|row| row.obligation > 0);
    let relief: Vec<_> = if can_relieve {
        let mut reciprocated: Vec<_> = rows
            .par_iter()
            .map(|&(recipient, helper)| (helper, recipient))
            .collect();
        if reciprocated.par_windows(2).any(|pair| pair[0] > pair[1]) {
            reciprocated.par_sort_unstable();
        }
        let index = state.index();
        reciprocated
            .par_chunk_by(|a, b| a == b)
            .filter_map(|group| {
                let gain = cases
                    .binary_search_by_key(&group[0], |row| (row.0, row.1.target))
                    .ok()
                    .and_then(|i| cases[i].1.obligation_relief)
                    .or_else(|| {
                        sensitivities
                            .binary_search_by_key(&group[0].0, |row| row.person)
                            .ok()
                            .and_then(|i| sensitivities[i].obligation_relief)
                    })
                    .unwrap_or(state.rules.obligation_relief);
                let previous = index.obligation(group[0].0, group[0].1);
                (gain > 0 && previous > 0).then_some((
                    group[0],
                    gain.saturating_mul(group.len() as u64).min(previous),
                ))
            })
            .collect()
    } else {
        Vec::new()
    };
    merge(state, &updates, Change::Help);
    merge(state, &relief, Change::Relief);
}

pub fn theft(state: &mut Opinions, incidents: &[(u64, u64)], tick: u64) {
    if incidents.is_empty()
        || (state.rules.theft_loss == 0
            && state
                .sensitivities
                .par_iter()
                .all(|row| row.theft_loss.unwrap_or(0) == 0))
    {
        return;
    }
    let mut sensitivities: Vec<_> = state.sensitivities.par_iter().collect();
    if sensitivities
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        sensitivities.par_sort_unstable_by_key(|row| row.person);
    }
    let mut rows: Vec<_> = incidents
        .par_iter()
        .copied()
        .filter(|row| row.0 != row.1)
        .collect();
    if rows.par_windows(2).any(|pair| pair[0] > pair[1]) {
        rows.par_sort_unstable();
    }
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| a == b)
        .filter_map(|group| {
            let loss = sensitivities
                .binary_search_by_key(&group[0].0, |row| row.person)
                .ok()
                .and_then(|index| sensitivities[index].theft_loss)
                .unwrap_or(state.rules.theft_loss);
            (loss > 0).then_some((
                group[0],
                (group.len() as u64).saturating_mul(u64::from(loss)),
            ))
        })
        .collect();
    let index = state.index();
    let events: Vec<_> = updates
        .par_iter()
        .map(|&((observer, target), delta)| {
            let before = index.trust(observer, target);
            let after = (i128::from(before) - i128::from(delta)).clamp(
                i128::from(state.rules.minimum),
                i128::from(state.rules.maximum),
            ) as i32;
            Event {
                tick,
                observer,
                target,
                before,
                after,
            }
        })
        .collect();
    state.events.par_extend(events);
    merge(state, &updates, Change::Theft);
}

fn merge(state: &mut Opinions, updates: &[((u64, u64), u64)], kind: Change) {
    game_parallel_opinions_model::merge(&mut state.records, &state.rules, updates, kind);
}

/// A character's interpretation of an experience; no sign is imposed by the engine.
#[derive(Clone, Copy)]
pub enum AffinityKind {
    Help,
    Harm,
    Encounter,
    PressuredEncounter,
    SiteUse,
    SiteDestruction,
    SiteSalvage,
}
impl AffinityKind {
    fn rate(
        self,
        rules: &game_parallel_opinions_model::AffinityRules,
        personal: Option<&game_parallel_opinions_model::Sensitivity>,
        case: Option<&game_parallel_opinions_model::RelationshipCase>,
    ) -> i32 {
        match self {
            Self::Help => case
                .and_then(|c| c.help)
                .or_else(|| personal.and_then(|p| p.affinity_help))
                .unwrap_or(rules.help_change),
            Self::Harm => case
                .and_then(|c| c.harm_per_damage)
                .or_else(|| personal.and_then(|p| p.affinity_harm_per_damage))
                .unwrap_or(rules.harm_change_per_damage),
            Self::Encounter => case
                .and_then(|c| c.encounter)
                .or_else(|| personal.and_then(|p| p.affinity_encounter))
                .unwrap_or(rules.encounter_change),
            Self::PressuredEncounter => case
                .and_then(|c| c.pressured_encounter)
                .or_else(|| personal.and_then(|p| p.affinity_pressured_encounter))
                .unwrap_or(rules.pressured_encounter_change),
            Self::SiteUse => case
                .and_then(|c| c.site_use)
                .or_else(|| personal.and_then(|p| p.affinity_site_use))
                .unwrap_or(0),
            Self::SiteDestruction => case
                .and_then(|c| c.site_destruction)
                .or_else(|| personal.and_then(|p| p.affinity_site_destruction))
                .unwrap_or(0),
            Self::SiteSalvage => case
                .and_then(|c| c.site_salvage)
                .or_else(|| personal.and_then(|p| p.affinity_site_salvage))
                .unwrap_or(0),
        }
    }
}
pub fn tracks_affinity(state: &Opinions, kind: AffinityKind) -> bool {
    let Some(rules) = &state.rules.affinity else {
        return false;
    };
    kind.rate(rules, None, None) != 0
        || state.sensitivities.par_iter().any(|p| {
            kind.rate(rules, Some(p), None) != 0
                || p.relationship_cases
                    .iter()
                    .any(|case| kind.rate(rules, Some(p), Some(case)) != 0)
        })
}
/// Aggregate by directed pair before publishing; hot pairs remain parallel.
pub fn affinity(state: &mut Opinions, events: &[(u64, u64, u64)], kind: AffinityKind) {
    if events.is_empty() || !tracks_affinity(state, kind) {
        return;
    }
    let mut sensitivities: Vec<_> = state.sensitivities.par_iter().collect();
    if sensitivities
        .par_windows(2)
        .any(|p| p[0].person > p[1].person)
    {
        sensitivities.par_sort_unstable_by_key(|p| p.person);
    }
    let mut cases: Vec<_> = state
        .sensitivities
        .par_iter()
        .flat_map_iter(|p| {
            p.relationship_cases
                .iter()
                .map(move |case| (p.person, case))
        })
        .collect();
    cases.par_sort_unstable_by_key(|row| (row.0, row.1.target));
    let mut rows: Vec<_> = events
        .par_iter()
        .copied()
        .filter(|r| r.0 != r.1 && r.2 > 0)
        .collect();
    rows.par_sort_unstable_by_key(|r| (r.0, r.1));
    let rules = state.rules.affinity.as_ref().expect("affinity enabled");
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
        .filter_map(|group| {
            let personal = sensitivities
                .binary_search_by_key(&group[0].0, |p| p.person)
                .ok()
                .map(|i| sensitivities[i]);
            let case = cases
                .binary_search_by_key(&(group[0].0, group[0].1), |row| (row.0, row.1.target))
                .ok()
                .map(|i| cases[i].1);
            let rate = kind.rate(rules, personal, case);
            if rate == 0 {
                return None;
            }
            let total = if group.len() >= 2048 {
                group
                    .par_iter()
                    .map(|r| r.2)
                    .reduce(|| 0, u64::saturating_add)
            } else {
                group.iter().fold(0u64, |sum, r| sum.saturating_add(r.2))
            };
            Some((
                (group[0].0, group[0].1),
                total.saturating_mul(u64::from(rate.unsigned_abs())),
                rate > 0,
            ))
        })
        .collect();
    let gains: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, amount, positive)| positive.then_some((pair, amount)))
        .collect();
    let losses: Vec<_> = updates
        .par_iter()
        .filter_map(|&(pair, amount, positive)| (!positive).then_some((pair, amount)))
        .collect();
    merge(state, &gains, Change::AffinityGain);
    merge(state, &losses, Change::AffinityLoss);
}

/// Each maker interprets their own promise; no agreement compels an action.
pub fn promises(state: &mut Opinions, promises: &[(u64, u64)]) {
    let mut sensitivities: Vec<_> = state.sensitivities.par_iter().collect();
    sensitivities.par_sort_unstable_by_key(|p| p.person);
    let mut cases: Vec<_> = state
        .sensitivities
        .par_iter()
        .flat_map_iter(|p| {
            p.relationship_cases
                .iter()
                .map(move |case| (p.person, case))
        })
        .collect();
    cases.par_sort_unstable_by_key(|row| (row.0, row.1.target));
    let mut rows: Vec<_> = promises
        .par_iter()
        .copied()
        .filter(|(a, b)| a != b)
        .collect();
    rows.par_sort_unstable();
    let updates: Vec<_> = rows
        .par_chunk_by(|a, b| a == b)
        .filter_map(|group| {
            let gain = cases
                .binary_search_by_key(&group[0], |row| (row.0, row.1.target))
                .ok()
                .and_then(|i| cases[i].1.promise_obligation_gain)
                .or_else(|| {
                    sensitivities
                        .binary_search_by_key(&group[0].0, |p| p.person)
                        .ok()
                        .and_then(|i| sensitivities[i].promise_obligation_gain)
                })
                .unwrap_or(0);
            (gain > 0).then_some((group[0], gain.saturating_mul(group.len() as u64)))
        })
        .collect();
    merge(state, &updates, Change::Help);
}

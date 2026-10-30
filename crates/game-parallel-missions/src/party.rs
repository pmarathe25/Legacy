//! Independent party coordination over identity-sorted groups.
use crate::{Departure, Mission, Phase, phase::Edit};
use rayon::prelude::*;

fn active(phase: Phase) -> bool {
    !matches!(phase, Phase::Completed | Phase::Failed)
}

pub fn valid(missions: &[Mission]) -> bool {
    let mut groups: Vec<_> = missions
        .par_iter()
        .filter_map(|row| row.party.map(|party| (party, row)))
        .collect();
    groups.par_sort_unstable_by_key(|(party, row)| (*party, row.id));
    groups.par_chunk_by(|a, b| a.0 == b.0).all(|group| {
        let (id, first) = group[0];
        if group.len() < 2
            || first.id != id
            || group.iter().any(|(_, row)| {
                (!row.party_opt_in && !row.party_compliant)
                    || (row.origin, row.destination, row.visit_ticks)
                        != (first.origin, first.destination, first.visit_ticks)
            })
        {
            return false;
        }
        let mut living = group
            .iter()
            .filter(|(_, row)| active(row.phase))
            .map(|(_, row)| (row.phase, row.remaining));
        living
            .next()
            .is_none_or(|head| living.all(|state| state == head))
    })
}

/// A party departs only when every remaining active participant is ready.
pub fn ready(missions: &[Mission], departures: &[Departure], edits: &[Edit]) -> Vec<u64> {
    let dense = departures.first().and_then(|first| {
        (departures.last()?.index.checked_sub(first.index) == Some(departures.len() - 1))
            .then_some(first.index)
    });
    let mut groups: Vec<_> = missions
        .par_iter()
        .enumerate()
        .filter_map(|(index, mission)| {
            let party = mission.party.filter(|_| active(mission.phase))?;
            if edits
                .binary_search_by_key(&index, |edit| edit.index)
                .ok()
                .is_some_and(|i| !active(edits[i].phase))
            {
                return None;
            }
            let slot = dense
                .and_then(|first| index.checked_sub(first))
                .filter(|&i| i < departures.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| {
                            departures
                                .binary_search_by_key(&index, |row| row.index)
                                .ok()
                        })
                        .flatten()
                });
            let departure = slot.map(|i| {
                let row = departures[i];
                (row.source, row.destination, row.outbound)
            });
            Some((party, departure))
        })
        .collect();
    if groups.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        groups.par_sort_unstable_by_key(|row| row.0);
    }
    groups
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| {
            group[0]
                .1
                .filter(|first| group.iter().all(|row| row.1 == Some(*first)))
                .map(|_| group[0].0)
        })
        .collect()
}

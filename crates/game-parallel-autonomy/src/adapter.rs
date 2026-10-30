//! Snapshot assembly and outcome receipts for the host.
use crate::{Autonomy, Draft, Event, Outcome, Path, Situation};
use game_parallel_knowledge::Knowledge;
use game_parallel_people::Region;
use game_parallel_transport::{Move, Route, Traveler};
use rayon::prelude::*;
use std::collections::HashSet;

pub fn prepare(
    autonomy: Option<&Autonomy>,
    knowledge: Option<&Knowledge>,
    regions: &[Region],
    travelers: &[Traveler],
    routes: &[Route],
    tick: u64,
    busy: &[u64],
) -> Result<Option<Draft>, &'static str> {
    let Some(autonomy) = autonomy else {
        return Ok(None);
    };
    let knowledge = knowledge.ok_or("autonomy requires knowledge")?;
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let situations: Vec<_> = regions
        .par_iter()
        .flat_map(|region| {
            region.people.par_iter().map(|p| Situation {
                busy: busy.binary_search(&p.id).is_ok(),
                person: p.id,
                region: Some(region.id),
                health: p.health,
                hunger: p.hunger,
            })
        })
        .chain(travelers.par_iter().map(|t| Situation {
            busy: busy.binary_search(&t.person.id).is_ok(),
            person: t.person.id,
            region: None,
            health: t.person.health,
            hunger: t.person.hunger,
        }))
        .collect();
    let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
    if region_ids.len() != regions.len() {
        return Err("duplicate region id");
    }
    let paths: Vec<_> = routes
        .par_iter()
        .map(|r| Path {
            source: r.source,
            destination: r.destination,
            ticks: r.ticks,
        })
        .collect();
    autonomy
        .prepare(&situations, &paths, &knowledge.minds, &region_ids, tick)
        .map(Some)
}

pub fn requests(draft: &Draft, regions: &[Region]) -> (Vec<Move>, Vec<bool>) {
    let mut local: Vec<_> = regions
        .par_iter()
        .flat_map(|r| {
            r.people
                .par_iter()
                .filter(|p| p.health > 0)
                .map(|p| (p.id, r.id))
        })
        .collect();
    local.par_sort_unstable();
    let available: Vec<_> = draft
        .decisions
        .par_iter()
        .map(|d| local.binary_search(&(d.person, d.source)).is_ok())
        .collect();
    let requests: Vec<_> = draft
        .decisions
        .par_iter()
        .zip(available.par_iter())
        .filter(|(_, yes)| **yes)
        .map(|(d, _)| Move {
            person: d.person,
            destination: d.destination,
        })
        .collect();
    (requests, available)
}

pub fn finish(
    mut draft: Draft,
    available: &[bool],
    result: &Result<(), String>,
    tick: u64,
) -> Autonomy {
    draft.state.events = draft
        .decisions
        .par_iter()
        .zip(available.par_iter())
        .map(|(d, &yes)| {
            let outcome = if !yes {
                Outcome::Unavailable
            } else if result.is_ok() {
                Outcome::Started
            } else {
                Outcome::Rejected
            };
            Event {
                tick,
                person: d.person,
                source: d.source,
                destination: d.destination,
                outcome,
                reason: if yes {
                    result.as_ref().err().cloned()
                } else {
                    Some("person unavailable at departure".into())
                },
            }
        })
        .collect();
    draft.state
}
/// Prepare decisions from the compiled runtime's shared physical index.
pub fn prepare_indexed(
    autonomy: &Autonomy,
    knowledge: &Knowledge,
    regions: &[Region],
    people: &[game_parallel_production_model::PersonSnapshot],
    routes: &[Route],
    busy: &[u64],
    tick: u64,
) -> Result<Draft, &'static str> {
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let situations: Vec<_> = people
        .par_iter()
        .map(|&(person, region, health, hunger)| Situation {
            person,
            region,
            health,
            hunger,
            busy: busy.binary_search(&person).is_ok(),
        })
        .collect();
    let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
    if region_ids.len() != regions.len() {
        return Err("duplicate region id");
    }
    let paths: Vec<_> = routes
        .par_iter()
        .map(|r| Path {
            source: r.source,
            destination: r.destination,
            ticks: r.ticks,
        })
        .collect();
    autonomy.prepare_validated(&situations, &paths, &knowledge.minds, &region_ids, tick)
}
/// Resolve decision availability against a committed physical index.
pub fn requests_indexed(
    draft: &Draft,
    people: &[game_parallel_production_model::PersonSnapshot],
) -> (Vec<Move>, Vec<bool>) {
    use game_parallel_production_model::person_index;
    let available: Vec<_> = draft
        .decisions
        .par_iter()
        .map(|decision| {
            person_index(people, decision.person)
                .is_some_and(|i| people[i].1 == Some(decision.source) && people[i].2 > 0)
        })
        .collect();
    let requests: Vec<_> = draft
        .decisions
        .par_iter()
        .zip(available.par_iter())
        .filter(|(_, yes)| **yes)
        .map(|(decision, _)| Move {
            person: decision.person,
            destination: decision.destination,
        })
        .collect();
    (requests, available)
}

/// Compiled adapter avoids reconstructing editable minds.
pub fn prepare_records_indexed<'a>(
    autonomy: &Autonomy,
    records_for: impl Fn(u64) -> Option<&'a [game_parallel_knowledge::Record]> + Sync,
    regions: &[Region],
    people: &[game_parallel_production_model::PersonSnapshot],
    routes: &[Route],
    busy: &[u64],
    tick: u64,
) -> Result<Draft, &'static str> {
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let situations: Vec<_> = people
        .par_iter()
        .map(|&(person, region, health, hunger)| Situation {
            person,
            region,
            health,
            hunger,
            busy: busy.binary_search(&person).is_ok(),
        })
        .collect();
    let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
    if region_ids.len() != regions.len() {
        return Err("duplicate region id");
    }
    let paths: Vec<_> = routes
        .par_iter()
        .map(|r| Path {
            source: r.source,
            destination: r.destination,
            ticks: r.ticks,
        })
        .collect();
    let mut paths = paths;
    paths.par_sort_unstable_by_key(|p| (p.source, p.destination));
    autonomy.prepare_records_validated(&situations, &paths, records_for, tick)
}

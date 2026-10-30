//! Persistent infection columns and deterministic encounter routing.
use game_parallel_disease::{Case, Disease, Event, EventKind, Strain};
use game_parallel_fused_data::{Block, DiseaseColumns};
use game_parallel_social::Encounter;
use rayon::prelude::*;
use std::collections::HashMap;

/// Called only after world validation. Preserve original case order until commit.
pub fn import(state: &Disease, blocks: &mut [Block]) {
    let cases: HashMap<_, _> = state
        .cases
        .par_iter()
        .map(|case| (case.person, case))
        .collect();
    blocks.par_iter_mut().for_each(|block| {
        let mut columns = DiseaseColumns::empty(block.ids.len());
        for (i, id) in block.ids.iter().enumerate() {
            if let Some(case) = cases.get(id) {
                columns.strains[i] = case.strain;
                columns.infected_at[i] = case.infected_at;
                columns.sources[i] = case.source.unwrap_or(0);
                columns.flags[i] = 1 | (u8::from(case.source.is_some()) << 1);
            }
        }
        block.disease = Some(columns);
    });
}
pub fn ensure(blocks: &mut [Block]) {
    blocks.par_iter_mut().for_each(|block| {
        if block.disease.is_none() {
            block.disease = Some(DiseaseColumns::empty(block.ids.len()));
        }
    });
}
pub fn export(state: &mut Disease, blocks: &[Block], positions: &[(u64, usize, usize)]) {
    state.cases = positions
        .par_iter()
        .filter_map(|&(id, block, i)| {
            let columns = blocks[block].disease.as_ref().expect("disease columns");
            (columns.flags[i] & 1 != 0).then(|| Case {
                person: id,
                strain: columns.strains[i],
                infected_at: columns.infected_at[i],
                source: (columns.flags[i] & 2 != 0).then_some(columns.sources[i]),
            })
        })
        .collect();
}
pub fn find_strain(strains: &[Strain], id: u32) -> &Strain {
    strains
        .iter()
        .find(|strain| strain.id == id)
        .expect("validated strain")
}
fn end(strain: &Strain) -> u64 {
    strain.incubation + strain.infectious_ticks + strain.immunity_ticks
}
/// Read tick-start cases, then write exclusively to each block's scratch lanes.
/// The host guarantees validated identities and disjoint social encounters.
pub fn prepare(
    state: &Disease,
    blocks: &mut [Block],
    positions: &[(u64, usize, usize)],
    encounters: &[Encounter],
    seed: u64,
    tick: u64,
) -> Result<Vec<Event>, &'static str> {
    let next = tick.checked_add(1).ok_or("tick overflow")?;
    let dense = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let index = |id: u64| -> usize {
        dense
            .and_then(|first| id.checked_sub(first))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < positions.len())
            .or_else(|| {
                if dense.is_none() {
                    positions.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .expect("validated disease identity")
    };
    let read = |person: u64| {
        let (_, block, i) = positions[index(person)];
        let block = &blocks[block];
        let columns = block.disease.as_ref().expect("disease columns");
        let case = (columns.flags[i] & 1 != 0)
            .then(|| {
                let strain = find_strain(&state.strains, columns.strains[i]);
                let age = tick - columns.infected_at[i];
                (strain, age)
            })
            .filter(|(strain, age)| *age < end(strain));
        (block.health[i], case)
    };
    let mut infections: Vec<Case> = encounters
        .par_iter()
        .filter_map(|encounter| {
            let (a_health, a) = read(encounter.a);
            let (b_health, b) = read(encounter.b);
            let attempt = |source: u64,
                           target: u64,
                           case: Option<(&Strain, u64)>,
                           target_case: Option<(&Strain, u64)>| {
                let (strain, age) = case?;
                if a_health == 0
                    || b_health == 0
                    || !strain.contagious(age)
                    || target_case.is_some()
                    || game_parallel_genetics::random(seed ^ source, target, next) % 1_000_000
                        >= u64::from(strain.transmission_per_million)
                {
                    return None;
                }
                Some(Case {
                    person: target,
                    strain: strain.id,
                    infected_at: next,
                    source: Some(source),
                })
            };
            attempt(encounter.a, encounter.b, a, b)
                .or_else(|| attempt(encounter.b, encounter.a, b, a))
        })
        .collect();
    infections.par_sort_unstable_by_key(|case| case.person);
    let mut events: Vec<_> = blocks
        .par_iter_mut()
        .flat_map_iter(|block| {
            let columns = block.disease.as_mut().expect("disease columns");
            let mut at = block
                .ids
                .first()
                .map_or(0, |id| infections.partition_point(|case| case.person < *id));
            let mut events = Vec::new();
            for (i, &id) in block.ids.iter().enumerate() {
                while at < infections.len() && infections[at].person < id {
                    at += 1;
                }
                let old = columns.flags[i] & 1 != 0;
                let mut keep = false;
                if old {
                    let strain = find_strain(&state.strains, columns.strains[i]);
                    let age = tick - columns.infected_at[i];
                    keep = age < end(strain) && block.health[i] > 0;
                    if age == strain.incubation + strain.infectious_ticks && block.health[i] > 0 {
                        events.push(Event {
                            tick: next,
                            person: id,
                            strain: strain.id,
                            source: (columns.flags[i] & 2 != 0).then_some(columns.sources[i]),
                            kind: EventKind::Recovered,
                        });
                    }
                }
                if let Some(case) = infections.get(at).filter(|case| case.person == id) {
                    columns.next_strains[i] = case.strain;
                    columns.next_infected_at[i] = case.infected_at;
                    columns.next_sources[i] = case.source.unwrap_or(0);
                    columns.next_flags[i] = 1 | (u8::from(case.source.is_some()) << 1);
                    events.push(Event {
                        tick: next,
                        person: id,
                        strain: case.strain,
                        source: case.source,
                        kind: EventKind::Infected,
                    });
                } else if keep {
                    columns.next_strains[i] = columns.strains[i];
                    columns.next_infected_at[i] = columns.infected_at[i];
                    columns.next_sources[i] = columns.sources[i];
                    columns.next_flags[i] = columns.flags[i];
                } else {
                    columns.next_flags[i] = 0;
                }
            }
            events
        })
        .collect();
    events.par_sort_unstable_by_key(|event| {
        (
            event.person,
            event.strain,
            match event.kind {
                EventKind::Recovered => 0,
                EventKind::Infected => 1,
                EventKind::Treated => 2,
            },
            event.source,
        )
    });
    Ok(events)
}

/// Complete a course against unpublished infection lanes. Multiple caregivers
/// may treat one patient, but a case is cleared and reported only once.
pub fn treat(
    state: &Disease,
    blocks: &mut [Block],
    medicine: u32,
    patients: &[u64],
    tick: u64,
    events: &mut Vec<Event>,
) {
    if patients.is_empty() || !state.treatments.iter().any(|t| t.medicine == medicine) {
        return;
    }
    let mut patients = patients.to_vec();
    patients.par_sort_unstable();
    patients.dedup();
    let mut treated: Vec<_> = blocks
        .par_iter_mut()
        .flat_map_iter(|block| {
            let columns = block.disease.as_mut().expect("disease columns");
            let mut cursor = block
                .ids
                .first()
                .map_or(0, |id| patients.partition_point(|patient| patient < id));
            let mut treated = Vec::new();
            for (i, &id) in block.ids.iter().enumerate() {
                while cursor < patients.len() && patients[cursor] < id {
                    cursor += 1;
                }
                if patients.get(cursor) == Some(&id)
                    && columns.next_flags[i] & 1 != 0
                    && state
                        .treatments
                        .iter()
                        .any(|t| t.medicine == medicine && t.strain == columns.next_strains[i])
                {
                    treated.push(Event {
                        tick,
                        person: id,
                        strain: columns.next_strains[i],
                        source: (columns.next_flags[i] & 2 != 0).then_some(columns.next_sources[i]),
                        kind: EventKind::Treated,
                    });
                    columns.next_flags[i] = 0;
                }
            }
            treated
        })
        .collect();
    events.append(&mut treated);
    events.par_sort_unstable_by_key(|event| {
        (
            event.person,
            event.strain,
            match event.kind {
                EventKind::Recovered => 0,
                EventKind::Infected => 1,
                EventKind::Treated => 2,
            },
            event.source,
        )
    });
}

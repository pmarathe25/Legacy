//! Fused care admission and unpublished healing.
use game_parallel::{Care, CareStart, World};
use game_parallel_care::{Event, Job};
use game_parallel_fused_data::{Block, NONE};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
pub struct Draft {
    pub state: Care,
    due: Vec<Job>,
    tick: u64,
}
pub fn prepare(world: &World, people: &[PersonSnapshot], tick: u64) -> Option<Draft> {
    world.care.as_ref().map(|care| {
        let (state, due) = game_parallel_care::prepare(care, people, tick);
        Draft { state, due, tick }
    })
}
fn locate(positions: &[(u64, usize, usize)], id: u64) -> Option<(usize, usize)> {
    let first = positions.first()?.0;
    let last = positions.last()?.0;
    let index = if last.checked_sub(first) == Some((positions.len() - 1) as u64) {
        usize::try_from(id.checked_sub(first)?)
            .ok()
            .filter(|&index| index < positions.len())?
    } else {
        positions.binary_search_by_key(&id, |row| row.0).ok()?
    };
    Some((positions[index].1, positions[index].2))
}
fn sort_events(events: &mut [Event]) {
    if events.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        events.par_sort_unstable_by_key(|event| event.id);
    }
}
impl Draft {
    pub fn apply(
        &mut self,
        blocks: &mut [Block],
        positions: &[(u64, usize, usize)],
        max_health: u32,
    ) {
        let mut due: Vec<_> = self
            .due
            .par_iter()
            .map(|job| {
                let (block, offset) = locate(positions, job.patient).expect("validated patient");
                let caregiver = locate(positions, job.caregiver)
                    .is_some_and(|(b, i)| blocks[b].next_health[i] > 0);
                (block, offset, job, caregiver)
            })
            .collect();
        if due
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1, pair[0].2.id) > (pair[1].0, pair[1].1, pair[1].2.id))
        {
            due.par_sort_unstable_by_key(|row| (row.0, row.1, row.2.id));
        }
        let healing = self.state.healing;
        let tick = self.tick;
        if due
            .par_windows(2)
            .all(|pair| (pair[0].0, pair[0].1) != (pair[1].0, pair[1].1))
        {
            let events: Vec<_> = due
                .par_iter()
                .map(|&(block, offset, job, caregiver)| {
                    let health = blocks[block].next_health[offset];
                    let valid = caregiver && health > 0;
                    let healed = if valid {
                        healing.min(max_health.saturating_sub(health))
                    } else {
                        0
                    };
                    Event {
                        tick,
                        id: job.id,
                        caregiver: job.caregiver,
                        patient: job.patient,
                        healed,
                        interrupted: !valid,
                    }
                })
                .collect();
            blocks
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, block)| {
                    let start = due.partition_point(|row| row.0 < index);
                    let end = due.partition_point(|row| row.0 <= index);
                    for row in start..end {
                        block.next_health[due[row].1] += events[row].healed;
                    }
                });
            self.state.events.extend(events);
            sort_events(&mut self.state.events);
            return;
        }
        let groups: Vec<_> = due
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| {
                let (block, offset, _, _) = group[0];
                let initial = blocks[block].next_health[offset];
                let available = if initial > 0 {
                    u128::from(max_health.saturating_sub(initial))
                } else {
                    0
                };
                let counts: Vec<usize> = group
                    .par_chunks(2048)
                    .map(|chunk| chunk.iter().filter(|row| row.3 && initial > 0).count())
                    .collect();
                let mut prefix = 0usize;
                let offsets: Vec<_> = counts
                    .into_iter()
                    .map(|count| {
                        let start = prefix;
                        prefix += count;
                        start
                    })
                    .collect();
                let gained = (u128::from(healing) * prefix as u128).min(available) as u32;
                let events: Vec<_> = group
                    .par_chunks(2048)
                    .zip(offsets.into_par_iter())
                    .flat_map_iter(|(chunk, mut rank)| {
                        chunk.iter().map(move |&(_, _, job, caregiver)| {
                            let valid = caregiver && initial > 0;
                            let used = u128::from(healing) * rank as u128;
                            let healed = if valid {
                                u128::from(healing).min(available.saturating_sub(used)) as u32
                            } else {
                                0
                            };
                            rank += usize::from(valid);
                            Event {
                                tick,
                                id: job.id,
                                caregiver: job.caregiver,
                                patient: job.patient,
                                healed,
                                interrupted: !valid,
                            }
                        })
                    })
                    .collect();
                (block, offset, gained, events)
            })
            .collect();
        blocks
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, block)| {
                let start = groups.partition_point(|row| row.0 < index);
                let end = groups.partition_point(|row| row.0 <= index);
                for &(_, offset, gained, _) in &groups[start..end] {
                    block.next_health[offset] += gained;
                }
            });
        let mut events: Vec<_> = groups
            .into_par_iter()
            .flat_map(|row| row.3.into_par_iter())
            .collect();
        self.state.events.append(&mut events);
        sort_events(&mut self.state.events);
    }
    /// Stop treatments whose caregiver or patient died in the lifecycle phase.
    pub fn finish(&mut self, blocks: &[Block], positions: &[(u64, usize, usize)]) {
        let tick = self.tick;
        let mut interrupted: Vec<_> = self
            .state
            .jobs
            .par_iter()
            .filter(|job| {
                !locate(positions, job.caregiver).is_some_and(|(b, i)| blocks[b].next_health[i] > 0)
                    || !locate(positions, job.patient)
                        .is_some_and(|(b, i)| blocks[b].next_health[i] > 0)
            })
            .map(|job| Event {
                tick,
                id: job.id,
                caregiver: job.caregiver,
                patient: job.patient,
                healed: 0,
                interrupted: true,
            })
            .collect();
        if !interrupted.is_empty() {
            let ids: std::collections::HashSet<_> =
                interrupted.iter().map(|event| event.id).collect();
            self.state.jobs = self
                .state
                .jobs
                .par_iter()
                .filter(|job| !ids.contains(&job.id))
                .cloned()
                .collect();
            self.state.events.append(&mut interrupted);
        }
        sort_events(&mut self.state.events);
    }
}
pub fn start(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    requests: &[CareStart],
) -> Result<(), String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        .is_some_and(|person| {
            requests
                .par_iter()
                .any(|request| request.caregiver == person)
        })
    {
        return Err("player control is suspended".into());
    }
    let people = game_parallel_fused_index::people(blocks, &world.regions, positions);
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &handle)| (handle != NONE).then_some(id))
        })
        .collect();
    if let Some(conflict) = &world.conflict {
        busy.extend(conflict.actors());
    }
    let care = world.care.as_ref().ok_or("care is disabled")?;
    if let Some(property) = &world.property {
        let rights = property.index();
        if requests.par_iter().any(|request| {
            person_index(&people, request.caregiver).is_none_or(|i| {
                people[i]
                    .1
                    .is_none_or(|region| !rights.allows(request.caregiver, region, care.medicine))
            })
        }) {
            return Err("medicine access denied".into());
        }
    }
    let production = world
        .production
        .as_mut()
        .ok_or("care requires production")?;
    world
        .care
        .as_mut()
        .unwrap()
        .start(&mut production.inventories, &people, &busy, requests)
}

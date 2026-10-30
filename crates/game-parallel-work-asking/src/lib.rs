//! Bounded personal requests derived from existing goals and local visibility.
use game_parallel_goals::Goals;
use game_parallel_production_model::{PersonSnapshot, person_index};
use game_parallel_work_requests::{Request, Requests};
use rayon::prelude::*;
use std::cmp::Reverse;

pub struct Draft {
    pub requests: Vec<Request>,
    pub actors: Vec<u64>,
    pub next_id: u64,
}
struct Candidate {
    requester: u64,
    worker: u64,
    region: u64,
    recipe: u32,
    expires: u64,
}

pub fn prepare(
    owner: &Requests,
    goals: Option<&Goals>,
    people: &[PersonSnapshot],
    busy: &[u64],
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    let Some(goals) = goals.filter(|_| !owner.askers.is_empty()) else {
        return Ok(None);
    };
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    if !owner.askers.par_iter().any(|asker| {
        tick % asker.interval == asker.person % asker.interval
            && busy.binary_search(&asker.person).is_err()
    }) {
        return Ok(None);
    }
    let mut goals: Vec<_> = goals
        .people
        .par_iter()
        .filter(|row| row.enabled && row.progress < row.target && row.next_review < tick)
        .collect();
    goals.par_sort_unstable_by_key(|row| (row.person, Reverse(row.priority), row.recipe));
    let goals: Vec<_> = goals
        .par_chunk_by(|a, b| a.person == b.person)
        .map(|rows| rows[0])
        .collect();
    let mut local: Vec<_> = people
        .par_iter()
        .filter_map(|person| {
            (person.2 > 0)
                .then_some(person.1.map(|region| (region, person.0)))
                .flatten()
        })
        .collect();
    local.par_sort_unstable();
    let mut old: Vec<_> = owner
        .pending
        .par_iter()
        .filter(|row| row.expires >= tick)
        .map(|row| (row.worker, row.requester, row.recipe))
        .collect();
    old.par_sort_unstable();
    let mut memories: Vec<_> = owner.memories.par_iter().collect();
    memories.par_sort_unstable_by_key(|row| {
        (
            row.observer,
            row.recipe,
            Reverse(row.successes.saturating_sub(row.failures)),
            row.worker,
        )
    });
    let candidates: Vec<_> = owner
        .askers
        .par_iter()
        .map(|asker| -> Result<Option<Candidate>, &'static str> {
            if tick % asker.interval != asker.person % asker.interval
                || busy.binary_search(&asker.person).is_ok()
            {
                return Ok(None);
            }
            let Some(goal) = goals
                .binary_search_by_key(&asker.person, |row| row.person)
                .ok()
                .map(|i| goals[i])
            else {
                return Ok(None);
            };
            let Some(region) = person_index(people, asker.person).and_then(|i| people[i].1) else {
                return Ok(None);
            };
            let Ok(index) = local.binary_search(&(region, asker.person)) else {
                return Ok(None);
            };
            let start = local.partition_point(|row| row.0 < region);
            let end = local.partition_point(|row| row.0 <= region);
            if end - start < 2 {
                return Ok(None);
            }
            let first = memories
                .partition_point(|row| (row.observer, row.recipe) < (asker.person, goal.recipe));
            let last = memories
                .partition_point(|row| (row.observer, row.recipe) <= (asker.person, goal.recipe));
            let remembered = memories[first..last]
                .iter()
                .find(|row| {
                    row.successes > row.failures
                        && local.binary_search(&(region, row.worker)).is_ok()
                })
                .map(|row| row.worker);
            let eligible = |worker| {
                worker != asker.person
                    && old
                        .binary_search(&(worker, asker.person, goal.recipe))
                        .is_err()
                    && !memories[first..last]
                        .iter()
                        .any(|row| row.worker == worker && row.failures >= row.successes)
            };
            let worker = remembered.filter(|&worker| eligible(worker)).or_else(|| {
                // There are at most `memory_limit` negative memories. Looking at one
                // more local person avoids scanning an arbitrarily large region.
                (1..=(owner.memory_limit + 5).min(end - start - 1))
                    .map(|step| {
                        let offset = (index - start + step) % (end - start);
                        local[start + offset].1
                    })
                    .find(|&worker| eligible(worker))
            });
            let Some(worker) = worker else {
                return Ok(None);
            };
            Ok(Some(Candidate {
                requester: asker.person,
                worker,
                region,
                recipe: goal.recipe,
                expires: tick
                    .checked_add(asker.lifetime)
                    .ok_or("work request expiry overflow")?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_par_iter()
        .flatten()
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut candidates = candidates;
    candidates.par_sort_unstable_by_key(|row| (row.worker, row.requester));
    let mut granted: Vec<_> = candidates
        .par_chunk_by(|a, b| a.worker == b.worker)
        .flat_map_iter(|group| {
            let start = old.partition_point(|row| row.0 < group[0].worker);
            let end = old.partition_point(|row| row.0 <= group[0].worker);
            group.iter().take(4usize.saturating_sub(end - start))
        })
        .collect();
    granted.par_sort_unstable_by_key(|row| (row.requester, row.worker));
    let next_id = owner
        .next_id
        .checked_add(u64::try_from(granted.len()).map_err(|_| "work request id overflow")?)
        .ok_or("work request id overflow")?;
    let requests: Vec<_> = granted
        .par_iter()
        .enumerate()
        .map(|(index, row)| Request {
            id: owner.next_id + index as u64,
            requester: row.requester,
            worker: row.worker,
            region: row.region,
            recipe: row.recipe,
            created_at: tick,
            expires: row.expires,
        })
        .collect();
    if requests.is_empty() {
        return Ok(None);
    }
    let actors = requests.par_iter().map(|row| row.requester).collect();
    Ok(Some(Draft {
        requests,
        actors,
        next_id,
    }))
}
pub fn publish(owner: &mut Requests, draft: Draft) {
    owner.pending.extend(draft.requests);
    owner.next_id = draft.next_id;
}

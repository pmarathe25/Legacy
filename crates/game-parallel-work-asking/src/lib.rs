//! Bounded personal requests derived from existing goals and local visibility.
use game_parallel_goals::Goals;
use game_parallel_production_model::{PersonSnapshot, person_index};
use game_parallel_work_requests::{OfferIndex, Request, Requests};
use rayon::prelude::*;
use std::cmp::Reverse;

pub struct Draft {
    pub requests: Vec<Request>,
    pub actors: Vec<u64>,
    pub next_id: u64,
}
/// A person's current intention to ask a specific worker under personal terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PracticeRequest {
    pub requester: u64,
    pub worker: u64,
    pub recipe: u32,
    pub expires: u64,
}
#[derive(Clone, Copy)]
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
    offers: Option<&OfferIndex>,
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    prepare_with_practices(owner, goals, people, busy, offers, &[], tick)
}

pub fn prepare_with_practices(
    owner: &Requests,
    goals: Option<&Goals>,
    people: &[PersonSnapshot],
    busy: &[u64],
    offers: Option<&OfferIndex>,
    practices: &[PracticeRequest],
    tick: u64,
) -> Result<Option<Draft>, &'static str> {
    let Some(goals) = goals.filter(|_| !owner.askers.is_empty() || !practices.is_empty()) else {
        return Ok(None);
    };
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    if practices.is_empty()
        && !owner.askers.par_iter().any(|asker| {
            tick % asker.interval == asker.person % asker.interval
                && busy.binary_search(&asker.person).is_err()
        })
    {
        return Ok(None);
    }
    let mut goals: Vec<_> = goals
        .people
        .par_iter()
        .filter(|row| row.enabled && row.progress < row.target && row.next_review < tick)
        .collect();
    let mut active_goals: Vec<_> = goals
        .par_iter()
        .map(|row| (row.person, row.recipe))
        .collect();
    active_goals.par_sort_unstable();
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
    let mut active_work: Vec<_> = owner
        .work_links
        .par_iter()
        .map(|row| (row.worker, row.requester, row.recipe))
        .collect();
    active_work.par_sort_unstable();
    let mut preferred: Vec<_> = practices
        .par_iter()
        .filter_map(|practice| {
            if practice.expires <= tick
                || practice.requester == practice.worker
                || busy.binary_search(&practice.requester).is_ok()
                || active_goals
                    .binary_search(&(practice.requester, practice.recipe))
                    .is_err()
                || old
                    .binary_search(&(practice.worker, practice.requester, practice.recipe))
                    .is_ok()
                || active_work
                    .binary_search(&(practice.worker, practice.requester, practice.recipe))
                    .is_ok()
            {
                return None;
            }
            let actor = person_index(people, practice.requester).map(|i| people[i])?;
            let worker = person_index(people, practice.worker).map(|i| people[i])?;
            let region = actor.1?;
            (actor.2 > 0 && worker.2 > 0 && worker.1 == Some(region)).then_some(Candidate {
                requester: practice.requester,
                worker: practice.worker,
                region,
                recipe: practice.recipe,
                expires: practice.expires,
            })
        })
        .collect();
    preferred.par_sort_unstable_by_key(|row| (row.requester, row.worker, row.recipe, row.expires));
    let preferred: Vec<_> = preferred
        .par_chunk_by(|a, b| a.requester == b.requester)
        .map(|group| group[0])
        .collect();
    let preferred_people: std::collections::HashSet<_> = preferred
        .par_iter()
        .map(|candidate| candidate.requester)
        .collect();
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
            if preferred_people.contains(&asker.person)
                || tick % asker.interval != asker.person % asker.interval
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
                    && (!memories[first..last]
                        .iter()
                        .any(|row| row.worker == worker && row.failures >= row.successes)
                        || (asker.retry_after_offer
                            && offers.is_some_and(|index| index.has_offer(worker, asker.person))))
            };
            let offered = asker
                .retry_after_offer
                .then(|| {
                    memories[first..last]
                        .iter()
                        .find(|row| {
                            offers.is_some_and(|index| index.has_offer(row.worker, asker.person))
                                && local.binary_search(&(region, row.worker)).is_ok()
                                && eligible(row.worker)
                        })
                        .map(|row| row.worker)
                })
                .flatten();
            let worker = offered
                .or_else(|| remembered.filter(|&worker| eligible(worker)))
                .or_else(|| {
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
    if candidates.is_empty() && preferred.is_empty() {
        return Ok(None);
    }
    let mut candidates = candidates;
    candidates.par_extend(preferred);
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

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_goals::Goal;
    use game_parallel_work_requests::WorkLink;

    #[test]
    fn personal_terms_create_only_one_local_request_for_an_active_goal() {
        let mut owner = Requests {
            next_id: 1,
            inherit_births: false,
            memory_limit: 0,
            work_links: vec![],
            memories: vec![],
            preferences: vec![],
            askers: vec![],
            pending: vec![],
            events: vec![],
        };
        let goals = Goals {
            retry_interval: 1,
            plan_inputs: false,
            events: vec![],
            people: vec![Goal {
                person: 1,
                recipe: 7,
                priority: 1,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
                recognize_help: false,
            }],
        };
        let people = [
            (1, Some(4), 10, 0),
            (2, Some(4), 10, 0),
            (3, Some(5), 10, 0),
        ];
        let terms = PracticeRequest {
            requester: 1,
            worker: 2,
            recipe: 7,
            expires: 10,
        };
        let draft =
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms, terms], 1)
                .unwrap()
                .unwrap();
        assert_eq!(draft.requests.len(), 1);
        assert_eq!(
            (
                draft.requests[0].requester,
                draft.requests[0].worker,
                draft.requests[0].recipe
            ),
            (1, 2, 7)
        );
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[1], None, &[terms], 1)
                .unwrap()
                .is_none()
        );
        assert!(
            prepare_with_practices(
                &owner,
                Some(&goals),
                &people,
                &[],
                None,
                &[PracticeRequest { worker: 3, ..terms }],
                1
            )
            .unwrap()
            .is_none()
        );
        assert!(
            prepare_with_practices(
                &owner,
                Some(&goals),
                &people,
                &[],
                None,
                &[PracticeRequest { recipe: 8, ..terms }],
                1
            )
            .unwrap()
            .is_none()
        );
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms], 10)
                .unwrap()
                .is_none()
        );
        publish(&mut owner, draft);
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms], 2)
                .unwrap()
                .is_none()
        );
        owner.pending.clear();
        owner.work_links.push(WorkLink {
            request: 1,
            requester: 1,
            worker: 2,
            job: 1,
            region: 4,
            recipe: 7,
        });
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms], 2)
                .unwrap()
                .is_none()
        );
    }
}

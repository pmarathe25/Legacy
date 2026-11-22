//! Bounded personal requests derived from existing goals and local visibility.
use game_goals::Goals;
use game_production_model::{PersonSnapshot, person_index};
use game_work_requests::{ElapsedLifetime, OfferIndex, Request, Requests};
use rayon::prelude::*;
use std::cmp::Reverse;

/// Returns the local store, regional stock, or no location while in transit.
pub type EndpointLookup<'a> = dyn Fn(u64, u64) -> Option<Option<u64>> + Sync + 'a;

#[derive(Clone, Copy)]
pub struct Timing {
    pub tick: u64,
    pub now: Option<u128>,
    pub decision_period_millis: u64,
}

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
    elapsed: Option<ElapsedLifetime>,
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
    prepare_at_endpoints(owner, goals, people, busy, offers, practices, (tick, None))
}

/// Select people at the same endpoint; transit has no endpoint.
pub fn prepare_at_endpoints(
    owner: &Requests,
    goals: Option<&Goals>,
    people: &[PersonSnapshot],
    busy: &[u64],
    offers: Option<&OfferIndex>,
    practices: &[PracticeRequest],
    timing_and_endpoint: (u64, Option<&EndpointLookup<'_>>),
) -> Result<Option<Draft>, &'static str> {
    let (tick, endpoint) = timing_and_endpoint;
    prepare_at(
        owner,
        goals,
        people,
        busy,
        offers,
        practices,
        (
            Timing {
                tick,
                now: None,
                decision_period_millis: 1,
            },
            endpoint,
        ),
    )
}

pub fn prepare_at(
    owner: &Requests,
    goals: Option<&Goals>,
    people: &[PersonSnapshot],
    busy: &[u64],
    offers: Option<&OfferIndex>,
    practices: &[PracticeRequest],
    (timing, endpoint): (Timing, Option<&EndpointLookup<'_>>),
) -> Result<Option<Draft>, &'static str> {
    let Timing {
        tick,
        now,
        decision_period_millis,
    } = timing;
    let elapsed = owner.elapsed.zip(now);
    let due = |asker: &game_work_requests::Asker| match elapsed {
        Some((clock, now)) => {
            let unit = u128::from(clock.unit_millis.get());
            let phase = u128::from(asker.person % asker.interval) * unit;
            let period = u128::from(asker.interval) * unit;
            now.checked_sub(phase)
                .is_some_and(|age| age % period < u128::from(decision_period_millis))
        }
        None => owner.elapsed.is_none() && tick % asker.interval == asker.person % asker.interval,
    };
    let Some(goals) = goals.filter(|_| !owner.askers.is_empty() || !practices.is_empty()) else {
        return Ok(None);
    };
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    if practices.is_empty()
        && !owner
            .askers
            .par_iter()
            .any(|asker| due(asker) && busy.binary_search(&asker.person).is_err())
    {
        return Ok(None);
    }
    let goals_owner = goals;
    let mut goals: Vec<_> = goals_owner
        .people
        .par_iter()
        .filter(|row| {
            row.enabled
                && row.progress < row.target
                && if goals_owner.review_clock.is_some() || elapsed.is_some() {
                    goals_owner.ready_at(row, tick, now)
                } else {
                    row.next_review < tick
                }
        })
        .collect();
    let mut active_goals: Vec<_> = goals
        .par_iter()
        .map(|row| (row.person, row.recipe))
        .collect();
    if active_goals.par_windows(2).any(|pair| pair[0] > pair[1]) {
        active_goals.par_sort_unstable();
    }
    if goals.par_windows(2).any(|pair| {
        (pair[0].person, Reverse(pair[0].priority), pair[0].recipe)
            > (pair[1].person, Reverse(pair[1].priority), pair[1].recipe)
    }) {
        goals.par_sort_unstable_by_key(|row| (row.person, Reverse(row.priority), row.recipe));
    }
    let goals: Vec<_> = goals
        .par_chunk_by(|a, b| a.person == b.person)
        .map(|rows| rows[0])
        .collect();
    let mut local: Vec<_> = people
        .par_iter()
        .filter_map(|person| {
            let region = person.1?;
            let site = endpoint.map_or(Some(None), |lookup| lookup(person.0, region))?;
            (person.2 > 0).then_some((region, site, person.0))
        })
        .collect();
    if local.par_windows(2).any(|pair| pair[0] > pair[1]) {
        local.par_sort_unstable();
    }
    // A common concentrated case has contiguous people at one endpoint.
    // Its direct index avoids three searches of the same shared population
    // for every independent asker.
    let dense_local = local.first().zip(local.last()).and_then(|(first, last)| {
        ((first.0, first.1) == (last.0, last.1)
            && last.2.checked_sub(first.2) == Some((local.len() - 1) as u64))
        .then_some(*first)
    });
    let mut old: Vec<_> = owner
        .pending
        .par_iter()
        .filter(|row| row.live_at(tick, now))
        .map(|row| (row.worker, row.requester, row.recipe))
        .collect();
    if old.par_windows(2).any(|pair| pair[0] > pair[1]) {
        old.par_sort_unstable();
    }
    let mut active_work: Vec<_> = owner
        .work_links
        .par_iter()
        .map(|row| (row.worker, row.requester, row.recipe))
        .collect();
    if active_work.par_windows(2).any(|pair| pair[0] > pair[1]) {
        active_work.par_sort_unstable();
    }
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
            let site = endpoint.map_or(Some(None), |lookup| lookup(actor.0, region))?;
            let worker_site = endpoint.map_or(Some(None), |lookup| lookup(worker.0, region))?;
            (actor.2 > 0 && worker.2 > 0 && worker.1 == Some(region) && site == worker_site)
                .then_some(Candidate {
                    requester: practice.requester,
                    worker: practice.worker,
                    region,
                    recipe: practice.recipe,
                    expires: practice.expires,
                    elapsed: None,
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
                || !due(asker)
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
            let Some(site) = endpoint.map_or(Some(None), |lookup| lookup(asker.person, region))
            else {
                return Ok(None);
            };
            let (index, start, end) = if let Some((dense_region, dense_site, first)) = dense_local
                && (region, site) == (dense_region, dense_site)
            {
                let Some(index) = asker
                    .person
                    .checked_sub(first)
                    .and_then(|id| usize::try_from(id).ok())
                    .filter(|&index| index < local.len())
                else {
                    return Ok(None);
                };
                (index, 0, local.len())
            } else {
                let Ok(index) = local.binary_search(&(region, site, asker.person)) else {
                    return Ok(None);
                };
                let start = local.partition_point(|row| (row.0, row.1) < (region, site));
                let end = local.partition_point(|row| (row.0, row.1) <= (region, site));
                (index, start, end)
            };
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
                        && local.binary_search(&(region, site, row.worker)).is_ok()
                })
                .map(|row| row.worker);
            let eligible = |worker| {
                worker != asker.person
                    && old
                        .binary_search(&(worker, asker.person, goal.recipe))
                        .is_err()
                    && active_work
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
                                && local.binary_search(&(region, site, row.worker)).is_ok()
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
                            local[start + offset].2
                        })
                        .find(|&worker| eligible(worker))
                });
            let Some(worker) = worker else {
                return Ok(None);
            };
            let (expires, lifetime) = if let Some((clock, now)) = elapsed {
                let duration = u128::from(asker.lifetime) * u128::from(clock.unit_millis.get());
                let ends_at = now
                    .checked_add(duration)
                    .ok_or("elapsed work request expiry overflow")?;
                (
                    tick,
                    Some(ElapsedLifetime {
                        posted_at: now,
                        ends_at,
                    }),
                )
            } else {
                (
                    tick.checked_add(asker.lifetime)
                        .ok_or("work request expiry overflow")?,
                    None,
                )
            };
            Ok(Some(Candidate {
                requester: asker.person,
                worker,
                region,
                recipe: goal.recipe,
                expires,
                elapsed: lifetime,
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
    if candidates
        .par_windows(2)
        .any(|pair| (pair[0].worker, pair[0].requester) > (pair[1].worker, pair[1].requester))
    {
        candidates.par_sort_unstable_by_key(|row| (row.worker, row.requester));
    }
    let mut granted: Vec<_> = candidates
        .par_chunk_by(|a, b| a.worker == b.worker)
        .flat_map_iter(|group| {
            let start = old.partition_point(|row| row.0 < group[0].worker);
            let end = old.partition_point(|row| row.0 <= group[0].worker);
            group.iter().take(4usize.saturating_sub(end - start))
        })
        .collect();
    if granted
        .par_windows(2)
        .any(|pair| (pair[0].requester, pair[0].worker) > (pair[1].requester, pair[1].worker))
    {
        granted.par_sort_unstable_by_key(|row| (row.requester, row.worker));
    }
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
            elapsed: row.elapsed,
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
    owner.register_elapsed_requests(&draft.requests);
    owner.pending.extend(draft.requests);
    owner.next_id = draft.next_id;
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_goals::Goal;
    use game_work_requests::WorkLink;

    #[test]
    fn personal_terms_create_only_one_local_request_for_an_active_goal() {
        let mut owner = Requests {
            elapsed: None,
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
            review_clock: None,
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
        let nearby = [
            (1, Some(4), 10, 0),
            (2, Some(4), 10, 0),
            (3, Some(4), 10, 0),
        ];
        let endpoint = |person, _region| match person {
            2 => Some(Some(8)),
            _ => Some(None),
        };
        assert!(
            prepare_at_endpoints(
                &owner,
                Some(&goals),
                &nearby,
                &[],
                None,
                &[terms],
                (1, Some(&endpoint))
            )
            .unwrap()
            .is_none()
        );
        owner.askers.push(game_work_requests::Asker {
            person: 1,
            interval: 1,
            lifetime: 5,
            retry_after_offer: false,
        });
        let local = prepare_at_endpoints(
            &owner,
            Some(&goals),
            &nearby,
            &[],
            None,
            &[],
            (1, Some(&endpoint)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(local.requests[0].worker, 3);
        let walking = |person, _region| if person == 1 { None } else { Some(None) };
        assert!(
            prepare_at_endpoints(
                &owner,
                Some(&goals),
                &nearby,
                &[],
                None,
                &[],
                (1, Some(&walking))
            )
            .unwrap()
            .is_none()
        );
        owner.askers.clear();
        publish(&mut owner, draft);
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms], 2)
                .unwrap()
                .is_none()
        );
        owner.askers.push(game_work_requests::Asker {
            person: 1,
            interval: 1,
            lifetime: 5,
            retry_after_offer: false,
        });
        assert!(
            prepare(&owner, Some(&goals), &people, &[], None, 2)
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
            prepare(&owner, Some(&goals), &people, &[], None, 2)
                .unwrap()
                .is_none()
        );
        assert!(
            prepare_with_practices(&owner, Some(&goals), &people, &[], None, &[terms], 2)
                .unwrap()
                .is_none()
        );
    }
}

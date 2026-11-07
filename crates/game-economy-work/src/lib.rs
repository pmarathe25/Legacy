//! Personal responses to requested work, separate from physical admission.
pub mod care;
mod site_filter;
pub use game_work_sites as sites;
pub mod tools;
pub use asking::PracticeRequest;
use game_autowork::{Autowork, Plan as WorkPlan, Policy};
use game_goals::Goals;
use game_lot_runtime::{Events, Phase, Work};
use game_opinions::Opinions;
use game_production::Production;
use game_production_model::{PersonSnapshot, person_index};
use game_work_asking as asking;
use game_work_requests::{Draft as RequestDraft, Event, OfferIndex, Outcome};
use rayon::prelude::*;

pub struct Plan {
    pub work: Option<WorkPlan>,
    /// People excluded from considering personal work, including those who declined it.
    pub ineligible: Vec<u64>,
    /// People whose time is actually reserved by this boundary's activities.
    pub occupied: Vec<u64>,
    pub site_disregards: Vec<u64>,
    request: Option<RequestDraft>,
    asks: Option<asking::Draft>,
}
/// A request that led to an actually started job, visible to its worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartedRequest {
    pub request: u64,
    pub requester: u64,
    pub worker: u64,
    pub recipe: u32,
    pub job: u64,
}
fn site_disregards(
    auto: &Autowork,
    claims: Option<&game_economy_claims::Index<'_>>,
    classes: &[(u64, Option<u64>)],
) -> Vec<u64> {
    if classes.is_empty() {
        return Vec::new();
    }
    let mut people: Vec<_> = auto
        .policies
        .par_iter()
        .filter(|p| p.disregard_claims || claims.is_some_and(|c| c.disregards(p.person)))
        .map(|p| p.person)
        .collect();
    people.par_sort_unstable();
    people.dedup();
    people
}
pub fn reserve(busy: &mut Vec<u64>, actors: impl IntoIterator<Item = u64>) {
    let actors = actors.into_iter();
    if let Some(upper) = actors.size_hint().1 {
        let maximum = busy.len().saturating_add(upper);
        if maximum < 2048 {
            busy.extend(actors);
            busy.sort_unstable();
            busy.dedup();
            return;
        }
        if rayon::current_num_threads() > 1
            && maximum < 131_072
            && (upper > 0 || busy.len() < 32_768)
        {
            busy.extend(actors);
            busy.par_sort_unstable();
            busy.dedup();
            return;
        }
    }
    let mut incoming: Vec<_> = actors.collect();
    if busy.len().saturating_add(incoming.len()) < 2048 {
        busy.extend(incoming);
        busy.sort_unstable();
        busy.dedup();
        return;
    }
    // Parallel sorting is cheaper than several merge passes for moderate sets.
    if rayon::current_num_threads() > 1
        && busy.len().saturating_add(incoming.len()) < 131_072
        && (!incoming.is_empty() || busy.len() < 32_768)
    {
        busy.extend(incoming);
        busy.par_sort_unstable();
        busy.dedup();
        return;
    }
    let normalize = |rows: &mut Vec<u64>| {
        if rows.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            rows.par_sort_unstable();
            if rows.par_windows(2).any(|pair| pair[0] == pair[1]) {
                *rows = rows
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
    };
    normalize(busy);
    normalize(&mut incoming);
    if incoming.is_empty() {
        return;
    }
    if busy.is_empty() {
        *busy = incoming;
        return;
    }
    if busy.last() < incoming.first() {
        busy.extend(incoming);
        return;
    }
    const CHUNK: usize = 4096;
    let old = std::mem::take(busy);
    *busy = old
        .par_chunks(CHUNK)
        .enumerate()
        .flat_map_iter(|(index, chunk)| {
            let start = if index == 0 {
                0
            } else {
                incoming.partition_point(|id| *id < chunk[0])
            };
            let next = (index + 1) * CHUNK;
            let end = if next >= old.len() {
                incoming.len()
            } else {
                incoming.partition_point(|id| *id < old[next])
            };
            let mut merged = Vec::with_capacity(chunk.len() + end - start);
            let mut at = start;
            for &person in chunk {
                while at < end && incoming[at] < person {
                    merged.push(incoming[at]);
                    at += 1;
                }
                if at < end && incoming[at] == person {
                    at += 1;
                }
                merged.push(person);
            }
            merged.extend_from_slice(&incoming[at..end]);
            merged
        })
        .collect();
}

/// Remove an ordered set from ordered IDs without a lookup for every row.
pub fn exclude_sorted(rows: &[u64], excluded: &[u64]) -> Vec<u64> {
    if excluded.is_empty() {
        return rows.par_iter().copied().collect();
    }
    rows.par_chunks(4096)
        .flat_map_iter(|chunk| {
            let mut at = excluded.partition_point(|person| *person < chunk[0]);
            chunk.iter().copied().filter(move |person| {
                while at < excluded.len() && excluded[at] < *person {
                    at += 1;
                }
                excluded.get(at) != Some(person)
            })
        })
        .collect()
}

pub fn initial_busy(
    people: &[game_economy_person::Person],
    meals: &[(u64, u32)],
    work: &[Work],
) -> Vec<u64> {
    let mut busy: Vec<_> = people
        .par_iter()
        .filter(|person| person.controlled)
        .map(|person| person.id)
        .chain(meals.par_iter().map(|row| row.0))
        .chain(
            work.par_iter()
                .filter(|row| row.phase == Phase::Working)
                .map(|row| row.job.worker),
        )
        .collect();
    busy.par_sort_unstable();
    busy.dedup();
    busy
}
pub fn underage(family: &game_families::Families) -> Vec<u64> {
    let mut ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.age < family.rules.min_work_age)
        .map(|m| m.id)
        .collect();
    ids.par_sort_unstable();
    ids
}
pub fn reserve_underage(family: Option<&game_families::Families>, busy: &mut Vec<u64>) {
    if let Some(family) = family {
        reserve(busy, underage(family));
    }
}
pub fn validate_age(underage: &[u64], work: &[Work]) -> Result<(), String> {
    if work
        .par_iter()
        .any(|row| row.phase == Phase::Working && underage.binary_search(&row.job.worker).is_ok())
    {
        return Err("an underage person is working".into());
    }
    Ok(())
}
/// A remembered request may influence only the actual active work it initiated.
pub fn validate_links(auto: &Autowork, work: &[Work]) -> Result<(), String> {
    if auto.requests.as_ref().is_some_and(|requests| {
        requests.work_links.par_iter().any(|link| {
            work.binary_search_by_key(&link.job, |row| row.job.id)
                .ok()
                .is_none_or(|index| {
                    let row = &work[index];
                    row.phase != Phase::Working
                        || row.job.worker != link.worker
                        || row.job.region != link.region
                        || row.job.recipe != link.recipe
                })
        })
    }) {
        return Err("requested work does not match active physical work".into());
    }
    Ok(())
}
pub struct Context<'a> {
    pub auto: &'a Autowork,
    pub production: &'a Production,
    pub people: &'a [PersonSnapshot],
    pub tool_requirements: &'a [tools::Requirement],
    pub view: &'a game_lots::View,
    pub opinions: Option<&'a Opinions>,
    pub offers: Option<&'a OfferIndex>,
    pub practice_requests: &'a [PracticeRequest],
    pub claims: Option<&'a game_economy_claims::Index<'a>>,
    pub goals: Option<&'a Goals>,
    pub preplanned_goals: Option<&'a Autowork>,
    pub sites: Option<&'a sites::Facilities>,
    pub site_classes: &'a [(u64, Option<u64>)],
    pub seed: u64,
    pub busy: &'a [u64],
    pub unwilling: &'a [u64],
    pub max_job: Option<u64>,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self) -> Result<Plan, String> {
        if self.auto.requests.is_none() && self.goals.is_none() {
            #[cfg(feature = "profile")]
            let _phase = game_diagnostics::span("work.context.simple");
            let mut busy = self.busy.to_vec();
            busy.extend_from_slice(self.unwilling);
            busy.par_sort_unstable();
            busy.dedup();
            let equipped = (!self.tool_requirements.is_empty()).then(|| Autowork {
                next_id: self.auto.next_id,
                policies: self
                    .auto
                    .policies
                    .par_iter()
                    .filter(|p| {
                        tools::available(self.tool_requirements, self.view, p.person, p.recipe)
                    })
                    .cloned()
                    .collect(),
                requests: None,
            });
            let base = equipped.as_ref().unwrap_or(self.auto);
            let filtered =
                game_economy_claims::filter_work(base, self.people, self.production, self.claims);
            let base = filtered.as_ref().unwrap_or(base);
            let work = if let Some(sites) = self.sites.filter(|s| !s.requirements.is_empty()) {
                site_filter::plan(
                    sites,
                    base,
                    self.people,
                    &busy,
                    site_filter::Selection {
                        seed: self.seed,
                        tick: self.tick,
                        claims: self.claims,
                        classes: self.site_classes,
                        production: self.production,
                        view: self.view,
                        max_job: self.max_job,
                    },
                )?
            } else {
                base.plan_validated(self.people, self.production, &busy, self.max_job)?
            };
            let mut occupied = self.busy.to_vec();
            if let Some(plan) = &work {
                reserve(&mut occupied, plan.actors.iter().copied());
            }
            return Ok(Plan {
                work,
                ineligible: busy,
                occupied,
                site_disregards: site_disregards(base, self.claims, self.site_classes),
                request: None,
                asks: None,
            });
        }
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("work.context.requests");
        let request = self.auto.requests.as_ref().map(|owner| {
            owner.prepare_with_offers(
                self.people,
                self.opinions,
                self.busy,
                self.offers,
                self.tick,
            )
        });
        let mut ask_busy = self.busy.to_vec();
        if let Some(draft) = &request {
            ask_busy.par_extend(draft.decisions.par_iter().map(|row| row.worker));
        }
        let asks = self
            .auto
            .requests
            .as_ref()
            .map(|owner| {
                asking::prepare_with_practices(
                    owner,
                    self.goals,
                    self.people,
                    &ask_busy,
                    self.offers,
                    self.practice_requests,
                    self.tick,
                )
            })
            .transpose()
            .map_err(str::to_owned)?
            .flatten();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("work.context.goals_and_busy");
        }
        let mut goal_policies = self
            .preplanned_goals
            .is_none()
            .then(|| {
                self.goals
                    .map(|goals| goals.policies(self.auto, self.tick.saturating_sub(1)))
            })
            .flatten();
        if self.goals.is_some_and(|goals| goals.plan_inputs)
            && let Some(policies) = &mut goal_policies
        {
            let review =
                self.tick.saturating_sub(1) / self.goals.expect("goal planning").retry_interval;
            game_goals::planning::plan_at(
                policies,
                self.auto,
                self.production,
                self.people,
                review,
            );
        }
        let base = self
            .preplanned_goals
            .or(goal_policies.as_ref())
            .unwrap_or(self.auto);
        let mut selected: Vec<_> = request.as_ref().map_or_else(Vec::new, |draft| {
            draft.decisions.par_iter().filter(|row| row.help).collect()
        });
        selected.par_sort_unstable_by_key(|row| row.worker);
        let mut busy = self.busy.to_vec();
        if let Some(asks) = &asks {
            busy.extend_from_slice(&asks.actors);
        }
        busy.par_extend(
            self.unwilling
                .par_iter()
                .filter(|id| {
                    selected
                        .binary_search_by_key(*id, |row| row.worker)
                        .is_err()
                })
                .copied(),
        );
        if let Some(draft) = &request {
            busy.par_extend(
                draft
                    .decisions
                    .par_iter()
                    .filter(|row| !row.help)
                    .map(|row| row.worker),
            );
        }
        busy.par_sort_unstable();
        busy.dedup();
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("work.context.policies_and_claims");
        }
        let mut policies: Vec<_> = base
            .policies
            .par_iter()
            .filter(|row| {
                selected
                    .binary_search_by_key(&row.person, |item| item.worker)
                    .is_err()
            })
            .cloned()
            .collect();
        policies.par_extend(selected.par_iter().map(|row| Policy {
            person: row.worker,
            recipe: row.recipe,
            enabled: true,
            disregard_claims: row.disregard_claims,
        }));
        if !self.tool_requirements.is_empty() {
            policies = policies
                .into_par_iter()
                .filter(|p| tools::available(self.tool_requirements, self.view, p.person, p.recipe))
                .collect();
        }
        let staged = Autowork {
            next_id: self.auto.next_id,
            policies,
            requests: None,
        };
        let filtered =
            game_economy_claims::filter_work(&staged, self.people, self.production, self.claims);
        let base = filtered.as_ref().unwrap_or(&staged);
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("work.context.admission");
        }
        let work = if let Some(sites) = self.sites.filter(|s| !s.requirements.is_empty()) {
            site_filter::plan(
                sites,
                base,
                self.people,
                &busy,
                site_filter::Selection {
                    seed: self.seed,
                    tick: self.tick,
                    claims: self.claims,
                    classes: self.site_classes,
                    production: self.production,
                    view: self.view,
                    max_job: self.max_job,
                },
            )?
        } else {
            base.plan_validated(self.people, self.production, &busy, self.max_job)?
        };
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("work.context.publish");
        }
        if self.goals.is_some() {
            reserve(&mut busy, selected.iter().map(|row| row.worker));
        }
        let mut occupied = self.busy.to_vec();
        if let Some(asks) = &asks {
            reserve(&mut occupied, asks.actors.iter().copied());
        }
        if let Some(request) = &request {
            reserve(
                &mut occupied,
                request.decisions.iter().map(|row| row.worker),
            );
        }
        if let Some(plan) = &work {
            reserve(&mut occupied, plan.actors.iter().copied());
        }
        #[cfg(feature = "profile")]
        drop(phase);
        Ok(Plan {
            work,
            ineligible: busy,
            occupied,
            site_disregards: site_disregards(base, self.claims, self.site_classes),
            request,
            asks,
        })
    }
}
/// Ongoing requested work can still be motivated by current feelings; nothing binds the worker.
pub fn pressured_workers(
    auto: &Autowork,
    opinions: Option<&Opinions>,
    offers: Option<&OfferIndex>,
) -> Vec<u64> {
    let Some(owner) = auto.requests.as_ref() else {
        return Vec::new();
    };
    let mut prefs: Vec<_> = owner.preferences.par_iter().collect();
    prefs.par_sort_unstable_by_key(|row| row.person);
    let index = opinions.map(Opinions::index);
    let mut workers: Vec<_> = owner
        .work_links
        .par_iter()
        .filter_map(|link| {
            let pref = prefs
                .binary_search_by_key(&link.worker, |row| row.person)
                .ok()
                .map(|i| prefs[i])?;
            let (fear, obligation) = index
                .as_ref()
                .map_or((0, 0), |index| index.feelings(link.worker, link.requester));
            let affinity = if pref.affinity_weight == 0 {
                0
            } else {
                index
                    .as_ref()
                    .map_or(0, |index| index.affinity(link.worker, link.requester))
            };
            let trust = if pref.trust_weight == 0 {
                0
            } else {
                index
                    .as_ref()
                    .map_or(0, |index| index.trust(link.worker, link.requester))
            };
            let offered = pref
                .offer_value
                .as_ref()
                .and_then(|value| {
                    offers.map(|index| {
                        index.quantity_for(link.worker, link.requester, link.recipe, value.good)
                    })
                })
                .unwrap_or(0);
            pref.motivated(fear, obligation, affinity, trust, offered)
                .then_some(link.worker)
        })
        .collect();
    workers.par_sort_unstable();
    workers.dedup();
    workers
}
fn visible(people: &[PersonSnapshot], requester: u64, worker: u64, region: u64) -> bool {
    let a = person_index(people, requester).map(|index| people[index]);
    let b = person_index(people, worker).map(|index| people[index]);
    matches!((a, b), (Some((_, Some(ar), ah, _)), Some((_, Some(br), bh, _))) if ar == region && br == ar && ah > 0 && bh > 0)
}
impl Plan {
    /// Include a controlled person's independent decision in the same outcome
    /// and witness accounting as an autonomous request response.
    pub fn record_manual_response(
        &mut self,
        decision: game_work_requests::Decision,
        start: Option<game_production_model::Start>,
        next_id: Option<u64>,
    ) {
        self.request
            .as_mut()
            .expect("validated work requests")
            .decisions
            .push(decision);
        if let Some(start) = start {
            let work = self.work.get_or_insert_with(|| WorkPlan {
                requests: Vec::new(),
                next_id: next_id.expect("manual start has a next id"),
                actors: Vec::new(),
            });
            work.requests.push(start);
            work.actors.push(start.worker);
            work.next_id = work
                .next_id
                .max(next_id.expect("manual start has a next id"));
        }
    }
    /// Requests actually admitted to a receiver's bounded inbox this boundary.
    pub fn asks(&self) -> &[game_work_requests::Request] {
        self.asks.as_ref().map_or(&[], |draft| &draft.requests)
    }
    pub fn started_requests(&self, auto: &Autowork, physical: &Events) -> Vec<StartedRequest> {
        let (Some(owner), Some(decisions), Some(work)) =
            (&auto.requests, &self.request, &self.work)
        else {
            return Vec::new();
        };
        if physical.started.is_empty() {
            return Vec::new();
        }
        let mut pending: Vec<_> = owner.pending.par_iter().collect();
        pending.par_sort_unstable_by_key(|request| request.id);
        let mut started: Vec<_> = work
            .requests
            .par_iter()
            .filter(|job| physical.started.binary_search(&job.id).is_ok())
            .collect();
        started.par_sort_unstable_by_key(|job| job.worker);
        decisions
            .decisions
            .par_iter()
            .filter(|decision| decision.help)
            .filter_map(|decision| {
                let request = pending
                    .binary_search_by_key(&decision.request, |request| request.id)
                    .ok()
                    .map(|i| pending[i])?;
                let job = started
                    .binary_search_by_key(&decision.worker, |job| job.worker)
                    .ok()
                    .map(|i| started[i])?;
                (request.worker == job.worker
                    && request.region == job.region
                    && request.recipe == job.recipe)
                    .then_some(StartedRequest {
                        request: request.id,
                        requester: request.requester,
                        worker: request.worker,
                        recipe: request.recipe,
                        job: job.id,
                    })
            })
            .collect()
    }
    /// Avoid scanning personal preferences on boundaries with no relevant outcomes.
    pub fn may_have_work_experiences(&self, physical: &Events) -> bool {
        !physical.finished.is_empty()
            || self
                .request
                .as_ref()
                .is_some_and(|request| !request.decisions.is_empty())
    }
    /// Actual outcomes observed by the requester, independent of memory capacity.
    pub fn work_experiences(
        &self,
        auto: &Autowork,
        physical: &Events,
        people: &[PersonSnapshot],
    ) -> Vec<(u64, u64, bool)> {
        let Some(owner) = &auto.requests else {
            return Vec::new();
        };
        let mut observations: Vec<_> = owner
            .work_links
            .par_iter()
            .filter(|link| {
                physical.finished.binary_search(&link.job).is_ok()
                    && visible(people, link.requester, link.worker, link.region)
            })
            .map(|link| (link.requester, link.worker, true))
            .collect();
        if let Some(request) = self
            .request
            .as_ref()
            .filter(|request| request.decisions.par_iter().any(|decision| !decision.help))
        {
            let mut pending: Vec<_> = owner.pending.par_iter().collect();
            pending.par_sort_unstable_by_key(|row| row.id);
            observations.par_extend(
                request
                    .decisions
                    .par_iter()
                    .filter(|decision| !decision.help)
                    .filter_map(|decision| {
                        let row = pending
                            .binary_search_by_key(&decision.request, |row| row.id)
                            .ok()
                            .map(|i| pending[i])?;
                        visible(people, row.requester, row.worker, row.region).then_some((
                            row.requester,
                            row.worker,
                            false,
                        ))
                    }),
            );
        }
        observations
    }
    /// Publish remembered outcomes only after the physical generation succeeded.
    pub fn publish(self, auto: &mut Autowork, physical: &Events, people: &[PersonSnapshot]) {
        if let Some(work) = &self.work {
            auto.next_id = work.next_id;
        }
        let Some(owner) = &mut auto.requests else {
            return;
        };
        let mut existing: Vec<_> = owner
            .work_links
            .par_iter()
            .filter_map(|link| {
                if physical.finished.binary_search(&link.job).is_ok() {
                    Some((link.job, true))
                } else if physical.interrupted.binary_search(&link.job).is_ok()
                    || physical.abandoned.binary_search(&link.job).is_ok()
                {
                    Some((link.job, false))
                } else {
                    None
                }
            })
            .collect();
        existing.par_sort_unstable_by_key(|row| row.0);
        owner.resolve(&existing, physical.tick, |link| {
            visible(people, link.requester, link.worker, link.region)
        });
        let Some(request) = self.request else {
            owner.events.clear();
            if let Some(asks) = self.asks {
                asking::publish(owner, asks);
            }
            return;
        };
        let mut started: Vec<_> = self.work.as_ref().map_or_else(Vec::new, |work| {
            work.requests
                .par_iter()
                .filter(|row| physical.started.binary_search(&row.id).is_ok())
                .map(|row| (row.worker, row.id, row.region, row.recipe))
                .collect()
        });
        started.par_sort_unstable_by_key(|row| row.0);
        let mut events: Vec<_> = request
            .decisions
            .par_iter()
            .filter_map(|row| {
                let outcome = if !row.help {
                    Outcome::Refused
                } else {
                    let found = started
                        .binary_search_by_key(&row.worker, |row| row.0)
                        .ok()
                        .map(|i| started[i])?;
                    if found.2 != row.region || found.3 != row.recipe {
                        return None;
                    }
                    Outcome::Started { job: found.1 }
                };
                Some(Event {
                    tick: physical.tick,
                    request: row.request,
                    worker: row.worker,
                    outcome,
                })
            })
            .collect();
        events.par_extend(request.expired);
        events.par_sort_unstable_by_key(|row| row.request);
        let mut witnessed: Vec<_> = owner
            .pending
            .par_iter()
            .filter(|row| visible(people, row.requester, row.worker, row.region))
            .map(|row| row.id)
            .collect();
        witnessed.par_sort_unstable();
        owner.record_starts(&events);
        let ended_on_start: Vec<_> = started
            .par_iter()
            .filter_map(|row| {
                (physical.interrupted.binary_search(&row.1).is_ok()
                    || physical.abandoned.binary_search(&row.1).is_ok())
                .then_some((row.1, false))
            })
            .collect();
        owner.resolve(&ended_on_start, physical.tick, |link| {
            visible(people, link.requester, link.worker, link.region)
        });
        owner.observe_refusals(&events, &witnessed, physical.tick);
        let ids: Vec<_> = events.par_iter().map(|row| row.request).collect();
        owner.pending = std::mem::take(&mut owner.pending)
            .into_par_iter()
            .filter(|row| ids.binary_search(&row.id).is_err())
            .collect();
        owner.events = events;
        if let Some(asks) = self.asks {
            asking::publish(owner, asks);
        }
    }
}

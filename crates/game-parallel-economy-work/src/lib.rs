//! Personal responses to requested work, separate from physical admission.
pub mod care;
pub mod tools;
use game_parallel_autowork::{Autowork, Plan as WorkPlan, Policy};
use game_parallel_goals::Goals;
use game_parallel_lot_runtime::{Events, Phase, Work};
use game_parallel_opinions::Opinions;
use game_parallel_production::Production;
use game_parallel_production_model::{PersonSnapshot, person_index};
use game_parallel_work_asking as asking;
use game_parallel_work_requests::{Draft as RequestDraft, Event, Outcome};
use rayon::prelude::*;

pub struct Plan {
    pub work: Option<WorkPlan>,
    pub occupied: Vec<u64>,
    request: Option<RequestDraft>,
    asks: Option<asking::Draft>,
}
pub fn reserve(busy: &mut Vec<u64>, actors: impl IntoIterator<Item = u64>) {
    busy.extend(actors);
    busy.par_sort_unstable();
    busy.dedup();
}
pub fn initial_busy(
    people: &[game_parallel_economy_person::Person],
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
pub fn underage(family: &game_parallel_families::Families) -> Vec<u64> {
    let mut ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.age < family.rules.min_work_age)
        .map(|m| m.id)
        .collect();
    ids.par_sort_unstable();
    ids
}
pub fn reserve_underage(family: Option<&game_parallel_families::Families>, busy: &mut Vec<u64>) {
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
    pub view: &'a game_parallel_lots::View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub goals: Option<&'a Goals>,
    pub preplanned_goals: Option<&'a Autowork>,
    pub busy: &'a [u64],
    pub unwilling: &'a [u64],
    pub max_job: Option<u64>,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self) -> Result<Plan, String> {
        if self.auto.requests.is_none() && self.goals.is_none() {
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
            let filtered = game_parallel_economy_claims::filter_work(
                base,
                self.people,
                self.production,
                self.claims,
            );
            return Ok(Plan {
                work: filtered.as_ref().unwrap_or(base).plan_validated(
                    self.people,
                    self.production,
                    &busy,
                    self.max_job,
                )?,
                occupied: busy,
                request: None,
                asks: None,
            });
        }
        let request = self
            .auto
            .requests
            .as_ref()
            .map(|owner| owner.prepare(self.people, self.opinions, self.busy, self.tick));
        let mut ask_busy = self.busy.to_vec();
        if let Some(draft) = &request {
            ask_busy.par_extend(draft.decisions.par_iter().map(|row| row.worker));
        }
        let asks = self
            .auto
            .requests
            .as_ref()
            .map(|owner| asking::prepare(owner, self.goals, self.people, &ask_busy, self.tick))
            .transpose()
            .map_err(str::to_owned)?
            .flatten();
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
            game_parallel_goals::planning::plan(policies, self.auto, self.production, self.people);
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
        let filtered = game_parallel_economy_claims::filter_work(
            &staged,
            self.people,
            self.production,
            self.claims,
        );
        let work = filtered.as_ref().unwrap_or(&staged).plan_validated(
            self.people,
            self.production,
            &busy,
            self.max_job,
        )?;
        if self.goals.is_some() {
            reserve(&mut busy, selected.iter().map(|row| row.worker));
        }
        Ok(Plan {
            work,
            occupied: busy,
            request,
            asks,
        })
    }
}
/// Ongoing requested work can still be motivated by current feelings; nothing binds the worker.
pub fn pressured_workers(auto: &Autowork, opinions: Option<&Opinions>) -> Vec<u64> {
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
            let social = index.as_ref().map_or(0u128, |index| {
                u128::from(index.fear(link.worker, link.requester))
                    + u128::from(index.obligation(link.worker, link.requester))
            });
            (u128::from(pref.help_value) + social >= u128::from(pref.effort)).then_some(link.worker)
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

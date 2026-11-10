//! Personal responses to addressed work requests in the limited core.
use super::*;
use game_autowork::requests::{Event, Outcome, Requests};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreRequestResults {
    /// People who considered an addressed request, including refusals.
    pub actors: Vec<u64>,
    pub events: Vec<Event>,
}

fn together(people: &[Person], requester: u64, worker: u64, region: u64) -> bool {
    [requester, worker].into_iter().all(|id| {
        people
            .binary_search_by_key(&id, |person| person.id)
            .ok()
            .is_some_and(|index| {
                let person = &people[index];
                person.health > 0 && person.region == Some(region)
            })
    })
}

impl CoreHost {
    /// Resolve local addressed requests using each worker's directed feelings.
    /// No response binds future work; a failed reservation stays pending.
    pub fn respond_to_work_requests(
        &mut self,
        blocked: &[u64],
        rate: u64,
    ) -> Result<CoreRequestResults, String> {
        if rate == 0 || blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid request response batch".into());
        }
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before responding".into());
        }
        let Some(auto) = &self.autowork else {
            return Ok(CoreRequestResults::default());
        };
        let Some(requests) = &auto.requests else {
            return Ok(CoreRequestResults::default());
        };
        let mut busy = blocked.to_vec();
        busy.par_extend(
            self.core
                .people
                .par_iter()
                .filter(|person| {
                    person.controlled
                        || person.health == 0
                        || person.hunger > self.core.rules.max_work_hunger
                })
                .map(|person| person.id),
        );
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let people: Vec<_> = self
            .core
            .people
            .par_iter()
            .map(|person| (person.id, person.region, person.health, person.hunger))
            .collect();
        let draft = requests.prepare_filtered(
            &people,
            self.opinions.as_ref(),
            &busy,
            None,
            tick,
            |request| {
                request.created_at <= tick
                    && together(
                        &self.core.people,
                        request.requester,
                        request.worker,
                        request.region,
                    )
            },
        );
        let actors: Vec<_> = draft
            .decisions
            .par_iter()
            .map(|decision| decision.worker)
            .collect();
        let helping: Vec<_> = draft
            .decisions
            .par_iter()
            .filter(|decision| decision.help)
            .collect();
        let (first, next) = if helping.is_empty() {
            (0, auto.next_id)
        } else {
            let first =
                auto.next_id
                    .max(self.core.physical.work().last().map_or(Ok(1), |row| {
                        row.job.id.checked_add(1).ok_or("job id overflow")
                    })?);
            let next = first
                .checked_add(u64::try_from(helping.len()).map_err(|_| "job count overflow")?)
                .ok_or("job id overflow")?;
            (first, next)
        };
        let starts: Vec<_> = helping
            .par_iter()
            .enumerate()
            .map(|(index, decision)| {
                (
                    game_production_model::Start {
                        id: first + index as u64,
                        worker: decision.worker,
                        region: decision.region,
                        recipe: decision.recipe,
                        units: 1,
                    },
                    rate,
                )
            })
            .collect();
        let outcomes = if starts.is_empty() {
            Vec::new()
        } else {
            self.core.start_work(tick, offset, &starts)?
        };
        let mut events: Vec<_> = draft
            .decisions
            .par_iter()
            .filter_map(|decision| {
                let outcome = if !decision.help {
                    Outcome::Refused
                } else {
                    let index = helping
                        .binary_search_by_key(&decision.worker, |row| row.worker)
                        .ok()?;
                    let job = first + index as u64;
                    let accepted = outcomes
                        .binary_search_by_key(&job, |row| row.0)
                        .ok()
                        .is_some_and(|i| outcomes[i].1);
                    if !accepted {
                        return None;
                    }
                    Outcome::Started { job }
                };
                Some(Event {
                    tick,
                    request: decision.request,
                    worker: decision.worker,
                    outcome,
                })
            })
            .collect();
        events.par_extend(draft.expired);
        events.par_sort_unstable_by_key(|event| event.request);
        let mut visible: Vec<_> = requests
            .pending
            .par_iter()
            .filter(|request| {
                together(
                    &self.core.people,
                    request.requester,
                    request.worker,
                    request.region,
                )
            })
            .map(|request| request.id)
            .collect();
        visible.par_sort_unstable();
        let auto = self.autowork.as_mut().expect("checked work owner");
        if !starts.is_empty() {
            auto.next_id = next;
        }
        let requests = auto.requests.as_mut().expect("checked request owner");
        requests.record_starts(&events);
        requests.observe_refusals(&events, &visible, tick);
        let ended: Vec<_> = events.iter().map(|event| event.request).collect();
        requests.pending = std::mem::take(&mut requests.pending)
            .into_par_iter()
            .filter(|request| ended.binary_search(&request.id).is_err())
            .collect();
        requests.events = events.clone();
        Ok(CoreRequestResults { actors, events })
    }

    /// Prepare personal learning from committed job outcomes without mutating
    /// the saved request owner before the physical generation commits.
    pub(super) fn stage_request_resolution(
        &self,
        completed: &[(u64, bool)],
        tick: u64,
        people: &[Person],
        clear_events: bool,
    ) -> Option<Requests> {
        let owner = self.autowork.as_ref()?.requests.as_ref()?;
        let resolving = !completed.is_empty() && !owner.work_links.is_empty();
        if !resolving && (!clear_events || owner.events.is_empty()) {
            return None;
        }
        let mut next = owner.clone();
        if clear_events {
            next.events.clear();
        }
        next.resolve(completed, tick, |link| {
            together(people, link.requester, link.worker, link.region)
        });
        Some(next)
    }
    pub(super) fn publish_request_resolution(&mut self, next: Option<Requests>) {
        if let Some(next) = next {
            self.autowork.as_mut().expect("staged work owner").requests = Some(next);
        }
    }
}

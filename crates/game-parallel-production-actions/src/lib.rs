//! Checked ordinary-world production commands.
use game_parallel_people::Region;
use game_parallel_production::{Production, Start};
use game_parallel_property::Property;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
pub struct Context<'a> {
    pub state: &'a mut Option<Production>,
    pub autowork: &'a mut Option<game_parallel_autowork::Autowork>,
    pub property: Option<&'a Property>,
    pub regions: &'a [Region],
    pub travelers: &'a [Traveler],
    pub tick: u64,
    pub suspended: Option<u64>,
}
impl Context<'_> {
    /// Check an addressed response without changing production or the request.
    pub fn work_response(
        &self,
        id: u64,
        worker: u64,
    ) -> Result<game_parallel_autowork::requests::Request, String> {
        if self.suspended == Some(worker) {
            return Err("player control is suspended".into());
        }
        let production = self.state.as_ref().ok_or("production is disabled")?;
        if production.busy_people().contains(&worker) {
            return Err("worker is busy".into());
        }
        let request = self
            .autowork
            .as_ref()
            .and_then(|auto| auto.requests.as_ref())
            .and_then(|requests| requests.pending.iter().find(|row| row.id == id))
            .ok_or("work request not found")?;
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        let local = game_parallel_production_model::person_index(&people, worker)
            .is_some_and(|i| people[i].1 == Some(request.region) && people[i].2 > 0);
        if request.worker != worker || request.expires < self.tick || !local {
            return Err("invalid work response".into());
        }
        Ok(request.clone())
    }
    /// Address a person without deciding their response for them.
    pub fn request_work(
        &mut self,
        proposal: game_parallel_autowork::requests::Proposal,
    ) -> Result<u64, String> {
        Ok(self.request_work_batch(&[proposal])?[0])
    }
    /// Send a batch atomically, with one validation pass over the world.
    pub fn request_work_batch(
        &mut self,
        proposals: &[game_parallel_autowork::requests::Proposal],
    ) -> Result<Vec<u64>, String> {
        if proposals.is_empty() {
            return Ok(Vec::new());
        }
        if proposals
            .par_iter()
            .any(|p| self.suspended == Some(p.requester))
        {
            return Err("player control is suspended".into());
        }
        let production = self.state.as_ref().ok_or("production is disabled")?;
        let mut busy = production.busy_people();
        busy.par_sort_unstable();
        if proposals
            .par_iter()
            .any(|p| busy.binary_search(&p.requester).is_ok())
        {
            return Err("requester is busy".into());
        }
        let people = production.validate_people(self.regions, self.travelers, self.tick)?;
        self.autowork
            .as_mut()
            .ok_or("autonomous work is disabled")?
            .requests
            .as_mut()
            .ok_or("work requests are disabled")?
            .open_batch(proposals, &people, production, self.tick)
    }
    pub fn start(&mut self, requests: &[Start]) -> Result<(), String> {
        if self
            .suspended
            .is_some_and(|person| requests.iter().any(|request| request.worker == person))
        {
            return Err("player control is suspended".into());
        }
        let production = self.state.as_ref().ok_or("production is disabled")?;
        if let Some(property) = self.property {
            property.check_jobs(&production.recipes, requests)?;
        }
        let draft = production.prepare_start(self.regions, self.travelers, self.tick, requests)?;
        let production = self.state.as_mut().unwrap();
        draft.commit(&mut production.jobs, &mut production.inventories);
        Ok(())
    }
    pub fn cancel(&mut self, id: u64) -> Result<(), String> {
        if self.suspended.is_some_and(|person| {
            self.state.as_ref().is_some_and(|state| {
                state
                    .jobs
                    .iter()
                    .any(|job| job.id == id && job.worker == person)
            })
        }) {
            return Err("player control is suspended".into());
        }
        let next = self
            .state
            .as_ref()
            .ok_or("production is disabled")?
            .cancel(self.regions, self.travelers, self.tick, id)?;
        *self.state = Some(next);
        Ok(())
    }
    pub fn reassign(&mut self, id: u64, worker: u64) -> Result<(), String> {
        if self.suspended.is_some_and(|person| {
            person == worker
                || self.state.as_ref().is_some_and(|state| {
                    state
                        .jobs
                        .iter()
                        .any(|job| job.id == id && job.worker == person)
                })
        }) {
            return Err("player control is suspended".into());
        }
        if let (Some(property), Some(job)) = (
            self.property,
            self.state
                .as_ref()
                .and_then(|state| state.jobs.iter().find(|job| job.id == id)),
        ) {
            property.check_jobs(
                &self.state.as_ref().unwrap().recipes,
                &[Start {
                    id,
                    worker,
                    region: job.region,
                    recipe: job.recipe,
                    units: 1,
                }],
            )?;
        }
        let next = self
            .state
            .as_ref()
            .ok_or("production is disabled")?
            .reassign(self.regions, self.travelers, self.tick, id, worker)?;
        *self.state = Some(next);
        Ok(())
    }
}

#[macro_export]
macro_rules! world_production_methods {
    () => {
        fn production_actions(&mut self) -> game_parallel_production_actions::Context<'_> {
            let suspended = self.suspended_person();
            game_parallel_production_actions::Context {
                state: &mut self.production,
                autowork: &mut self.autowork,
                property: self.property.as_ref(),
                regions: &self.regions,
                travelers: &self.travelers,
                tick: self.tick,
                suspended,
            }
        }
        pub fn request_work(
            &mut self,
            proposal: game_parallel_autowork::requests::Proposal,
        ) -> Result<u64, String> {
            self.production_actions().request_work(proposal)
        }
        pub fn request_work_batch(
            &mut self,
            proposals: &[game_parallel_autowork::requests::Proposal],
        ) -> Result<Vec<u64>, String> {
            self.production_actions().request_work_batch(proposals)
        }
        /// Personally decline or start the requested physical work.
        pub fn respond_work(&mut self, id: u64, worker: u64, start: bool) -> Result<(), String> {
            let request = self.production_actions().work_response(id, worker)?;
            let outcome = if start {
                let auto = self
                    .autowork
                    .as_ref()
                    .ok_or("autonomous work is disabled")?;
                let last = self
                    .production
                    .as_ref()
                    .ok_or("production is disabled")?
                    .jobs
                    .iter()
                    .map(|job| job.id)
                    .max()
                    .unwrap_or(0);
                let job = auto
                    .next_id
                    .max(last.checked_add(1).ok_or("work id overflow")?);
                let next = job.checked_add(1).ok_or("work id overflow")?;
                self.start_work(&[game_parallel_production::Start {
                    id: job,
                    worker,
                    region: request.region,
                    recipe: request.recipe,
                    units: 1,
                }])?;
                self.autowork.as_mut().unwrap().next_id = next;
                game_parallel_autowork::requests::Outcome::Started { job }
            } else {
                game_parallel_autowork::requests::Outcome::Refused
            };
            let witnessed = self.regions.iter().any(|region| {
                region.id == request.region
                    && region
                        .people
                        .iter()
                        .any(|person| person.id == request.requester && person.health > 0)
            });
            let requests = self.autowork.as_mut().unwrap().requests.as_mut().unwrap();
            if matches!(outcome, game_parallel_autowork::requests::Outcome::Refused) && witnessed {
                requests.observe_refusal(&request, self.tick);
            }
            if let game_parallel_autowork::requests::Outcome::Started { job } = outcome {
                requests.record_start(&request, job);
            }
            requests.pending.retain(|row| row.id != id);
            requests
                .events
                .push(game_parallel_autowork::requests::Event {
                    tick: self.tick,
                    request: id,
                    worker,
                    outcome,
                });
            Ok(())
        }
        pub fn start_work(
            &mut self,
            requests: &[game_parallel_production::Start],
        ) -> Result<(), String> {
            game_parallel_family_host::check_work_age(
                self.families.as_ref(),
                requests.iter().map(|request| request.worker),
            )?;
            self.production_actions().start(requests)
        }
        fn resolve_interrupted_requested_work(&mut self, id: u64) {
            let link = self
                .autowork
                .as_ref()
                .and_then(|a| a.requests.as_ref())
                .and_then(|r| r.work_links.iter().find(|link| link.job == id))
                .cloned();
            if let Some(link) = link {
                let visible = self.regions.iter().any(|region| {
                    region.id == link.region
                        && region
                            .people
                            .iter()
                            .any(|person| person.id == link.requester && person.health > 0)
                });
                self.autowork
                    .as_mut()
                    .unwrap()
                    .requests
                    .as_mut()
                    .unwrap()
                    .resolve(&[(id, false)], self.tick, |_| visible);
            }
        }
        pub fn cancel_work(&mut self, id: u64) -> Result<(), String> {
            self.production_actions().cancel(id)?;
            self.resolve_interrupted_requested_work(id);
            Ok(())
        }
        pub fn reassign_work(&mut self, id: u64, worker: u64) -> Result<(), String> {
            game_parallel_family_host::check_work_age(
                self.families.as_ref(),
                std::iter::once(worker),
            )?;
            let changed = self
                .production
                .as_ref()
                .and_then(|production| production.jobs.iter().find(|job| job.id == id))
                .is_some_and(|job| job.worker != worker);
            self.production_actions().reassign(id, worker)?;
            if changed {
                self.resolve_interrupted_requested_work(id);
            }
            Ok(())
        }
    };
}

//! Transactional adapter for ordinary and persistent packed production jobs.
use game_parallel_production::Production;
use game_parallel_production_model::PersonSnapshot;
use game_parallel_production_packed::PackedJobs;
pub enum Draft {
    Ordinary(game_parallel_production::Draft),
    Packed {
        people: Vec<PersonSnapshot>,
        draft: game_parallel_production_packed::Draft,
    },
}
impl Draft {
    pub fn people(&self) -> &[PersonSnapshot] {
        match self {
            Self::Ordinary(draft) => draft.people(),
            Self::Packed { people, .. } => people,
        }
    }
    pub fn discard(self) -> Vec<PersonSnapshot> {
        match self {
            Self::Ordinary(draft) => draft.discard(),
            Self::Packed { people, .. } => people,
        }
    }
    pub fn commit(
        self,
        state: &mut Production,
        packed: &mut Option<PackedJobs>,
    ) -> Vec<PersonSnapshot> {
        match self {
            Self::Ordinary(draft) => draft.commit(state),
            Self::Packed { people, draft } => {
                draft.commit(
                    packed.as_mut().expect("prepared packed jobs"),
                    &mut state.inventories,
                );
                people
            }
        }
    }
}
pub fn prepare(
    state: &Production,
    packed: Option<&PackedJobs>,
    people: Vec<PersonSnapshot>,
    tick: u64,
) -> Draft {
    match packed {
        Some(jobs) => {
            let draft = jobs.prepare(&people, &state.inventories, state.max_work_hunger, tick);
            Draft::Packed { people, draft }
        }
        None => Draft::Ordinary(state.prepare_validated(people, tick)),
    }
}
pub fn busy(state: &Production, packed: Option<&PackedJobs>) -> Vec<u64> {
    packed.map_or_else(|| state.busy_people(), PackedJobs::busy_people)
}
pub fn busy_for_tick(world: &game_parallel::World, packed: Option<&PackedJobs>) -> Vec<u64> {
    let mut busy = world
        .production
        .as_ref()
        .map_or_else(Vec::new, |state| busy(state, packed));
    if let Some(person) = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.controlled_person(world.tick))
    {
        busy.push(person);
    }
    busy
}
pub fn extract(world: &mut game_parallel::World) -> Option<PackedJobs> {
    world.production.as_mut().map(|production| {
        let mut jobs = PackedJobs::from_jobs(&production.jobs);
        production.jobs.clear();
        jobs.set_events(std::mem::take(&mut production.events));
        jobs
    })
}
pub fn export(world: &mut game_parallel::World, packed: Option<&PackedJobs>) {
    if let (Some(packed), Some(production)) = (packed, &mut world.production) {
        production.jobs = packed.jobs();
        production.events = packed.events();
    }
}

/// Command adapter that retains private packed ownership across fallback actions.
pub struct Context<'a> {
    pub world: &'a mut game_parallel::World,
    pub people: &'a [PersonSnapshot],
    pub packed: &'a mut Option<PackedJobs>,
}
impl Context<'_> {
    fn suspended(&self) -> Option<u64> {
        self.world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
    }
    fn job_worker(&self, id: u64) -> Option<u64> {
        self.packed
            .as_ref()
            .and_then(|jobs| jobs.worker_for_job(id))
            .or_else(|| {
                self.world.production.as_ref().and_then(|production| {
                    production
                        .jobs
                        .iter()
                        .find(|job| job.id == id)
                        .map(|job| job.worker)
                })
            })
    }
    fn with_jobs<T>(
        &mut self,
        action: impl FnOnce(&mut game_parallel_block_actions::Context<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if let Some(packed) = self.packed.take() {
            self.world
                .production
                .as_mut()
                .expect("packed production")
                .jobs = packed.jobs();
            self.world
                .production
                .as_mut()
                .expect("packed production")
                .events = packed.events();
            let result = action(&mut game_parallel_block_actions::Context {
                world: self.world,
                people: self.people,
            });
            let production = self.world.production.as_mut().expect("packed production");
            let mut packed = PackedJobs::from_jobs(&std::mem::take(&mut production.jobs));
            packed.set_events(std::mem::take(&mut production.events));
            *self.packed = Some(packed);
            result
        } else {
            action(&mut game_parallel_block_actions::Context {
                world: self.world,
                people: self.people,
            })
        }
    }
    pub fn busy_people(&self) -> Vec<u64> {
        self.world
            .production
            .as_ref()
            .map_or_else(Vec::new, |production| {
                busy(production, self.packed.as_ref())
            })
    }
    pub fn start(&mut self, requests: &[game_parallel::StartWork]) -> Result<(), String> {
        if let Some(property) = &self.world.property {
            property.check_jobs(
                &self
                    .world
                    .production
                    .as_ref()
                    .ok_or("production is disabled")?
                    .recipes,
                requests,
            )?;
        }
        if self
            .suspended()
            .is_some_and(|person| requests.iter().any(|request| request.worker == person))
        {
            return Err("player control is suspended".into());
        }
        if let Some(packed) = self.packed {
            let state = self.world.production.as_mut().expect("packed production");
            match packed.prepare_start(&state.recipes, &state.inventories, self.people, requests) {
                Ok(draft) => {
                    draft.commit(packed, &mut state.inventories);
                    return Ok(());
                }
                Err(error) if error != "interleaved job IDs require repacking" => {
                    return Err(error);
                }
                Err(_) => {}
            }
        }
        self.with_jobs(|actions| actions.start_work(requests))
    }
    pub fn cancel(&mut self, id: u64) -> Result<(), String> {
        if self
            .suspended()
            .is_some_and(|person| self.job_worker(id) == Some(person))
        {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.cancel_work(id))
    }
    pub fn reassign(&mut self, id: u64, worker: u64) -> Result<(), String> {
        if self
            .suspended()
            .is_some_and(|person| person == worker || self.job_worker(id) == Some(person))
        {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.reassign_work(id, worker))
    }
    pub fn propose(&mut self, proposal: game_parallel::TradeProposal) -> Result<u64, String> {
        if self.suspended() == Some(proposal.proposer) {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.propose_trade(proposal))
    }
    pub fn propose_batch(
        &mut self,
        proposals: &[game_parallel::TradeProposal],
    ) -> Result<Vec<u64>, String> {
        if self
            .suspended()
            .is_some_and(|person| proposals.iter().any(|proposal| proposal.proposer == person))
        {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.propose_trades(proposals))
    }
    pub fn decide(
        &mut self,
        id: u64,
        actor: u64,
        outcome: game_parallel::TradeOutcome,
    ) -> Result<(), String> {
        if self.suspended() == Some(actor) {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.decide_trade(id, actor, outcome))
    }
    pub fn decide_batch(
        &mut self,
        decisions: &[game_parallel::TradeDecision],
    ) -> Result<(), String> {
        if self
            .suspended()
            .is_some_and(|person| decisions.iter().any(|decision| decision.actor == person))
        {
            return Err("player control is suspended".into());
        }
        self.with_jobs(|actions| actions.decide_trades(decisions))
    }
}

pub enum WorkDraft {
    Ordinary(game_parallel_autowork::Draft),
    Packed {
        actors: Vec<u64>,
        next_id: u64,
        start: game_parallel_production_packed::StartDraft,
    },
}
impl WorkDraft {
    pub fn commit_world(self, world: &mut game_parallel::World, packed: &mut Option<PackedJobs>) {
        self.commit(
            world.production.as_mut().expect("prepared production"),
            packed,
            world.autowork.as_mut().expect("prepared autowork"),
        );
    }
    pub fn actors(&self) -> &[u64] {
        match self {
            Self::Ordinary(draft) => &draft.actors,
            Self::Packed { actors, .. } => actors,
        }
    }
    pub fn commit(
        self,
        state: &mut Production,
        packed: &mut Option<PackedJobs>,
        autowork: &mut game_parallel_autowork::Autowork,
    ) {
        match self {
            Self::Ordinary(draft) => {
                draft
                    .reservations
                    .commit(&mut state.jobs, &mut state.inventories);
                autowork.next_id = draft.next_id;
            }
            Self::Packed { start, next_id, .. } => {
                start.commit(
                    packed.as_mut().expect("prepared packed jobs"),
                    &mut state.inventories,
                );
                autowork.next_id = next_id;
            }
        }
    }
}
pub fn prepare_work(
    autowork: &game_parallel_autowork::Autowork,
    state: &Production,
    packed: Option<&PackedJobs>,
    people: &[PersonSnapshot],
    busy: &[u64],
    property: Option<&game_parallel::Property>,
) -> Result<Option<WorkDraft>, String> {
    if let Some(packed) = packed {
        let Some(plan) = autowork.plan_validated_with_property(
            people,
            state,
            busy,
            packed.max_job_id(),
            property,
        )?
        else {
            return Ok(None);
        };
        let start =
            packed.prepare_planned(&state.recipes, &state.inventories, people, &plan.requests)?;
        Ok(Some(WorkDraft::Packed {
            actors: plan.actors,
            next_id: plan.next_id,
            start,
        }))
    } else {
        Ok(autowork
            .prepare_with_property(people, state, busy, property)?
            .map(WorkDraft::Ordinary))
    }
}

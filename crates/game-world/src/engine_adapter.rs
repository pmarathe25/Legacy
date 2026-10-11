//! Composition-owned registrations. The engine sees stable IDs and callbacks,
//! while these adapters connect the actual optional feature state.
use crate::*;
use game_sim::{Clock, ClockWorld, Engine, Interval, Module, ModuleId, PhaseId, PhaseKey};

/// One opening checked sum of closing IDs and publications. Domain demands
/// stay with their owners; checking the sum prevents individually valid bounds
/// from exhausting the shared counters halfway through publication.
#[derive(Default)]
pub(crate) struct ResourceEnvelope {
    ids: u64,
    events: u64,
}
impl ResourceEnvelope {
    pub(crate) fn add_ids(&mut self, count: u64) -> Result<(), Error> {
        self.ids = self.ids.checked_add(count).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn add_events(&mut self, count: u64) -> Result<(), Error> {
        self.events = self.events.checked_add(count).ok_or(Error::Overflow)?;
        Ok(())
    }
    pub(crate) fn check(&self, next_id: u64, next_event: u64) -> Result<(), Error> {
        next_id.checked_add(self.ids).ok_or(Error::Overflow)?;
        next_event.checked_add(self.events).ok_or(Error::Overflow)?;
        Ok(())
    }
}

pub(crate) type WorldEngine = Engine<Simulation, StopReason>;
pub(crate) const PREPARE: PhaseKey = PhaseKey {
    module: ModuleId(100),
    phase: PhaseId(0),
};
pub(crate) const COMMIT: PhaseKey = PhaseKey {
    module: ModuleId(100),
    phase: PhaseId(1),
};
impl ClockWorld for Simulation {
    fn clock(&self) -> Clock {
        Clock {
            time: self.state.time,
            revision: self.revision,
        }
    }
    fn publish_clock(&mut self, clock: Clock) {
        self.state.time = clock.time;
        self.revision = clock.revision;
    }
}
fn due(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world
        .state
        .due
        .first_key_value()
        .map_or(interval.to, |(at, _)| *at)
        .min(interval.to))
}
fn agreements(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(interval
        .to
        .min(world.next_agreement_deadline().unwrap_or(interval.to)))
}
fn physical(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.physical_boundary(interval.to))
}
fn environment(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    world.environment_boundary(interval.to)
}
fn projects(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.project_boundary(interval.to))
}
fn invocation(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.invocation_boundary(interval.to))
}
fn missions(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.mission_boundary(interval.to))
}
fn social(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.social_boundary(interval.to))
}
fn health(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    world.health_boundary(interval.to)
}
fn realm(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.realm_boundary(interval.to))
}
fn armies(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.army_boundary(interval.to))
}
fn transport(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    world.transport_boundary(interval.to)
}
fn agriculture(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world
        .state
        .agriculture
        .as_ref()
        .map_or(interval.to, |crop| {
            crop.next_boundary(interval.from, interval.to)
        }))
}
fn genetics(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    Ok(world.traits_boundary(interval.to))
}
fn skills(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    world.skill_boundary(interval.to)
}
fn bodies(world: &Simulation, interval: Interval) -> Result<SimTime, Error> {
    world.body_boundary(interval.to)
}

pub(crate) fn build(execution: &Execution) -> Result<WorldEngine, Error> {
    let callbacks: [game_sim::Boundary<Simulation>; 16] = [
        due,
        agreements,
        physical,
        environment,
        projects,
        invocation,
        missions,
        social,
        health,
        realm,
        armies,
        transport,
        genetics,
        skills,
        bodies,
        agriculture,
    ];
    // Preserve the existing clipping dependencies. Every callback is safe when
    // its state is absent, so later constructor installations need no refresh.
    let mut modules: Vec<Module<Simulation, StopReason>> = callbacks
        .into_iter()
        .enumerate()
        .map(|(index, boundary)| {
            let mut module = Module::new(ModuleId(index as u32));
            module.boundary = Some(boundary);
            module
        })
        .collect();
    modules[5].input_stop = Some(|world, _| {
        Ok(world
            .required_invocation_input()
            .map(|warning| StopReason::InvocationWarning { warning }))
    });
    let mut control = Module::<Simulation, StopReason>::new(ModuleId(16));
    control.input_stop = Some(|world, _| {
        Ok(world
            .awaiting_combat_input()
            .map(|encounter| StopReason::CombatInput { encounter }))
    });
    modules.push(control);
    modules.push(
        Module::new(PREPARE.module)
            .phase(PREPARE.phase, [])
            .phase(COMMIT.phase, [PREPARE]),
    );
    Engine::new(
        modules,
        game_sim::Execution {
            workers: execution.workers,
            reverse_jobs: execution.reverse_jobs,
        },
    )
}

pub(crate) struct PreparedWorldStep {
    pub next: SimTime,
    pub elapsed: u64,
    pub equipment: BTreeMap<PersonId, bool>,
    pub output: Vec<compute::ProgressOutput>,
    pub physical_output: Vec<physical::Outcome>,
    pub skill_output: Vec<skills::PracticeDelta>,
    pub project_output: Option<projects::World>,
    pub invocation_output: invocation::PendingInvocation,
    pub environment_output: Option<environment::Step>,
    pub agriculture_output: Option<game_agriculture::PreparedAdvance>,
    pub ecology_output: Option<game_ecology::PreparedAdvance>,
    pub health_output: health::PendingHealth,
    pub due: Vec<Due>,
    pub lifecycle: Vec<Due>,
    pub transfers: Vec<TransferIntent>,
    pub opening_stock: BTreeMap<InventoryId, u64>,
    pub evaluated_people: usize,
    pub evaluation_ns: u128,
    pub arbitration_ns: u128,
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    #[test]
    fn individually_fitting_domains_must_fit_the_same_checked_counter_together() {
        let mut ids = ResourceEnvelope::default();
        ids.add_ids(3).unwrap();
        ids.add_ids(3).unwrap();
        assert!((u64::MAX - 5).checked_add(3).is_some());
        assert_eq!(ids.check(u64::MAX - 5, 1), Err(Error::Overflow));
        assert_eq!(ids.check(u64::MAX - 6, 1), Ok(()));
        let mut events = ResourceEnvelope::default();
        events.add_events(4).unwrap();
        events.add_events(4).unwrap();
        assert!((u64::MAX - 6).checked_add(4).is_some());
        assert_eq!(events.check(1, u64::MAX - 6), Err(Error::Overflow));
        assert_eq!(events.check(1, u64::MAX - 8), Ok(()));
    }
}

//! Composition only. Features own gameplay, fixtures own initial conditions.
mod binding;
mod plugins;
use game_api::*;
use game_sim::*;
pub struct Simulation {
    host: Host,
}
impl Simulation {
    /// Developer research start: no history warmup or player assignment.
    pub fn uncontrolled(
        seed: u64,
        workers: usize,
        store: &mut game_content::Store,
    ) -> Result<Self> {
        let mut host = plugins::builder()?.build(seed, workers)?;
        game_openings::seed_world(&mut host, store, seed)?;
        Ok(Self { host })
    }
    pub fn new(scenario: &str, seed: u64, workers: usize) -> Result<Self> {
        let mut store = if scenario == "generated" {
            game_content::Store::standard()
        } else {
            game_content::Store::fixtures()
        };
        Self::with_content(scenario, seed, workers, &mut store)
    }
    pub fn with_content(
        scenario: &str,
        seed: u64,
        workers: usize,
        store: &mut game_content::Store,
    ) -> Result<Self> {
        let mut host = plugins::builder()?.build(seed, workers)?;
        game_openings::initialize(&mut host, store, scenario, seed)?;
        Ok(Self { host })
    }
    pub fn load(save: &str, workers: usize) -> Result<Self> {
        Ok(Self {
            host: plugins::load(save, workers)?,
        })
    }
    pub fn save(&self) -> Result<String> {
        self.host.save()
    }
    pub fn save_to(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        self.host.save_to(writer)
    }
    pub fn submit(&mut self, sequence: u64, command: Command) -> Result<CommandResult> {
        binding::submit(&mut self.host, sequence, command)
    }
    pub fn observe(&self) -> Result<PlayerView> {
        binding::observe(&self.host)
    }
    pub fn advance(&mut self, target: SimTime, budget: usize) -> Result<Progress> {
        self.host.advance(target, budget)
    }
    pub fn now(&self) -> SimTime {
        self.host.now()
    }
    /// Privileged construction and inspection for developer fixtures, never a player capability.
    pub fn debug_host(&self) -> &Host {
        &self.host
    }
    pub fn debug_host_mut(&mut self) -> &mut Host {
        &mut self.host
    }
}
pub fn scenarios() -> Result<Vec<game_openings::Scenario>> {
    game_openings::scenarios(&mut game_content::Store::fixtures())
}
pub fn scenarios_with_content(
    store: &mut game_content::Store,
) -> Result<Vec<game_openings::Scenario>> {
    game_openings::scenarios(store)
}
pub fn default_scenario(store: &mut game_content::Store) -> Result<String> {
    game_openings::default_scenario(store)
}

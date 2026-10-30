//! Production admission against the persistent physical index.
use crate::Context;
use game_parallel::StartWork;
impl Context<'_> {
    pub fn start_work(&mut self, requests: &[StartWork]) -> Result<(), String> {
        let state = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let draft = game_parallel_production_admission::Context {
            jobs: &state.jobs,
            recipes: &state.recipes,
            inventories: &state.inventories,
            people: self.people,
        }
        .prepare(requests)?;
        let state = self.world.production.as_mut().unwrap();
        draft.commit(&mut state.jobs, &mut state.inventories);
        Ok(())
    }
    pub fn cancel_work(&mut self, id: u64) -> Result<(), String> {
        let state = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let next = state.cancel_indexed(&self.world.regions, self.people, self.world.tick, id)?;
        self.world.production = Some(next);
        Ok(())
    }
    pub fn reassign_work(&mut self, id: u64, worker: u64) -> Result<(), String> {
        let state = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        if let (Some(property), Some(job)) = (
            &self.world.property,
            state.jobs.iter().find(|job| job.id == id),
        ) {
            property.check_jobs(
                &state.recipes,
                &[StartWork {
                    id,
                    worker,
                    region: job.region,
                    recipe: job.recipe,
                    units: 1,
                }],
            )?;
        }
        let next = state.reassign_indexed(
            &self.world.regions,
            self.people,
            self.world.tick,
            id,
            worker,
        )?;
        self.world.production = Some(next);
        Ok(())
    }
}

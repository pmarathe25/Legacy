//! Paused and active work share one authoritative job component on the worker.
use super::*;
use game_ecs::{Schema, StableId, Store};
impl Job {
    pub fn validate(&self) -> Result<(), Error> {
        if self.site.0 == 0 || self.tool.is_some_and(|id| id.0 == 0) {
            return Err(Error::InvalidConfiguration(
                "invalid workshop job identity".into(),
            ));
        }
        // These rates are rational mechanics, not one particular human setting.
        // Skill ceilings, available tools, and site duration require host facts.
        game_skills::advance_work(0, self.rate, self.work_remainder)?;
        game_skills::advance_work(0, self.body_rate, 0)?;
        // Zero remaining work is valid while output storage is blocked.
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Job>("workshop/job", Job::validate)
}
pub fn job(store: &Store, actor: PersonId) -> Option<&Job> {
    store.get(StableId(actor.0))
}
pub fn job_mut(store: &mut Store, actor: PersonId) -> Option<&mut Job> {
    store.get_mut(StableId(actor.0))
}
pub fn jobs(store: &Store) -> impl Iterator<Item = (PersonId, &Job)> {
    store.iter::<Job>().map(|(id, job)| (PersonId(id.0), job))
}
pub fn insert_job(store: &mut Store, actor: PersonId, job: Job) -> Result<(), Error> {
    job.validate()?;
    if !store.schema().is_registered::<Job>() {
        return Err(Error::InvalidConfiguration(
            "workshop job component is not registered".into(),
        ));
    }
    store.ensure(StableId(actor.0))?;
    store.insert(StableId(actor.0), job)
}
pub fn remove_job(store: &mut Store, actor: PersonId) -> Result<Option<Job>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}

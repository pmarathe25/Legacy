//! Sites and worker jobs live in the host's sole component store.
use super::*;
use game_ecs::{Schema, StableId, Store};
impl Site {
    /// Validate the declaration without interpreting host-owned stocks, areas,
    /// people, or permissions.
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.area.0 == 0
            || self.input.0 == 0
            || self.tool.0 == 0
            || self.output.0 == 0
            || self.input_quantity.0 == 0
            || self.output_quantity.0 == 0
            || self.duration.0 == 0
            || self.access.iter().any(|id| id.0 == 0)
            || self.last_granted.is_some_and(|id| id.0 == 0)
        {
            return Err(Error::InvalidConfiguration(
                "invalid workshop site declaration".into(),
            ));
        }
        Ok(())
    }
}
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
    schema.register::<Site>("workshop/site", Site::validate)?;
    schema.register::<Job>("workshop/job", Job::validate)
}
pub fn site(store: &Store, id: SiteId) -> Option<&Site> {
    store.get(StableId(id.0))
}
pub fn site_mut(store: &mut Store, id: SiteId) -> Option<&mut Site> {
    store.get_mut(StableId(id.0))
}
pub fn sites(store: &Store) -> impl Iterator<Item = (SiteId, &Site)> {
    store.iter::<Site>().map(|(id, site)| (SiteId(id.0), site))
}
pub fn insert_site(store: &mut Store, site: Site) -> Result<(), Error> {
    site.validate()?;
    store.insert_or_spawn(StableId(site.id.0), site)
}
pub fn remove_site(store: &mut Store, id: SiteId) -> Result<Option<Site>, Error> {
    if !store.contains(StableId(id.0)) {
        return Ok(None);
    }
    store.remove(StableId(id.0))
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
    store.insert_or_spawn(StableId(actor.0), job)
}
pub fn remove_job(store: &mut Store, actor: PersonId) -> Result<Option<Job>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}

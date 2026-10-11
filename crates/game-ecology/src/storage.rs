//! Feature-owned declarations in the host's sole component store.
use super::*;
use game_ecs::{Schema, StableId, Store};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Resource>("ecology/resource", Resource::validate)?;
    schema.register::<Plot>("ecology/plot", Plot::validate)?;
    schema.register::<Gathering>("ecology/gathering", Gathering::validate)?;
    schema.register::<Knowledge>("ecology/knowledge", Knowledge::validate)?;
    schema.register::<Requests>("ecology/requests", Requests::validate)
}

impl Gathering {
    pub fn validate(&self) -> Result<(), Error> {
        if self.resource.0 == 0
            || self.destination.0 == 0
            || self.quantity.0 == 0
            || self.tool.is_some_and(|tool| tool.0 == 0)
            || self.completes.0 == 0
        {
            return Err(Error::InvalidConfiguration(
                "invalid gathering component".into(),
            ));
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.observed_areas.len() > 16
            || self
                .known_resources
                .iter()
                .any(|(id, view)| *id != view.id || view.contamination > 1_000)
            || self
                .known_plots
                .iter()
                .any(|(id, view)| *id != view.id || view.soil_fertility > 1_000)
        {
            return Err(Error::InvalidConfiguration(
                "invalid personal ecological knowledge".into(),
            ));
        }
        Ok(())
    }
}
impl Requests {
    pub fn validate(&self) -> Result<(), Error> {
        if self.next_request.keys().any(|resource| resource.0 == 0) {
            return Err(Error::InvalidConfiguration(
                "invalid personal resource request".into(),
            ));
        }
        Ok(())
    }
}
pub struct Queries {
    pub(super) resources: game_ecs::Query1<Resource>,
    pub(super) plots: game_ecs::Query2<Plot, Soil>,
}
impl Queries {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.resources.refresh(store)?;
        self.plots.refresh(store)
    }
}
impl Runtime {
    pub fn new(rules: EcologyRules, day: u64) -> Result<Self, Error> {
        rules.validate()?;
        Ok(Self { rules, day })
    }
    /// Validate this feature's registered data without interpreting another
    /// feature's people, topology, inventory, or simulation clock.
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        self.rules.validate()?;
        for (id, resource) in self.resources(store) {
            resource.validate()?;
            if id != resource.id {
                return Err(Error::InvalidSave("inconsistent resource entity".into()));
            }
        }
        for (id, plot) in self.plots(store) {
            plot.validate()?;
            game_land::soil(store, id)
                .ok_or_else(|| Error::InvalidSave("missing plot soil".into()))?
                .validate()?;
            if id != plot.id {
                return Err(Error::InvalidSave("inconsistent plot entity".into()));
            }
        }
        for (_, job) in self.gatherings(store) {
            job.validate()?;
        }
        for (_, known) in self.all_knowledge(store) {
            known.validate()?;
        }
        for (_, requests) in self.all_requests(store) {
            requests.validate()?;
        }
        Ok(())
    }
    pub fn bind(store: &mut Store) -> Queries {
        Queries {
            resources: store.query1::<Resource>(),
            plots: store.query2::<Plot, Soil>(),
        }
    }
    pub fn resources<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (ResourceId, &'a Resource)> {
        store
            .iter::<Resource>()
            .map(|(id, row)| (ResourceId(id.0), row))
    }
    pub fn plots<'a>(&self, store: &'a Store) -> impl Iterator<Item = (SiteId, &'a Plot)> {
        store.iter::<Plot>().map(|(id, row)| (SiteId(id.0), row))
    }
    pub fn resource<'a>(&self, store: &'a Store, id: ResourceId) -> Option<&'a Resource> {
        store.get(StableId(id.0))
    }
    pub fn resource_mut<'a>(
        &self,
        store: &'a mut Store,
        id: ResourceId,
    ) -> Option<&'a mut Resource> {
        store.get_mut(StableId(id.0))
    }
    pub fn plot<'a>(&self, store: &'a Store, id: SiteId) -> Option<&'a Plot> {
        store.get(StableId(id.0))
    }
    pub fn plot_mut<'a>(&self, store: &'a mut Store, id: SiteId) -> Option<&'a mut Plot> {
        store.get_mut(StableId(id.0))
    }
    pub fn insert_resource(&self, store: &mut Store, resource: Resource) -> Result<(), Error> {
        resource.validate()?;
        store.insert_or_spawn(StableId(resource.id.0), resource)
    }
    /// Attach land-use metadata without replacing existing soil.
    pub fn insert_plot(&self, store: &mut Store, plot: Plot) -> Result<(), Error> {
        plot.validate()?;
        game_land::soil(store, plot.id)
            .ok_or(Error::Unavailable)?
            .validate()?;
        store.insert(StableId(plot.id.0), plot)
    }
    /// Author one plot and its soil atomically, including schema validation.
    pub fn insert_plot_soil(&self, store: &mut Store, plot: Plot, soil: Soil) -> Result<(), Error> {
        plot.validate()?;
        soil.validate()?;
        store.insert_pair_or_spawn(StableId(plot.id.0), plot, soil)
    }
    pub fn soil<'a>(&self, store: &'a Store, id: SiteId) -> Option<&'a Soil> {
        game_land::soil(store, id)
    }
    pub fn soil_mut<'a>(&self, store: &'a mut Store, id: SiteId) -> Option<&'a mut Soil> {
        game_land::soil_mut(store, id)
    }
    pub fn remove_resource(
        &self,
        store: &mut Store,
        id: ResourceId,
    ) -> Result<Option<Resource>, Error> {
        if !store.contains(StableId(id.0)) {
            return Ok(None);
        }
        store.remove(StableId(id.0))
    }
    pub fn remove_plot(&self, store: &mut Store, id: SiteId) -> Result<Option<Plot>, Error> {
        if !store.contains(StableId(id.0)) {
            return Ok(None);
        }
        store.remove(StableId(id.0))
    }
}

impl Runtime {
    pub fn gathering<'a>(&self, store: &'a Store, actor: PersonId) -> Option<&'a Gathering> {
        store.get(StableId(actor.0))
    }
    pub fn gathering_mut<'a>(
        &self,
        store: &'a mut Store,
        actor: PersonId,
    ) -> Option<&'a mut Gathering> {
        store.get_mut(StableId(actor.0))
    }
    pub fn gatherings<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (PersonId, &'a Gathering)> {
        store
            .iter::<Gathering>()
            .map(|(id, row)| (PersonId(id.0), row))
    }
    pub fn insert_gathering(
        &self,
        store: &mut Store,
        actor: PersonId,
        row: Gathering,
    ) -> Result<(), Error> {
        row.validate()?;
        store.insert_or_spawn(StableId(actor.0), row)
    }
    pub fn remove_gathering(
        &self,
        store: &mut Store,
        actor: PersonId,
    ) -> Result<Option<Gathering>, Error> {
        if !store.contains(StableId(actor.0)) {
            return Ok(None);
        }
        store.remove(StableId(actor.0))
    }
    pub fn knowledge<'a>(&self, store: &'a Store, actor: PersonId) -> Option<&'a Knowledge> {
        store.get(StableId(actor.0))
    }
    pub fn knowledge_mut<'a>(
        &self,
        store: &'a mut Store,
        actor: PersonId,
    ) -> Option<&'a mut Knowledge> {
        store.get_mut(StableId(actor.0))
    }
    pub fn all_knowledge<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (PersonId, &'a Knowledge)> {
        store
            .iter::<Knowledge>()
            .map(|(id, row)| (PersonId(id.0), row))
    }
    pub fn insert_knowledge(
        &self,
        store: &mut Store,
        actor: PersonId,
        row: Knowledge,
    ) -> Result<(), Error> {
        row.validate()?;
        store.insert_or_spawn(StableId(actor.0), row)
    }
    pub fn remove_knowledge(
        &self,
        store: &mut Store,
        actor: PersonId,
    ) -> Result<Option<Knowledge>, Error> {
        if !store.contains(StableId(actor.0)) {
            return Ok(None);
        }
        store.remove(StableId(actor.0))
    }
    pub fn requests<'a>(&self, store: &'a Store, actor: PersonId) -> Option<&'a Requests> {
        store.get(StableId(actor.0))
    }
    pub fn requests_mut<'a>(
        &self,
        store: &'a mut Store,
        actor: PersonId,
    ) -> Option<&'a mut Requests> {
        store.get_mut(StableId(actor.0))
    }
    pub fn all_requests<'a>(
        &self,
        store: &'a Store,
    ) -> impl Iterator<Item = (PersonId, &'a Requests)> {
        store
            .iter::<Requests>()
            .map(|(id, row)| (PersonId(id.0), row))
    }
    pub fn insert_requests(
        &self,
        store: &mut Store,
        actor: PersonId,
        row: Requests,
    ) -> Result<(), Error> {
        row.validate()?;
        store.insert_or_spawn(StableId(actor.0), row)
    }
    pub fn remove_requests(
        &self,
        store: &mut Store,
        actor: PersonId,
    ) -> Result<Option<Requests>, Error> {
        if !store.contains(StableId(actor.0)) {
            return Ok(None);
        }
        store.remove(StableId(actor.0))
    }
}

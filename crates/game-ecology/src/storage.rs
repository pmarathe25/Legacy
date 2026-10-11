//! Feature-owned declarations in the host's sole component store.
use super::*;
use game_ecs::{Schema, StableId, Store};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Resource>("ecology/resource", Resource::validate)?;
    schema.register::<Plot>("ecology/plot", Plot::validate)
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
        Ok(Self {
            rules,
            jobs: BTreeMap::new(),
            known: BTreeMap::new(),
            known_plots: BTreeMap::new(),
            observed_areas: BTreeMap::new(),
            day,
            next_request: BTreeMap::new(),
        })
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

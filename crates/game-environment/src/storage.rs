//! Feature-owned declarations and borrowed local forcing rows.
use super::*;
use game_ecs::{Query2, Schema};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<LocalClimate>("environment/local-climate", LocalClimate::validate)?;
    schema.register::<Weather>("environment/weather", Weather::validate)
}
pub struct Queries {
    weather: Query2<LocalClimate, Weather>,
}
impl Queries {
    pub fn refresh(&mut self, store: &Store) -> Result<(), Error> {
        self.weather.refresh(store)
    }
    pub(super) fn rows<'a>(
        &'a self,
        store: &'a Store,
    ) -> Result<impl Iterator<Item = (StableId, &'a LocalClimate, &'a Weather)> + 'a, Error> {
        self.weather.iter(store)
    }
}
impl Runtime {
    pub fn bind(store: &mut Store) -> Queries {
        Queries {
            weather: store.query2::<LocalClimate, Weather>(),
        }
    }
}

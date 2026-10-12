//! Canonical pending travel steps, independent of the optional carrying runtime.
use crate::*;
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub steps: Vec<AreaId>,
}
impl Route {
    pub fn validate(&self) -> Result<(), Error> {
        if self.steps.is_empty() || self.steps.iter().any(|area| area.0 == 0) {
            return Err(Error::InvalidSave("invalid pending transport route".into()));
        }
        Ok(())
    }
}
pub fn route(store: &Store, actor: PersonId) -> Option<&Route> {
    store.get(StableId(actor.0))
}
pub fn routes(store: &Store) -> impl Iterator<Item = (PersonId, &Route)> {
    store.iter::<Route>().map(|(id, row)| (PersonId(id.0), row))
}
pub fn route_steps(store: &Store, actor: PersonId) -> &[AreaId] {
    route(store, actor).map_or(&[], |row| row.steps.as_slice())
}
pub fn route_mut(store: &mut Store, actor: PersonId) -> Option<&mut Route> {
    store.get_mut(StableId(actor.0))
}
pub fn insert_route(store: &mut Store, actor: PersonId, row: Route) -> Result<(), Error> {
    row.validate()?;
    store.insert_or_spawn(StableId(actor.0), row)
}
pub fn remove_route(store: &mut Store, actor: PersonId) -> Result<Option<Route>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}
/// An owner-admitted replacement. Empty routes have no stored component.
/// Interruption coordinators accept this value, rather than a list of supposedly
/// replaced IDs or an untyped batch footprint, when preserving pending travel.
pub struct PreparedRoute {
    actor: PersonId,
    before: Option<Route>,
    after: Option<Route>,
    location_read: Option<Location>,
}
pub fn prepare_route_replacement(
    store: &Store,
    actor: PersonId,
    steps: Vec<AreaId>,
) -> Result<PreparedRoute, Error> {
    if actor.0 == 0 || !store.contains(StableId(actor.0)) {
        return Err(Error::Unavailable);
    }
    let after = (!steps.is_empty()).then_some(Route { steps });
    if let Some(row) = &after {
        row.validate()?;
    }
    Ok(PreparedRoute {
        actor,
        before: route(store, actor).cloned(),
        after,
        location_read: None,
    })
}
impl PreparedRoute {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
    pub fn steps(&self) -> &[AreaId] {
        self.after.as_ref().map_or(&[], |row| row.steps.as_slice())
    }
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if !store.contains(StableId(self.actor.0))
            || route(store, self.actor) != self.before.as_ref()
            || self.location_read.as_ref().is_some_and(|before| {
                game_spatial::location(store, StableId(self.actor.0)) != Some(before)
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<RouteAdmission, Error> {
        self.validate(store)?;
        if batch.touches::<Route>(StableId(self.actor.0)) {
            return Err(Error::Unavailable);
        }
        if let Some(location) = self.location_read {
            if batch.touches::<Location>(StableId(self.actor.0)) {
                return Err(Error::Unavailable);
            }
            batch.insert(StableId(self.actor.0), location);
        }
        // Stage even a no-op removal: the typed key reserves this admitted
        // replacement against foreign writes added later to the shared batch.
        if let Some(after) = self.after {
            batch.insert(StableId(self.actor.0), after);
        } else {
            batch.remove::<Route>(StableId(self.actor.0));
        }
        Ok(RouteAdmission {
            actor: self.actor,
            batch_key: batch.key(),
        })
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let admission = self.stage_into(store, &mut batch)?;
        admission.validate_footprint(&batch)?;
        store.apply_batch(batch)
    }
}
/// The topology owner supplies an actual actor location and directed links.
pub trait RouteReferences {
    /// Actual person placements only; unknown actor IDs must return None even
    /// if an unrelated entity has a forged spatial row.
    fn location(&self, actor: PersonId) -> Option<&Location>;
    fn link(&self, from: AreaId, to: AreaId) -> bool;
}
pub fn validate_routes(store: &Store, references: &impl RouteReferences) -> Result<(), Error> {
    let fail = || Error::InvalidSave("invalid transport route references".into());
    for (actor, row) in routes(store) {
        row.validate().map_err(|_| fail())?;
        let mut from = match references.location(actor).ok_or_else(fail)? {
            Location::AtArea(_) => return Err(fail()),
            Location::InTransit { to, .. } => *to,
            Location::StoppedOnRoad { .. } | Location::Carried { .. } => return Err(fail()),
        };
        for to in &row.steps {
            if !references.link(from, *to) {
                return Err(fail());
            }
            from = *to;
        }
    }
    Ok(())
}
/// Stable shortest-path policy over plain directed link facts.
pub fn shortest_path(
    roads: &KnownRoutes,
    origin: AreaId,
    destination: AreaId,
) -> Result<Vec<AreaId>, Error> {
    game_spatial::shortest_path(roads, origin, destination).map(|(path, _)| path)
}
#[cfg(test)]
mod tests;
impl PreparedRoute {
    /// Consume a next leg in the admitted route, without publishing an
    /// intermediate replacement or exposing its guarded opening component.
    pub fn consume_next(mut self, destination: AreaId) -> Result<Self, Error> {
        let route = self.after.as_mut().ok_or(Error::Unavailable)?;
        if route.steps.first() != Some(&destination) {
            return Err(Error::Unavailable);
        }
        route.steps.remove(0);
        if route.steps.is_empty() {
            self.after = None;
        }
        Ok(self)
    }
    pub(crate) fn into_edit(
        self,
        store: &Store,
    ) -> Result<(PersonId, Option<Route>, Option<Route>), Error> {
        self.validate(store)?;
        Ok((self.actor, self.before, self.after))
    }
}
pub fn prepare_route_to(
    store: &Store,
    actor: PersonId,
    destination: AreaId,
    roads: &KnownRoutes,
) -> Result<PreparedRoute, Error> {
    let location = game_spatial::location(store, StableId(actor.0)).ok_or(Error::Unavailable)?;
    let origin = match location {
        Location::AtArea(area) => *area,
        Location::InTransit { to, .. } => *to,
        Location::StoppedOnRoad { .. } | Location::Carried { .. } => {
            return Err(Error::Unavailable);
        }
    };
    let path = shortest_path(roads, origin, destination)?;
    if path.is_empty()
        && (matches!(location, Location::AtArea(_)) || route_steps(store, actor).is_empty())
    {
        return Err(Error::AlreadyThere);
    }
    let mut plan = prepare_route_replacement(store, actor, path)?;
    plan.location_read = Some(location.clone());
    Ok(plan)
}

/// Proof that Transport actually staged this replacement in this batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteAdmission {
    pub(crate) actor: PersonId,
    pub(crate) batch_key: game_ecs::BatchKey,
}
impl RouteAdmission {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
    pub fn validate_footprint(&self, batch: &Batch) -> Result<(), Error> {
        if self.batch_key != batch.key()
            || batch
                .edited_ids::<Route>()
                .filter(|id| id.0 == self.actor.0)
                .count()
                != 1
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

pub fn replace_route(store: &mut Store, actor: PersonId, steps: Vec<AreaId>) -> Result<(), Error> {
    prepare_route_replacement(store, actor, steps)?.commit(store)
}
pub fn consume_route_step(
    store: &mut Store,
    actor: PersonId,
    destination: AreaId,
) -> Result<(), Error> {
    prepare_route_replacement(store, actor, route_steps(store, actor).to_vec())?
        .consume_next(destination)?
        .commit(store)
}

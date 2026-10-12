//! Spatial data and carrier resolution, independent of transport policy.
//! Locations remain in the host's shared store. No persistent resolved-position
//! cache can become a second authority for an entity's position.
use game_core::{AreaId, Error, SimTime};
pub use game_core::{Location, RoadPosition};
use game_ecs::{Schema, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};

/// Intrinsic geometry and identifier checks. Area membership, road availability,
/// time, and carrying consent require facts from their respective owners.
pub fn validate(location: &Location) -> Result<(), Error> {
    let valid_road = |point: RoadPosition| point.from.0 != 0 && point.to.0 != 0 && point.valid();
    let valid = match *location {
        Location::AtArea(area) => area.0 != 0,
        Location::Carried { carrier } => carrier.0 != 0,
        Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } => valid_road(RoadPosition {
            from,
            to,
            progress,
            duration,
        }),
        Location::InTransit {
            from,
            to,
            departed,
            arrives,
            road_start,
        } => {
            from.0 != 0
                && to.0 != 0
                && from != to
                && arrives > departed
                && road_start.is_none_or(|point| {
                    valid_road(point)
                        && ((point.from == from && point.to == to)
                            || (point.from == to && point.to == from))
                })
        }
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidConfiguration(
            "invalid spatial location".into(),
        ))
    }
}

/// Read-only topology facts for contextual validation. Travel speed, arrival
/// scheduling, living bodies, and carrier agreements remain outside this crate.
pub struct Context<'a> {
    pub now: SimTime,
    pub areas: &'a BTreeSet<AreaId>,
    pub links: &'a BTreeSet<(AreaId, AreaId)>,
}
pub fn validate_context(location: &Location, facts: &Context<'_>) -> Result<(), Error> {
    validate(location)?;
    let known_road = |from, to| {
        facts.areas.contains(&from)
            && facts.areas.contains(&to)
            && facts.links.contains(&(from, to))
    };
    let valid = match *location {
        Location::AtArea(area) => facts.areas.contains(&area),
        Location::Carried { .. } => true, // resolve() separately checks the actual anchor graph.
        Location::StoppedOnRoad { from, to, .. } => known_road(from, to),
        Location::InTransit {
            from,
            to,
            departed,
            road_start,
            ..
        } => {
            departed <= facts.now
                && known_road(from, to)
                && road_start.is_none_or(|point| known_road(point.from, point.to))
        }
    };
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidConfiguration(
            "spatial location lacks topology or time facts".into(),
        ))
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Location>("spatial/location", validate)
}
pub fn location(store: &Store, entity: StableId) -> Option<&Location> {
    store.get(entity)
}
pub fn location_mut(store: &mut Store, entity: StableId) -> Option<&mut Location> {
    store.get_mut(entity)
}
pub fn locations(store: &Store) -> impl Iterator<Item = (StableId, &Location)> {
    store.iter::<Location>()
}
pub fn set_location(store: &mut Store, entity: StableId, value: Location) -> Result<(), Error> {
    validate(&value)?;
    store.insert_or_spawn(entity, value)
}
pub fn remove_location(store: &mut Store, entity: StableId) -> Result<Option<Location>, Error> {
    if !store.contains(entity) {
        return Ok(None);
    }
    store.remove(entity)
}

/// Returns the terminal entity and its borrowed canonical row. Missing anchors
/// and cycles fail explicitly rather than assigning a guessed cell or endpoint.
pub fn resolved_anchor(store: &Store, entity: StableId) -> Result<(StableId, &Location), Error> {
    let mut current = entity;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current) {
            return Err(Error::InvalidSave("cyclic spatial carrying".into()));
        }
        let row = location(store, current)
            .ok_or_else(|| Error::InvalidSave("missing spatial anchor".into()))?;
        validate(row).map_err(|_| Error::InvalidSave("invalid spatial anchor".into()))?;
        match *row {
            Location::Carried { carrier } => current = StableId(carrier.0),
            _ => return Ok((current, row)),
        }
    }
}
pub fn resolve(store: &Store, entity: StableId) -> Result<Location, Error> {
    Ok(resolved_anchor(store, entity)?.1.clone())
}
pub fn cell(store: &Store, entity: StableId) -> Result<AreaId, Error> {
    resolved_anchor(store, entity)?
        .1
        .cell()
        .ok_or_else(|| Error::InvalidSave("unresolved spatial cell".into()))
}

/// Resolves one immutable borrowed catalog in stable ID order, memoizing shared
/// carrier paths. The returned map is a transient observation, never saved state.
pub fn resolve_locations<'a>(
    rows: impl IntoIterator<Item = (StableId, &'a Location)>,
) -> Result<BTreeMap<StableId, Location>, Error> {
    let mut catalog = BTreeMap::new();
    for (id, row) in rows {
        if id.0 == 0 || catalog.insert(id, row).is_some() {
            return Err(Error::InvalidSave(
                "duplicate or zero spatial entity".into(),
            ));
        }
    }
    let mut resolved: BTreeMap<StableId, Location> = BTreeMap::new();
    for actor in catalog.keys() {
        if resolved.contains_key(actor) {
            continue;
        }
        let mut current = *actor;
        let mut path = BTreeSet::new();
        let position = loop {
            if let Some(position) = resolved.get(&current) {
                break position.clone();
            }
            if !path.insert(current) {
                return Err(Error::InvalidSave("cyclic spatial carrying".into()));
            }
            let row = catalog
                .get(&current)
                .ok_or_else(|| Error::InvalidSave("missing spatial anchor".into()))?;
            validate(row).map_err(|_| Error::InvalidSave("invalid spatial anchor".into()))?;
            match **row {
                Location::Carried { carrier } => current = StableId(carrier.0),
                _ => break (*row).clone(),
            }
        };
        for actor in path {
            resolved.insert(actor, position.clone());
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests;

mod roster;
pub use roster::{CandidateIds, LocalCandidates, LocalRoster};

mod navigation;
pub use navigation::{RoadGraph, shortest_path};

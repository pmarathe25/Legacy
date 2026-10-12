//! Borrowed phase indexes, rebuilt after any spatial mutation or restore.
use crate::*;

/// Derived local identities retain an immutable Store borrow for the entire
/// query phase. They cannot outlive or silently cross a value-only Location
/// mutation. The host builds once per declared phase, shares this view with
/// systems, then drops it before motion, births, carrying changes or restore.
pub struct LocalRoster<'a> {
    store: &'a Store,
    raw_areas: BTreeMap<AreaId, BTreeSet<StableId>>,
    resolved_areas: BTreeMap<AreaId, BTreeSet<StableId>>,
    cells: BTreeMap<AreaId, BTreeSet<StableId>>,
    roads: BTreeMap<(AreaId, AreaId), BTreeSet<StableId>>,
    incident_roads: BTreeMap<AreaId, BTreeSet<StableId>>,
    resolved: BTreeMap<StableId, Location>,
}
impl<'a> LocalRoster<'a> {
    pub fn build(store: &'a Store) -> Result<Self, Error> {
        let resolved = resolve_locations(locations(store))?;
        let mut roster = Self {
            store,
            raw_areas: BTreeMap::new(),
            resolved_areas: BTreeMap::new(),
            cells: BTreeMap::new(),
            roads: BTreeMap::new(),
            incident_roads: BTreeMap::new(),
            resolved,
        };
        for (actor, location) in locations(store) {
            if let Location::AtArea(area) = location {
                roster.raw_areas.entry(*area).or_default().insert(actor);
            }
        }
        for (actor, location) in &roster.resolved {
            if let Some(cell) = location.cell() {
                roster.cells.entry(cell).or_default().insert(*actor);
            }
            match location {
                Location::AtArea(area) => {
                    roster
                        .resolved_areas
                        .entry(*area)
                        .or_default()
                        .insert(*actor);
                }
                Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. } => {
                    roster
                        .roads
                        .entry(road_key(*from, *to))
                        .or_default()
                        .insert(*actor);
                    roster
                        .incident_roads
                        .entry(*from)
                        .or_default()
                        .insert(*actor);
                    roster.incident_roads.entry(*to).or_default().insert(*actor);
                }
                Location::Carried { .. } => unreachable!("resolved canonical location"),
            }
        }
        Ok(roster)
    }
    /// Strict stationary identities; carried actors are deliberately excluded.
    pub fn raw_at_area(&self, area: AreaId) -> impl Iterator<Item = StableId> + '_ {
        self.raw_areas.get(&area).into_iter().flatten().copied()
    }
    /// Actual anchor placement at an area; carried people share this roster.
    pub fn resolved_at_area(&self, area: AreaId) -> impl Iterator<Item = StableId> + '_ {
        self.resolved_areas
            .get(&area)
            .into_iter()
            .flatten()
            .copied()
    }
    /// Coarse weather/scheduling locality; this may include roads and carriers.
    pub fn cell(&self, area: AreaId) -> impl Iterator<Item = StableId> + '_ {
        self.cells.get(&area).into_iter().flatten().copied()
    }
    pub fn on_road(&self, from: AreaId, to: AreaId) -> impl Iterator<Item = StableId> + '_ {
        self.roads
            .get(&road_key(from, to))
            .into_iter()
            .flatten()
            .copied()
    }
    pub fn resolved_location(&self, actor: StableId) -> Option<&Location> {
        self.resolved.get(&actor)
    }
    pub fn raw_location(&self, actor: StableId) -> Option<&Location> {
        location(self.store, actor)
    }
    pub fn indexed(&self) -> usize {
        self.resolved.len()
    }
    /// Capture candidate identities for a declared phase that permits no
    /// Location value changes. Drop this capture before any spatial mutation.
    /// Current contact and eligibility still come from the receiving owner.
    pub fn into_candidates(self) -> LocalCandidates {
        LocalCandidates {
            areas: self.resolved_areas,
            roads: self.roads,
            incident_roads: self.incident_roads,
        }
    }
    /// Candidate IDs only. The movement owner must still compare exact road
    /// fractions at its actual timestamp, including reversed and endpoint rows.
    pub fn position_candidates(&self, actor: StableId) -> Result<Vec<StableId>, Error> {
        let location = self
            .resolved_location(actor)
            .ok_or_else(|| Error::InvalidSave("missing spatial roster actor".into()))?;
        let mut candidates = BTreeSet::new();
        match location {
            Location::AtArea(area) => {
                candidates.extend(self.resolved_at_area(*area));
                candidates.extend(self.incident_roads.get(area).into_iter().flatten().copied());
            }
            Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. } => {
                candidates.extend(self.on_road(*from, *to));
                candidates.extend(self.resolved_at_area(*from));
                candidates.extend(self.resolved_at_area(*to));
            }
            Location::Carried { .. } => unreachable!("resolved canonical location"),
        }
        Ok(candidates.into_iter().collect())
    }
}

/// Shared per-cell identities, rather than one crowded-cell copy per actor.
/// This is an explicit phase capture, with no persistent invalidation claim.
pub struct LocalCandidates {
    areas: BTreeMap<AreaId, BTreeSet<StableId>>,
    roads: BTreeMap<(AreaId, AreaId), BTreeSet<StableId>>,
    incident_roads: BTreeMap<AreaId, BTreeSet<StableId>>,
}
impl LocalCandidates {
    pub fn at_location(&self, location: &Location) -> CandidateIds<'_> {
        let mut lists = Vec::new();
        match location {
            Location::AtArea(area) => {
                lists.extend(self.areas.get(area).map(BTreeSet::iter));
                lists.extend(self.incident_roads.get(area).map(BTreeSet::iter));
            }
            Location::InTransit { from, to, .. } | Location::StoppedOnRoad { from, to, .. } => {
                lists.extend(self.roads.get(&road_key(*from, *to)).map(BTreeSet::iter));
                lists.extend(self.areas.get(from).map(BTreeSet::iter));
                lists.extend(self.areas.get(to).map(BTreeSet::iter));
            }
            Location::Carried { .. } => {}
        }
        CandidateIds {
            lists: lists.into_iter().map(Iterator::peekable).collect(),
        }
    }
}
pub struct CandidateIds<'a> {
    lists: Vec<std::iter::Peekable<std::collections::btree_set::Iter<'a, StableId>>>,
}
impl Iterator for CandidateIds<'_> {
    type Item = StableId;
    fn next(&mut self) -> Option<Self::Item> {
        let next = self
            .lists
            .iter_mut()
            .filter_map(|list| list.peek().map(|id| **id))
            .min()?;
        for list in &mut self.lists {
            if list.peek().is_some_and(|id| **id == next) {
                list.next();
            }
        }
        Some(next)
    }
}
fn road_key(from: AreaId, to: AreaId) -> (AreaId, AreaId) {
    (from.min(to), from.max(to))
}
#[cfg(test)]
mod tests;

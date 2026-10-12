//! Checked directed-area navigation over plain authored or remembered roads.
use game_core::{AreaId, Error, SimDuration};
use std::collections::{BTreeMap, BTreeSet};
pub type RoadGraph = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
pub fn shortest_path(
    graph: &RoadGraph,
    origin: AreaId,
    destination: AreaId,
) -> Result<(Vec<AreaId>, SimDuration), Error> {
    if origin.0 == 0 || destination.0 == 0 {
        return Err(Error::InvalidConfiguration("zero navigation area".into()));
    }
    let mut frontier = BTreeSet::from([(0_u64, Vec::<AreaId>::new(), origin)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, path, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == destination {
            return Ok((path, SimDuration(cost)));
        }
        for (to, duration) in graph.get(&area).into_iter().flatten() {
            if to.0 == 0 || duration.0 == 0 {
                return Err(Error::InvalidConfiguration(
                    "invalid navigation road".into(),
                ));
            }
            let mut next = path.clone();
            next.push(*to);
            frontier.insert((
                cost.checked_add(duration.0).ok_or(Error::Overflow)?,
                next,
                *to,
            ));
        }
    }
    Err(Error::NoRoute)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directed_paths_use_actual_cost_lexical_ties_and_checked_arithmetic() {
        let roads = RoadGraph::from([
            (
                AreaId(1),
                BTreeMap::from([(AreaId(2), SimDuration(10)), (AreaId(3), SimDuration(10))]),
            ),
            (AreaId(2), BTreeMap::from([(AreaId(4), SimDuration(10))])),
            (AreaId(3), BTreeMap::from([(AreaId(4), SimDuration(10))])),
        ]);
        assert_eq!(
            shortest_path(&roads, AreaId(1), AreaId(4)).unwrap(),
            (vec![AreaId(2), AreaId(4)], SimDuration(20))
        );
        assert_eq!(
            shortest_path(&roads, AreaId(4), AreaId(1)),
            Err(Error::NoRoute)
        );
        let overflow = RoadGraph::from([
            (
                AreaId(1),
                BTreeMap::from([(AreaId(2), SimDuration(u64::MAX))]),
            ),
            (AreaId(2), BTreeMap::from([(AreaId(3), SimDuration(1))])),
        ]);
        assert_eq!(
            shortest_path(&overflow, AreaId(1), AreaId(3)),
            Err(Error::Overflow)
        );
    }
    #[test]
    fn zero_areas_or_travel_durations_never_produce_navigation_intent() {
        let graph = RoadGraph::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(0))]))]);
        assert!(shortest_path(&graph, AreaId(1), AreaId(2)).is_err());
        assert!(shortest_path(&RoadGraph::new(), AreaId(0), AreaId(0)).is_err());
        assert_eq!(
            shortest_path(&RoadGraph::new(), AreaId(1), AreaId(1)).unwrap(),
            (vec![], SimDuration(0))
        );
    }
}

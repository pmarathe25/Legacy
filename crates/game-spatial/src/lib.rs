//! Deterministic shortest paths over the strategic area graph.
mod cache;
pub use cache::Routes;
use game_core::*;
use game_land::{Area, Land};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub areas: std::sync::Arc<[AreaId]>,
    pub duration: SimDuration,
}
pub fn route(land: &Land, from: AreaId, to: AreaId) -> Result<Route> {
    require(
        land.0.contains_key(&from) && land.0.contains_key(&to),
        "unknown area",
    )?;
    route_with(from, to, |id| land.0.get(&id).map(Cow::Borrowed))
}
/// A read view may expose only known outgoing roads. Reaching a known endpoint
/// does not require knowing that destination's outgoing roads.
pub fn route_with<'a>(
    from: AreaId,
    to: AreaId,
    mut area_at: impl FnMut(AreaId) -> Option<Cow<'a, Area>>,
) -> Result<Route> {
    if from == to {
        return Ok(Route {
            areas: vec![from].into(),
            duration: SimDuration(0),
        });
    }
    let mut costs = BTreeMap::from([(from, 0u64)]);
    let mut parents = BTreeMap::new();
    let mut queue = BTreeSet::from([(0u64, from)]);
    while let Some((cost, area)) = queue.pop_first() {
        if area == to {
            let mut path = vec![to];
            let mut p = to;
            while p != from {
                p = parents[&p];
                path.push(p);
            }
            path.reverse();
            return Ok(Route {
                areas: path.into(),
                duration: SimDuration(cost),
            });
        }
        if costs[&area] != cost {
            continue;
        }
        let Some(known) = area_at(area) else { continue };
        for (next, duration) in &known.links {
            let sum = cost.checked_add(duration.0).ok_or(Error::Overflow)?;
            if costs.get(next).is_none_or(|old| sum < *old) {
                costs.insert(*next, sum);
                parents.insert(*next, area);
                queue.insert((sum, *next));
            }
        }
    }
    Err(Error::Rejected("no route".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use game_land::Area;
    #[test]
    fn shortest_path_is_stable_and_unreachable_fails() {
        let land = Land(
            BTreeMap::from([
                (
                    AreaId(1),
                    Area {
                        name: "a".into(),
                        links: BTreeMap::from([
                            (AreaId(2), SimDuration(4)),
                            (AreaId(3), SimDuration(2)),
                        ]),
                    },
                ),
                (
                    AreaId(2),
                    Area {
                        name: "b".into(),
                        links: BTreeMap::new(),
                    },
                ),
                (
                    AreaId(3),
                    Area {
                        name: "c".into(),
                        links: BTreeMap::from([(AreaId(2), SimDuration(1))]),
                    },
                ),
            ])
            .into(),
        );
        assert_eq!(
            route(&land, AreaId(1), AreaId(2)).unwrap(),
            Route {
                areas: vec![AreaId(1), AreaId(3), AreaId(2)].into(),
                duration: SimDuration(3)
            }
        );
        assert!(route(&land, AreaId(2), AreaId(1)).is_err());
    }
}

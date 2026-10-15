//! Deterministic shortest paths over the strategic area graph.
use game_core::*;
use game_land::Land;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub areas: Vec<AreaId>,
    pub duration: SimDuration,
}
pub fn route(land: &Land, from: AreaId, to: AreaId) -> Result<Route> {
    require(
        land.0.contains_key(&from) && land.0.contains_key(&to),
        "unknown area",
    )?;
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
                areas: path,
                duration: SimDuration(cost),
            });
        }
        if costs[&area] != cost {
            continue;
        }
        for (next, duration) in &land.0[&area].links {
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
        let land = Land(BTreeMap::from([
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
        ]));
        assert_eq!(
            route(&land, AreaId(1), AreaId(2)).unwrap(),
            Route {
                areas: vec![AreaId(1), AreaId(3), AreaId(2)],
                duration: SimDuration(3)
            }
        );
        assert!(route(&land, AreaId(2), AreaId(1)).is_err());
    }
}

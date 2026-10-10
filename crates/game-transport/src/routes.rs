//! Strategic movement is selected from a person's remembered directional roads.
//! The returned intent still requires the ordinary timed physical executor.
use crate::{Action, Leg, Position, RoadPosition};
use game_core::{AreaId, Error, Location, SimDuration, SimTime};
use std::collections::{BTreeMap, BTreeSet};

pub type KnownRoutes = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteIntent {
    MoveArea { destination: AreaId },
    Transport(Action),
}

/// The target is a frozen area or road observation, never a live remote entity.
/// Moving actors must finish or explicitly stop before changing delivery legs.
pub fn route_intent(
    current: &Location,
    target: &Location,
    now: SimTime,
    routes: &KnownRoutes,
) -> Result<RouteIntent, Error> {
    let current = stationary(current, now)?;
    let target = stationary(target, now)?;
    if current.same_point(target) {
        return Err(Error::AlreadyThere);
    }
    if let Position::Road(point) = target
        && routes
            .get(&point.from)
            .and_then(|links| links.get(&point.to))
            .is_none_or(|duration| *duration != point.duration)
    {
        return Err(Error::NoRoute);
    }
    match current {
        Position::Road(point) => {
            if let Position::Road(target) = target
                && (target.from == point.from && target.to == point.to
                    || target.from == point.to && target.to == point.from)
            {
                let direction = if u128::from(point.progress.0) * u128::from(target.duration.0)
                    < u128::from(if point.from == target.from {
                        target.progress.0
                    } else {
                        target.duration.0 - target.progress.0
                    }) * u128::from(point.duration.0)
                {
                    (point.from, point.to)
                } else {
                    (point.to, point.from)
                };
                if routes
                    .get(&direction.0)
                    .and_then(|links| links.get(&direction.1))
                    .is_none_or(|duration| duration.0 == 0)
                {
                    return Err(Error::NoRoute);
                }
                return Ok(RouteIntent::Transport(approach(target)));
            }
            let endpoint = [point.from, point.to]
                .into_iter()
                .filter_map(|endpoint| {
                    Some((
                        return_cost(routes, point, endpoint)?
                            + target_cost(routes, endpoint, target)?,
                        endpoint,
                    ))
                })
                .min()
                .ok_or(Error::NoRoute)?
                .1;
            Ok(RouteIntent::Transport(Action::ReturnToArea {
                destination: endpoint,
            }))
        }
        Position::Area(area) => match target {
            Position::Area(destination) => {
                area_cost(routes, area, destination).ok_or(Error::NoRoute)?;
                Ok(RouteIntent::MoveArea { destination })
            }
            Position::Road(point) => {
                if area == point.from || area == point.to {
                    let other = if area == point.from {
                        point.to
                    } else {
                        point.from
                    };
                    if routes
                        .get(&area)
                        .and_then(|links| links.get(&other))
                        .is_none_or(|duration| duration.0 == 0)
                    {
                        return Err(Error::NoRoute);
                    }
                    Ok(RouteIntent::Transport(approach(point)))
                } else {
                    let endpoint = [point.from, point.to]
                        .into_iter()
                        .filter_map(|endpoint| {
                            Some((
                                u128::from(area_cost(routes, area, endpoint)?)
                                    + approach_cost(routes, endpoint, point)?,
                                endpoint,
                            ))
                        })
                        .min()
                        .ok_or(Error::NoRoute)?
                        .1;
                    Ok(RouteIntent::MoveArea {
                        destination: endpoint,
                    })
                }
            }
        },
    }
}
fn stationary(location: &Location, now: SimTime) -> Result<Position, Error> {
    match *location {
        Location::AtArea(area) if area.0 > 0 => Ok(Position::Area(area)),
        Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } => {
            let point = RoadPosition {
                from,
                to,
                progress,
                duration,
            };
            if from.0 == 0 || to.0 == 0 || !point.valid() {
                return Err(Error::InvalidConfiguration(
                    "invalid stationary road geometry".into(),
                ));
            }
            Ok(Position::Road(point))
        }
        Location::InTransit {
            from,
            to,
            departed,
            arrives,
            road_start,
        } => {
            Leg {
                from,
                to,
                departed,
                arrives,
                road_start,
            }
            .position(now)?;
            Err(Error::Unavailable)
        }
        _ => Err(Error::Unavailable),
    }
}
fn approach(point: RoadPosition) -> Action {
    Action::ApproachRoad {
        from: point.from,
        to: point.to,
        progress: point.progress,
        duration: point.duration,
    }
}
fn target_cost(routes: &KnownRoutes, origin: AreaId, target: Position) -> Option<u128> {
    match target {
        Position::Area(destination) => Some(u128::from(area_cost(routes, origin, destination)?)),
        Position::Road(point) => [point.from, point.to]
            .into_iter()
            .filter_map(|endpoint| {
                Some(
                    u128::from(area_cost(routes, origin, endpoint)?)
                        + approach_cost(routes, endpoint, point)?,
                )
            })
            .min(),
    }
}
fn return_cost(routes: &KnownRoutes, point: RoadPosition, endpoint: AreaId) -> Option<u128> {
    let (start, distance) = if endpoint == point.from {
        (point.to, point.progress.0)
    } else {
        (point.from, point.duration.0 - point.progress.0)
    };
    fraction(
        routes.get(&start)?.get(&endpoint)?.0,
        distance,
        point.duration.0,
    )
}
fn approach_cost(routes: &KnownRoutes, endpoint: AreaId, point: RoadPosition) -> Option<u128> {
    let (other, distance) = if endpoint == point.from {
        (point.to, point.progress.0)
    } else {
        (point.from, point.duration.0 - point.progress.0)
    };
    fraction(
        routes.get(&endpoint)?.get(&other)?.0,
        distance,
        point.duration.0,
    )
}
fn fraction(duration: u64, distance: u64, basis: u64) -> Option<u128> {
    if duration == 0 || basis == 0 {
        return None;
    }
    Some((u128::from(duration) * u128::from(distance)).div_ceil(u128::from(basis)))
}
fn area_cost(routes: &KnownRoutes, origin: AreaId, target: AreaId) -> Option<u64> {
    let mut frontier = BTreeSet::from([(0u64, Vec::<AreaId>::new(), origin)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, path, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == target {
            return Some(cost);
        }
        for (next, duration) in routes.get(&area).into_iter().flatten() {
            if duration.0 == 0 {
                continue;
            }
            let mut path = path.clone();
            path.push(*next);
            frontier.insert((cost.checked_add(duration.0)?, path, *next));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph(edges: &[(u64, u64, u64)]) -> KnownRoutes {
        let mut graph = KnownRoutes::new();
        for (from, to, duration) in edges {
            graph
                .entry(AreaId(*from))
                .or_default()
                .insert(AreaId(*to), SimDuration(*duration));
        }
        graph
    }
    fn road(from: u64, to: u64, progress: u64, duration: u64) -> Location {
        Location::StoppedOnRoad {
            from: AreaId(from),
            to: AreaId(to),
            progress: SimDuration(progress),
            duration: SimDuration(duration),
        }
    }
    #[test]
    fn stopped_road_return_accounts_for_asymmetric_duration_and_onward_cost() {
        let routes = graph(&[(1, 2, 100), (2, 1, 300), (1, 3, 100), (2, 3, 10)]);
        let current = road(1, 2, 25, 100);
        assert_eq!(
            route_intent(
                &current,
                &Location::AtArea(AreaId(3)),
                SimTime(1_000),
                &routes
            ),
            Ok(RouteIntent::Transport(Action::ReturnToArea {
                destination: AreaId(2)
            }))
        );
        let mut routes = routes;
        routes
            .get_mut(&AreaId(1))
            .unwrap()
            .insert(AreaId(3), SimDuration(5));
        assert_eq!(
            route_intent(
                &current,
                &Location::AtArea(AreaId(3)),
                SimTime(1_000),
                &routes
            ),
            Ok(RouteIntent::Transport(Action::ReturnToArea {
                destination: AreaId(1)
            }))
        );
        let leg = crate::retreat(
            RoadPosition {
                from: AreaId(1),
                to: AreaId(2),
                progress: SimDuration(25),
                duration: SimDuration(100),
            },
            AreaId(1),
            SimDuration(300),
            SimTime(1_000),
        )
        .unwrap();
        assert_eq!(leg.arrives, SimTime(1_075));
        assert_ne!(
            leg.position(SimTime(1_001)).unwrap(),
            Position::Area(AreaId(1))
        );
        assert_eq!(
            leg.position(leg.arrives).unwrap(),
            Position::Area(AreaId(1))
        );
    }
    #[test]
    fn another_midroad_target_is_reached_via_physical_endpoint_stages() {
        let routes = graph(&[
            (1, 2, 100),
            (2, 1, 300),
            (1, 3, 50),
            (2, 4, 10),
            (3, 4, 80),
            (4, 3, 240),
        ]);
        let target = road(3, 4, 60, 80);
        assert_eq!(
            route_intent(&road(1, 2, 25, 100), &target, SimTime(0), &routes),
            Ok(RouteIntent::Transport(Action::ReturnToArea {
                destination: AreaId(2)
            }))
        );
        assert_eq!(
            route_intent(&Location::AtArea(AreaId(2)), &target, SimTime(75), &routes),
            Ok(RouteIntent::MoveArea {
                destination: AreaId(4)
            })
        );
        assert_eq!(
            route_intent(&Location::AtArea(AreaId(4)), &target, SimTime(85), &routes),
            Ok(RouteIntent::Transport(Action::ApproachRoad {
                from: AreaId(3),
                to: AreaId(4),
                progress: SimDuration(60),
                duration: SimDuration(80)
            }))
        );
    }
    #[test]
    fn same_reversed_road_preserves_target_orientation_and_real_reverse_distance() {
        let routes = graph(&[(1, 2, 100), (2, 1, 300)]);
        assert_eq!(
            route_intent(
                &road(1, 2, 30, 100),
                &road(2, 1, 240, 300),
                SimTime(10),
                &routes
            ),
            Ok(RouteIntent::Transport(Action::ApproachRoad {
                from: AreaId(2),
                to: AreaId(1),
                progress: SimDuration(240),
                duration: SimDuration(300)
            }))
        );
        let start = RoadPosition {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(30),
            duration: SimDuration(100),
        };
        let target = RoadPosition {
            from: AreaId(2),
            to: AreaId(1),
            progress: SimDuration(240),
            duration: SimDuration(300),
        };
        let travel = crate::approach_from(
            start,
            target,
            SimDuration(300),
            SimDuration(100),
            SimTime(10),
        )
        .unwrap();
        assert_eq!(travel.1.completes, SimTime(40));
        assert!(
            !travel
                .0
                .position(SimTime(11))
                .unwrap()
                .same_point(Position::Road(target))
        );
        assert!(
            travel
                .0
                .position(travel.1.completes)
                .unwrap()
                .same_point(Position::Road(target))
        );
    }
    #[test]
    fn equal_endpoint_costs_choose_the_stable_area_id() {
        let routes = graph(&[(1, 2, 100), (2, 1, 100), (1, 3, 20), (2, 3, 20)]);
        assert_eq!(
            route_intent(
                &road(1, 2, 50, 100),
                &Location::AtArea(AreaId(3)),
                SimTime(0),
                &routes
            ),
            Ok(RouteIntent::Transport(Action::ReturnToArea {
                destination: AreaId(1)
            }))
        );
    }
    #[test]
    fn unknown_path_and_unusable_geometry_never_emit_a_movement_intent() {
        assert_eq!(
            route_intent(
                &Location::AtArea(AreaId(1)),
                &Location::AtArea(AreaId(2)),
                SimTime(0),
                &KnownRoutes::new()
            ),
            Err(Error::NoRoute)
        );
        assert_eq!(
            route_intent(
                &Location::AtArea(AreaId(1)),
                &road(1, 2, 50, 100),
                SimTime(0),
                &KnownRoutes::new()
            ),
            Err(Error::NoRoute)
        );
        let routes = graph(&[(1, 2, 100)]);
        assert_eq!(
            route_intent(
                &Location::AtArea(AreaId(2)),
                &road(1, 2, 50, 100),
                SimTime(0),
                &routes
            ),
            Err(Error::NoRoute)
        );
        assert!(
            route_intent(
                &road(1, 2, 101, 100),
                &Location::AtArea(AreaId(2)),
                SimTime(0),
                &routes
            )
            .is_err()
        );
        assert!(
            route_intent(
                &Location::AtArea(AreaId(1)),
                &road(1, 2, 0, 0),
                SimTime(0),
                &routes
            )
            .is_err()
        );
        assert_eq!(
            route_intent(
                &Location::AtArea(AreaId(1)),
                &road(1, 2, 50, 200),
                SimTime(0),
                &routes
            ),
            Err(Error::NoRoute)
        );
    }
    #[test]
    fn actual_motion_must_stop_and_targets_remain_frozen() {
        let routes = graph(&[(1, 2, 100)]);
        let moving = Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            departed: SimTime(10),
            arrives: SimTime(110),
            road_start: None,
        };
        assert_eq!(
            route_intent(&moving, &Location::AtArea(AreaId(1)), SimTime(30), &routes),
            Err(Error::Unavailable)
        );
        assert_eq!(
            route_intent(&Location::AtArea(AreaId(1)), &moving, SimTime(30), &routes),
            Err(Error::Unavailable)
        );
    }
}

use crate::*;

const SEARCH_LIMIT: usize = 64;

/// An unfinished exploration leg persists across rounds and save/restore.
/// Neither target identities nor unseen coordinates enter this state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchState {
    pub next_waypoint: u16,
    pub goal: Option<Point>,
}
impl SearchState {
    pub fn valid(&self, map: &Map) -> bool {
        usize::from(self.next_waypoint) < SEARCH_LIMIT
            && self.goal.is_none_or(|point| map.contains(point))
    }
}

fn corners(map: &Map) -> Vec<Point> {
    let mut points = Vec::new();
    for obstacle in map.obstacles.iter().filter(|o| o.integrity > 0) {
        for x in [
            obstacle.minimum.x.checked_sub(1),
            obstacle.maximum.x.checked_add(1),
        ] {
            for y in [
                obstacle.minimum.y.checked_sub(1),
                obstacle.maximum.y.checked_add(1),
            ] {
                if let (Some(x), Some(y)) = (x, y) {
                    let point = Point { x, y };
                    if map.clear(point, point) {
                        points.push(point);
                    }
                }
            }
        }
    }
    points.sort_unstable_by_key(|p| (p.x, p.y));
    points.dedup();
    points
}

fn length(a: Point, b: Point) -> u128 {
    let square = distance(a, b);
    let floor = square.isqrt();
    floor + u128::from(floor * floor < square)
}

impl Map {
    /// A visibility graph over millimetre positions around the observed solid
    /// rectangles. Stable coordinate ordering resolves equal-length detours.
    /// Every returned leg is checked against the actual continuous geometry.
    pub fn route(&self, from: Point, goal: Point) -> Option<Vec<Point>> {
        if !self.clear(from, from) || !self.clear(goal, goal) {
            return None;
        }
        if self.clear(from, goal) {
            return Some(if from == goal {
                vec![from]
            } else {
                vec![from, goal]
            });
        }
        let mut nodes = corners(self);
        nodes.extend([from, goal]);
        nodes.sort_unstable_by_key(|p| (p.x, p.y));
        nodes.dedup();
        let start = nodes.iter().position(|p| *p == from)?;
        let end = nodes.iter().position(|p| *p == goal)?;
        let mut costs = vec![u128::MAX; nodes.len()];
        let mut previous = vec![None; nodes.len()];
        let mut settled = vec![false; nodes.len()];
        costs[start] = 0;
        while let Some(current) = (0..nodes.len())
            .filter(|i| !settled[*i] && costs[*i] < u128::MAX)
            .min_by_key(|i| (costs[*i], *i))
        {
            if current == end {
                let mut route = vec![goal];
                let mut cursor = end;
                while cursor != start {
                    cursor = previous[cursor]?;
                    route.push(nodes[cursor]);
                }
                route.reverse();
                return Some(route);
            }
            settled[current] = true;
            for next in 0..nodes.len() {
                if !settled[next] && self.clear(nodes[current], nodes[next]) {
                    let candidate =
                        costs[current].checked_add(length(nodes[current], nodes[next]))?;
                    if candidate < costs[next] {
                        costs[next] = candidate;
                        previous[next] = Some(current);
                    }
                }
            }
        }
        None
    }

    /// One legal straight movement leg along a detour. Stand-off applies only
    /// on the final unobstructed approach, never before navigating a wall.
    pub fn navigation_step(
        &self,
        from: Point,
        goal: Point,
        budget: u64,
        stand_off: u64,
    ) -> Option<Point> {
        if budget == 0 {
            return None;
        }
        let route = self.route(from, goal)?;
        let next = *route.get(1)?;
        let remaining = length(from, next);
        let available = if route.len() == 2 {
            remaining.saturating_sub(u128::from(stand_off))
        } else {
            remaining
        };
        let step = u128::from(budget).min(available);
        if step == 0 {
            return None;
        }
        let point = Point {
            x: from.x
                + ((i128::from(next.x) - i128::from(from.x)) * step as i128 / remaining as i128)
                    as i64,
            y: from.y
                + ((i128::from(next.y) - i128::from(from.y)) * step as i128 / remaining as i128)
                    as i64,
        };
        (point != from
            && self.clear(from, point)
            && distance(from, point) <= u128::from(budget).pow(2))
        .then_some(point)
    }

    fn search_waypoints(&self) -> Vec<Point> {
        let mut points = Vec::new();
        for x in 0..4 {
            for y in 0..4 {
                let point = Point {
                    x: (i128::from(self.width) * (2 * x + 1) / 8) as i64,
                    y: (i128::from(self.height) * (2 * y + 1) / 8) as i64,
                };
                if self.clear(point, point) {
                    points.push(point);
                }
            }
        }
        let mut extra: Vec<_> = self
            .exits
            .iter()
            .map(|(_, p)| *p)
            .chain(corners(self))
            .collect();
        extra.sort_unstable_by_key(|p| (p.x, p.y));
        extra.dedup();
        for point in extra {
            if points.len() == SEARCH_LIMIT {
                break;
            }
            if self.clear(point, point) && !points.contains(&point) {
                points.push(point);
            }
        }
        points.dedup();
        points
    }

    /// Bounded exploration using only observed geometry and the actor's own
    /// position. An unreachable or newly obstructed leg is skipped safely.
    pub fn search_step(&self, from: Point, budget: u64, state: &mut SearchState) -> Option<Point> {
        if let Some(goal) = state.goal {
            if let Some(step) = self.navigation_step(from, goal, budget, 0) {
                return Some(step);
            }
            state.goal = None;
        }
        let points = self.search_waypoints();
        for _ in 0..points.len() {
            let index = usize::from(state.next_waypoint) % points.len();
            state.next_waypoint = ((index + 1) % points.len()) as u16;
            let goal = points[index];
            if let Some(step) = self.navigation_step(from, goal, budget, 0) {
                state.goal = Some(goal);
                return Some(step);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn map() -> Map {
        Map {
            id: CombatMapId(1),
            version: 1,
            area: AreaId(2),
            location: Location::AtArea(AreaId(2)),
            width: 20_000,
            height: 20_000,
            obstacles: vec![ObstacleView {
                index: 0,
                minimum: Point { x: 9_000, y: 4_000 },
                maximum: Point {
                    x: 11_000,
                    y: 16_000,
                },
                integrity: 10,
            }],
            exits: vec![],
        }
    }
    #[test]
    fn blocked_line_routes_around_a_wall_with_stable_equal_detours() {
        let map = map();
        let from = Point {
            x: 2_000,
            y: 10_000,
        };
        let goal = Point {
            x: 18_000,
            y: 10_000,
        };
        let route = map.route(from, goal).unwrap();
        assert!(route.len() > 2);
        assert!(route[1].y < 4_000); // Equal top/bottom costs choose stable lower coordinates.
        assert!(route.windows(2).all(|leg| map.clear(leg[0], leg[1])));
        let mut position = from;
        for _ in 0..20 {
            if position == goal {
                break;
            }
            let step = map.navigation_step(position, goal, 2_500, 0).unwrap();
            assert!(distance(position, step) <= 2_500u128.pow(2));
            assert!(map.clear(position, step));
            position = step;
        }
        assert_eq!(position, goal);
    }
    #[test]
    fn impossible_and_damaged_walls_change_actual_reachability() {
        let mut map = map();
        map.obstacles[0].minimum.y = 0;
        map.obstacles[0].maximum.y = map.height;
        let from = Point {
            x: 2_000,
            y: 10_000,
        };
        let goal = Point {
            x: 18_000,
            y: 10_000,
        };
        assert!(map.route(from, goal).is_none());
        assert!(map.navigation_step(from, goal, 5_000, 0).is_none());
        map.obstacles[0].integrity = 0;
        assert_eq!(map.route(from, goal), Some(vec![from, goal]));
    }
    #[test]
    fn detour_does_not_apply_attack_standoff_through_a_wall() {
        let mut map = map();
        map.obstacles[0].minimum = Point { x: 9_000, y: 9_000 };
        map.obstacles[0].maximum = Point {
            x: 11_000,
            y: 11_000,
        };
        let from = Point {
            x: 8_999,
            y: 10_000,
        };
        let goal = Point {
            x: 11_001,
            y: 10_000,
        };
        assert!(map.navigation_step(from, goal, 1_000, 5_000).is_some());
        assert!(
            map.navigation_step(
                Point { x: 2_000, y: 2_000 },
                Point { x: 2_100, y: 2_000 },
                1_000,
                500
            )
            .is_none()
        );
    }
    #[test]
    fn route_ties_do_not_depend_on_obstacle_storage_order() {
        let mut map = map();
        map.obstacles.push(ObstacleView {
            index: 1,
            minimum: Point { x: 1_000, y: 1_000 },
            maximum: Point { x: 2_000, y: 2_000 },
            integrity: 1,
        });
        let route = map.route(
            Point {
                x: 3_000,
                y: 10_000,
            },
            Point {
                x: 18_000,
                y: 10_000,
            },
        );
        map.obstacles.reverse();
        assert_eq!(
            route,
            map.route(
                Point {
                    x: 3_000,
                    y: 10_000
                },
                Point {
                    x: 18_000,
                    y: 10_000
                }
            )
        );
    }
    #[test]
    fn exploration_is_bounded_and_an_unfinished_leg_survives_roundtrip() {
        let map = map();
        assert!(map.search_waypoints().len() <= SEARCH_LIMIT);
        let mut state = SearchState::default();
        let from = Point { x: 4_000, y: 4_000 };
        let first = map.search_step(from, 500, &mut state).unwrap();
        let goal = state.goal.unwrap();
        assert!(map.clear(from, first));
        let saved = serde_json::to_vec(&state).unwrap();
        let mut restored: SearchState = serde_json::from_slice(&saved).unwrap();
        assert_eq!(
            map.search_step(first, 500, &mut state),
            map.search_step(first, 500, &mut restored)
        );
        assert_eq!(state.goal, Some(goal));
        assert_eq!(state, restored);
    }
}

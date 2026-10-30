//! Bounded local visibility graph for an individual fighter's pursuit.
use game_parallel_tactics::{Map, Point};
use std::collections::BinaryHeap;

fn distance(a: Point, b: Point) -> u128 {
    let x = u128::from(a.x.abs_diff(b.x));
    let y = u128::from(a.y.abs_diff(b.y));
    (x * x + y * y).isqrt()
}

/// Obstacles beyond this horizon remain physical, but are not graph vertices.
/// This keeps per-fighter planning bounded while letting nearby walls be bypassed.
pub fn waypoint(map: &Map, from: Point, target: Point) -> Option<Point> {
    if map.visible(from, target) {
        return Some(target);
    }
    let mut nearest = BinaryHeap::with_capacity(17);
    for (index, w) in map.obstacles.iter().enumerate() {
        let center = Point {
            x: w.min.x + (w.max.x - w.min.x) / 2,
            y: w.min.y + (w.max.y - w.min.y) / 2,
        };
        nearest.push((
            distance(from, center) + distance(center, target),
            w.min.x,
            w.min.y,
            w.max.x,
            w.max.y,
            index,
        ));
        if nearest.len() > 16 {
            nearest.pop();
        }
    }
    let walls = nearest.into_sorted_vec();
    let mut nodes = vec![from, target];
    for (_, _, _, _, _, index) in walls {
        let wall = &map.obstacles[index];
        for x in [wall.min.x.checked_sub(1), wall.max.x.checked_add(1)]
            .into_iter()
            .flatten()
        {
            for y in [wall.min.y.checked_sub(1), wall.max.y.checked_add(1)]
                .into_iter()
                .flatten()
            {
                let point = Point { x, y };
                if map.open(point) && !nodes.contains(&point) {
                    nodes.push(point);
                }
            }
        }
    }
    let mut costs = vec![u128::MAX; nodes.len()];
    let mut visited = vec![false; nodes.len()];
    let mut next = vec![None; nodes.len()];
    costs[1] = 0;
    for _ in 0..nodes.len() {
        let current = (0..nodes.len())
            .filter(|&i| !visited[i] && costs[i] != u128::MAX)
            .min_by_key(|&i| (costs[i], nodes[i].x, nodes[i].y, i))?;
        if current == 0 {
            return next[0].map(|i| nodes[i]);
        }
        visited[current] = true;
        for other in 0..nodes.len() {
            if visited[other] || !map.visible(nodes[current], nodes[other]) {
                continue;
            }
            let cost = costs[current].saturating_add(distance(nodes[current], nodes[other]));
            if cost < costs[other] {
                costs[other] = cost;
                next[other] = Some(current);
            }
        }
    }
    None
}

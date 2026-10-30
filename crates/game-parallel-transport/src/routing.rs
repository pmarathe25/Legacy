//! Deterministic shortest paths over directed routes.
use crate::Route;
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
pub struct Routing {
    next: HashMap<u64, HashMap<u64, (u64, u32)>>,
}
impl Routing {
    /// Build independent reverse shortest-path tables for the requested destinations.
    pub fn new(routes: &[Route], targets: &[u64]) -> Self {
        let mut incoming: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
        for route in routes {
            incoming
                .entry(route.destination)
                .or_default()
                .push((route.source, route.ticks));
        }
        let mut unique = targets.to_vec();
        unique.par_sort_unstable();
        unique.dedup();
        let next = unique
            .par_iter()
            .map(|&target| (target, reverse_paths(target, &incoming)))
            .collect();
        Self { next }
    }
    pub fn next(&self, source: u64, target: u64) -> Option<u64> {
        self.next_leg(source, target)
            .map(|(destination, _)| destination)
    }
    /// Return the chosen next region and its actual leg duration.
    pub fn next_leg(&self, source: u64, target: u64) -> Option<(u64, u32)> {
        self.next.get(&target)?.get(&source).copied()
    }
}
fn reverse_paths(
    target: u64,
    incoming: &HashMap<u64, Vec<(u64, u32)>>,
) -> HashMap<u64, (u64, u32)> {
    let mut distance = HashMap::from([(target, 0u64)]);
    let mut next = HashMap::new();
    let mut queue = BinaryHeap::from([Reverse((0u64, target))]);
    while let Some(Reverse((cost, node))) = queue.pop() {
        if distance.get(&node) != Some(&cost) {
            continue;
        }
        if let Some(edges) = incoming.get(&node) {
            for &(source, ticks) in edges {
                let Some(candidate) = cost.checked_add(u64::from(ticks)) else {
                    continue;
                };
                match distance.get(&source).copied() {
                    None => {
                        distance.insert(source, candidate);
                        next.insert(source, (node, ticks));
                        queue.push(Reverse((candidate, source)));
                    }
                    Some(old) if candidate < old => {
                        distance.insert(source, candidate);
                        next.insert(source, (node, ticks));
                        queue.push(Reverse((candidate, source)));
                    }
                    Some(old)
                        if candidate == old
                            && next
                                .get(&source)
                                .is_some_and(|&(previous, _)| node < previous) =>
                    {
                        next.insert(source, (node, ticks));
                    }
                    _ => {}
                }
            }
        }
    }
    next
}

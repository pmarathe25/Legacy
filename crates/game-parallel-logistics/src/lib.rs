//! Parallel shipment storage arbitration.
use game_parallel_transport::Route;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shipment {
    pub id: u64,
    pub source: u64,
    pub destination: u64,
    pub food: u32,
    pub ticks_remaining: u32,
}

pub fn advance(
    regions: &mut [game_parallel_people::Region],
    shipments: &mut Vec<Shipment>,
    rules: &game_parallel_people::Rules,
) {
    if shipments.is_empty() {
        return;
    }
    let space: HashMap<u64, u32> = regions
        .par_iter()
        .map(|r| (r.id, rules.food_capacity.saturating_sub(r.food)))
        .collect();
    let destination = shipments[0].destination;
    if shipments.par_iter().all(|s| s.destination == destination)
        && !shipments.par_windows(2).any(|pair| pair[0].id > pair[1].id)
    {
        let available = space.get(&destination).copied().unwrap_or(0);
        let (delivered, completed) = deliver_group(shipments, available);
        if delivered > 0
            && let Some(region) = regions.iter_mut().find(|r| r.id == destination)
        {
            region.food += delivered;
        }
        if completed {
            *shipments = shipments
                .par_iter()
                .filter(|s| s.food > 0)
                .cloned()
                .collect();
        }
        return;
    }
    if !shipments
        .par_iter()
        .any(|s| s.ticks_remaining <= 1 && space.get(&s.destination).is_some_and(|&n| n > 0))
    {
        shipments
            .par_iter_mut()
            .for_each(|s| s.ticks_remaining = s.ticks_remaining.saturating_sub(1));
        if shipments.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            shipments.par_sort_unstable_by_key(|s| s.id);
        }
        return;
    }
    // A destination owns its storage. Its claims are sorted by shipment ID,
    // then independent chunks reserve disjoint slices of its available space.
    if shipments
        .par_windows(2)
        .any(|pair| (pair[0].destination, pair[0].id) > (pair[1].destination, pair[1].id))
    {
        shipments.par_sort_unstable_by_key(|s| (s.destination, s.id));
    }
    let deliveries: HashMap<u64, u32> = shipments
        .par_chunk_by_mut(|a, b| a.destination == b.destination)
        .map(|group| {
            let destination = group[0].destination;
            let available = *space.get(&destination).unwrap_or(&0);
            let chunk_demands: Vec<u64> = group
                .par_chunks_mut(2048)
                .map(|chunk| {
                    let mut total = 0u64;
                    for s in chunk {
                        s.ticks_remaining = s.ticks_remaining.saturating_sub(1);
                        if s.ticks_remaining == 0 {
                            total = total.saturating_add(u64::from(s.food));
                        }
                    }
                    total
                })
                .collect();
            let mut prefix = 0u64;
            let starts: Vec<u64> = chunk_demands
                .into_iter()
                .map(|demand| {
                    let start = prefix;
                    prefix = prefix.saturating_add(demand).min(u64::from(available));
                    start
                })
                .collect();
            group
                .par_chunks_mut(2048)
                .zip(starts.into_par_iter())
                .for_each(|(chunk, mut used)| {
                    for s in chunk {
                        if s.ticks_remaining == 0 {
                            let amount =
                                u64::from(s.food).min(u64::from(available).saturating_sub(used));
                            s.food -= amount as u32;
                            used += amount;
                        }
                    }
                });
            (destination, prefix as u32)
        })
        .collect();
    regions.par_iter_mut().for_each(|region| {
        if let Some(amount) = deliveries.get(&region.id) {
            region.food += amount;
        }
    });
    let mut remaining: Vec<Shipment> = shipments
        .par_iter()
        .filter(|s| s.food > 0)
        .cloned()
        .collect();
    if remaining.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
        remaining.par_sort_unstable_by_key(|s| s.id);
    }
    *shipments = remaining;
}

// The authoritative vector is already in priority order. Chunk summaries and
// capped prefixes reserve disjoint storage without moving waiting cargo.
fn deliver_group(shipments: &mut [Shipment], available: u32) -> (u32, bool) {
    let demands: Vec<u64> = shipments
        .par_chunks_mut(2048)
        .map(|chunk| {
            let mut demand = 0u64;
            for s in chunk {
                s.ticks_remaining = s.ticks_remaining.saturating_sub(1);
                if s.ticks_remaining == 0 {
                    demand = demand.saturating_add(u64::from(s.food));
                }
            }
            demand
        })
        .collect();
    let mut total = 0u64;
    let prefixes: Vec<_> = demands
        .into_iter()
        .map(|demand| {
            let start = total;
            total = total.saturating_add(demand).min(u64::from(available));
            start
        })
        .collect();
    if total == 0 {
        return (0, false);
    }
    let complete = shipments
        .par_chunks_mut(2048)
        .zip(prefixes.into_par_iter())
        .map(|(chunk, mut used)| {
            let mut completed = false;
            for s in chunk {
                if s.ticks_remaining == 0 {
                    let delivered =
                        u64::from(s.food).min(u64::from(available).saturating_sub(used));
                    s.food -= delivered as u32;
                    used += delivered;
                    completed |= s.food == 0;
                }
            }
            completed
        })
        .reduce(|| false, |a, b| a || b);
    (total as u32, complete)
}

pub struct Context<'a> {
    pub regions: &'a mut [game_parallel_people::Region],
    pub routes: &'a [Route],
    pub shipments: &'a mut Vec<Shipment>,
}
impl Context<'_> {
    /// Reserve stock and launch a batch at a tick boundary, or reject it entirely.
    pub fn launch_shipments(&mut self, shipments: &[Shipment]) -> Result<(), String> {
        let regions: HashMap<u64, usize> = self
            .regions
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        if regions.len() != self.regions.len() {
            return Err("duplicate region id".into());
        }
        let mut ids: HashSet<u64> = self.shipments.iter().map(|s| s.id).collect();
        if ids.len() != self.shipments.len() {
            return Err("duplicate shipment id".into());
        }
        let mut totals: HashMap<usize, u64> = HashMap::new();
        for shipment in shipments {
            if !ids.insert(shipment.id) {
                return Err("duplicate shipment id".into());
            }
            let source = *regions.get(&shipment.source).ok_or("unknown source")?;
            if !regions.contains_key(&shipment.destination) {
                return Err("unknown destination".into());
            }
            if !self.routes.iter().any(|r| {
                r.source == shipment.source
                    && r.destination == shipment.destination
                    && r.ticks == shipment.ticks_remaining
            }) {
                return Err("shipment must follow an authored route duration".into());
            }
            if shipment.source == shipment.destination
                || shipment.food == 0
                || shipment.ticks_remaining == 0
            {
                return Err("invalid shipment".into());
            }
            *totals.entry(source).or_default() += u64::from(shipment.food);
        }
        for (&source, &food) in &totals {
            if food > u64::from(self.regions[source].food) {
                return Err("insufficient food".into());
            }
        }
        for (source, food) in totals {
            self.regions[source].food -= food as u32;
        }
        self.shipments.extend_from_slice(shipments);
        self.shipments.sort_unstable_by_key(|s| s.id);
        Ok(())
    }
}

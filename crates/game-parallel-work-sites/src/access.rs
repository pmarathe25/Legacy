//! Deterministic physical allocation after each worker's personal access choice.
use super::{Binding, Facilities, Plan};
use game_parallel_production_model::Start;
use rayon::prelude::*;
use std::collections::HashSet;

/// Claimed sites are sorted by ID; None means several distinct claimants.
pub struct Access<'a> {
    pub classes: &'a [(u64, Option<u64>)],
    pub disregards: &'a [u64],
}

impl Facilities {
    /// Allocate sole-claimant sites, then open sites, then remaining capacity.
    /// Claims only affect actors who personally choose to respect them.
    pub fn prepare_access(
        &self,
        starts: &[Start],
        access: &Access<'_>,
        seed: u64,
        tick: u64,
    ) -> Plan {
        if access.classes.is_empty()
            || starts.is_empty()
            || starts
                .par_iter()
                .all(|start| access.disregards.binary_search(&start.worker).is_ok())
        {
            return self.prepare(starts, seed, tick);
        }
        let mut occupied: Vec<_> = self
            .bindings
            .par_iter()
            .map(|b| (b.site, b.slots))
            .collect();
        occupied.par_sort_unstable_by_key(|row| row.0);
        let occupied: Vec<_> = occupied
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| (group[0].0, group.par_iter().map(|row| row.1).sum::<u64>()))
            .collect();
        let mut owned: Vec<_> = self
            .sites
            .par_iter()
            .filter_map(|site| {
                let class = access
                    .classes
                    .binary_search_by_key(&site.id, |row| row.0)
                    .ok()
                    .map(|i| access.classes[i].1)?;
                let owner = class?;
                let used = occupied
                    .binary_search_by_key(&site.id, |row| row.0)
                    .ok()
                    .map_or(0, |i| occupied[i].1);
                let free = site.capacity - used;
                (free > 0).then_some((site.region, site.kind.as_str(), owner, free, site.id))
            })
            .collect();
        owned.par_sort_unstable();
        let mut candidates: Vec<_> = starts
            .par_iter()
            .filter_map(|start| {
                let requirement = self
                    .requirements
                    .binary_search_by_key(&start.recipe, |r| r.recipe)
                    .ok()
                    .map(|i| &self.requirements[i])?;
                let key = (
                    start.region,
                    requirement.kind.as_str(),
                    start.worker,
                    requirement.slots,
                    0,
                );
                let i = owned.partition_point(|site| *site < key);
                let site = owned.get(i)?;
                if (site.0, site.1, site.2)
                    != (start.region, requirement.kind.as_str(), start.worker)
                {
                    return None;
                }
                Some((
                    start.worker,
                    game_parallel_genetics::random(seed ^ start.worker, start.worker, tick),
                    start.id,
                    Binding {
                        job: start.id,
                        site: site.4,
                        slots: requirement.slots,
                    },
                ))
            })
            .collect();
        candidates.par_sort_unstable_by_key(|row| (row.0, row.1, row.2));
        let owned_bindings: Vec<_> = candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| group[0].3.clone())
            .collect();
        let owned_workers: HashSet<_> = candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| group[0].0)
            .collect();

        let respectful: Vec<_> = starts
            .par_iter()
            .filter(|start| {
                !owned_workers.contains(&start.worker)
                    && access.disregards.binary_search(&start.worker).is_err()
            })
            .copied()
            .collect();
        let open_sites: Vec<_> = self
            .sites
            .par_iter()
            .filter(|site| {
                access
                    .classes
                    .binary_search_by_key(&site.id, |row| row.0)
                    .is_err()
            })
            .cloned()
            .collect();
        let open = Facilities {
            next_id: self.next_id,
            sites: open_sites,
            requirements: self.requirements.clone(),
            bindings: self.bindings.clone(),
        };
        let open_plan = open.prepare(&respectful, seed, tick);
        let mut accepted_workers: HashSet<_> = respectful
            .par_iter()
            .filter(|start| open_plan.permits(start.id))
            .map(|start| start.worker)
            .collect();
        accepted_workers.par_extend(owned_workers);

        let remaining: Vec<_> = starts
            .par_iter()
            .filter(|start| {
                !accepted_workers.contains(&start.worker)
                    && access.disregards.binary_search(&start.worker).is_ok()
            })
            .copied()
            .collect();
        let mut staged = self.clone();
        staged.bindings.par_extend(
            owned_bindings
                .par_iter()
                .cloned()
                .chain(open_plan.bindings.par_iter().cloned()),
        );
        let remaining_plan = staged.prepare(&remaining, seed, tick);
        let mut bindings: Vec<_> = owned_bindings
            .into_par_iter()
            .chain(open_plan.bindings.into_par_iter())
            .chain(remaining_plan.bindings.into_par_iter())
            .collect();
        bindings.par_sort_unstable_by_key(|binding| binding.job);
        Plan { bindings }
    }
}

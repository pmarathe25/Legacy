//! Publish combat collapse together with interrupted physical work and debris.
use game_economy_building::Construction;
use game_economy_combat::Draft as CombatDraft;
use game_economy_work::sites::Facilities;
use game_lot_runtime::{AdvanceDraft, Phase, Runtime};
use rayon::prelude::*;
use std::collections::HashSet;

pub fn apply(
    combat: &mut CombatDraft,
    construction: &mut Construction,
    sites: &mut Facilities,
    physical: &mut AdvanceDraft,
    runtime: &Runtime,
) -> Result<Vec<u64>, String> {
    if combat.state.structure_events.is_empty()
        || !combat
            .state
            .structures
            .par_iter()
            .any(|target| target.collapses_at_limit)
    {
        return Ok(Vec::new());
    }
    let Some(maintenance) = construction.maintenance.as_ref() else {
        return Ok(Vec::new());
    };
    // Site events are ordered by site; only actual strikes can trigger collapse.
    let struck: Vec<_> = combat
        .state
        .structure_events
        .par_chunk_by(|a, b| a.site == b.site)
        .map(|group| group[0].site)
        .collect();
    let mut collapsed: Vec<_> = combat
        .state
        .structures
        .par_iter()
        .filter(|target| {
            target.collapses_at_limit
                && struck.binary_search(&target.site).is_ok()
                && maintenance.is_broken(sites, target.site)
        })
        .map(|target| target.site)
        .collect();
    if collapsed.is_empty() {
        return Ok(collapsed);
    }
    collapsed.par_sort_unstable();
    let staged = physical.work();
    let mut workers: Vec<_> = sites
        .bindings
        .par_iter()
        .filter(|binding| collapsed.binary_search(&binding.site).is_ok())
        .map(|binding| {
            staged
                .binary_search_by_key(&binding.job, |work| work.job.id)
                .ok()
                .map(|i| &staged[i])
                .filter(|work| work.phase == Phase::Working)
                .map(|work| work.job.worker)
                .ok_or("collapsed site has an invalid work binding")
        })
        .collect::<Result<Vec<_>, _>>()?;
    workers.par_sort_unstable();
    workers.dedup();
    runtime.interrupt_workers_in(physical, &workers)?;
    sites.bindings = sites
        .bindings
        .par_iter()
        .filter(|binding| collapsed.binary_search(&binding.site).is_err())
        .cloned()
        .collect();
    let ids: HashSet<_> = collapsed.par_iter().copied().collect();
    sites.destroy_idle_many(&ids);
    if sites.sites.par_iter().any(|site| ids.contains(&site.id)) {
        return Err("collapsed site could not be removed".into());
    }
    construction
        .materials
        .par_iter_mut()
        .filter(|material| ids.contains(&material.site))
        .for_each(|material| material.debris = true);
    if let Some(maintenance) = &mut construction.maintenance {
        maintenance.sync(sites);
    }
    combat
        .state
        .structure_events
        .par_iter_mut()
        .filter(|event| ids.contains(&event.site))
        .for_each(|event| event.collapsed = true);
    Ok(collapsed)
}

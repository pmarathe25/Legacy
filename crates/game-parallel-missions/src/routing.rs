//! Deterministic shortest-route tables grouped by destination.
use crate::{Mission, Phase};
use rayon::prelude::*;

/// Avoid sorting one destination per person when all journeys share endpoints.
pub(crate) fn targets(missions: &[Mission], active_only: bool) -> Vec<u64> {
    let relevant = |mission: &Mission| {
        !active_only || !matches!(mission.phase, Phase::Completed | Phase::Failed)
    };
    let Some(first) = missions.par_iter().find_any(|mission| relevant(mission)) else {
        return Vec::new();
    };
    let pair = (first.origin, first.destination);
    if missions
        .par_iter()
        .all(|mission| !relevant(mission) || (mission.origin, mission.destination) == pair)
    {
        return vec![pair.0, pair.1];
    }
    let mut targets: Vec<_> = missions
        .par_iter()
        .filter(|mission| relevant(mission))
        .flat_map_iter(|mission| [mission.origin, mission.destination])
        .collect();
    targets.par_sort_unstable();
    targets.dedup();
    targets
}

pub use game_parallel_transport::routing::Routing;

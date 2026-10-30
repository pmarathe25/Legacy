//! Bind persistent autonomous reviews to committed physical columns.
use game_parallel_autonomy::{Actor, Autonomy, Path, Situation};
use game_parallel_autonomy_columns::SituationView;
pub use game_parallel_autonomy_columns::{Draft, Runtime};
use game_parallel_fused_data::Block;
use game_parallel_people::Region;
use game_parallel_transport::{Move, Route};
use rayon::prelude::*;
pub struct View<'a> {
    blocks: &'a [Block],
    regions: &'a [Region],
    positions: &'a [(u64, usize, usize)],
    busy: &'a [u64],
    dense: Option<u64>,
}
impl<'a> View<'a> {
    pub fn new(
        blocks: &'a [Block],
        regions: &'a [Region],
        positions: &'a [(u64, usize, usize)],
        busy: &'a [u64],
    ) -> Self {
        let dense = positions.first().and_then(|first| {
            positions.last().and_then(|last| {
                (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
                    .then_some(first.0)
            })
        });
        Self {
            blocks,
            regions,
            positions,
            busy,
            dense,
        }
    }
}
impl SituationView for View<'_> {
    fn read(&self, person: u64) -> Situation {
        let slot = self
            .dense
            .and_then(|first| person.checked_sub(first))
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| i < self.positions.len())
            .or_else(|| {
                if self.dense.is_none() {
                    self.positions.binary_search_by_key(&person, |p| p.0).ok()
                } else {
                    None
                }
            })
            .expect("validated actor identity");
        let (_, block, offset) = self.positions[slot];
        let block = &self.blocks[block];
        Situation {
            person,
            region: block
                .journey
                .is_none()
                .then(|| self.regions[block.region].id),
            health: block.health[offset],
            hunger: block.hunger[offset],
            busy: self.busy.binary_search(&person).is_ok(),
        }
    }
}
pub fn compile(owner: &Autonomy, routes: &[Route], tick: u64) -> Result<Runtime, &'static str> {
    let paths: Vec<_> = routes
        .iter()
        .map(|r| Path {
            source: r.source,
            destination: r.destination,
            ticks: r.ticks,
        })
        .collect();
    Runtime::compile_validated(owner, &paths, tick)
}
/// Reserve everyone who made an autonomous departure decision.
pub fn reserve(draft: Option<&Draft>, actors: &mut Vec<u64>) {
    if let Some(draft) = draft {
        actors.par_extend(draft.decisions.par_iter().map(|decision| decision.person));
    }
}
pub fn requests(draft: &Draft, view: &View<'_>) -> (Vec<Move>, Vec<bool>) {
    let available: Vec<_> = draft
        .decisions
        .par_iter()
        .map(|d| {
            let person = view.read(d.person);
            person.health > 0 && person.region == Some(d.source)
        })
        .collect();
    let moves = draft
        .decisions
        .par_iter()
        .zip(&available)
        .filter_map(|(d, &yes)| {
            yes.then_some(Move {
                person: d.person,
                destination: d.destination,
            })
        })
        .collect();
    (moves, available)
}
pub fn commit(
    runtime: &mut Runtime,
    draft: Draft,
    available: &[bool],
    result: &Result<(), String>,
    template: &mut Autonomy,
) {
    runtime
        .commit(draft, available, result)
        .expect("validated autonomous generation");
    template.actors.clear();
    template.events.clear();
}
pub fn enroll(runtime: &mut Runtime, births: &[u64], tick: u64, adult_age: u64) {
    let actors: Vec<_> = births
        .par_iter()
        .map(|&person| Actor {
            person,
            enabled: true,
            next_review: tick.saturating_add(adult_age),
            travel_caution: 0,
            hunger_urgency: 0,
        })
        .collect();
    runtime
        .enroll(&actors)
        .expect("validated newborn actor identities");
}

/// Publish reviewed departures through the physical host after its health commit.
#[macro_export]
macro_rules! publish {
    ($kernel:ident, $draft:expr) => {{
        if let Some(autonomy) = $draft {
            let (requests, available) = game_parallel_fused_autonomy::requests(
                &autonomy,
                &game_parallel_fused_autonomy::View::new(
                    &$kernel.blocks,
                    &$kernel.template.regions,
                    &$kernel.positions,
                    &[],
                ),
            );
            let result = if requests.is_empty() {
                Ok(())
            } else {
                $kernel.move_people_autonomous(&requests)
            };
            game_parallel_fused_autonomy::commit(
                $kernel
                    .owners
                    .autonomy
                    .as_mut()
                    .expect("enabled autonomy runtime"),
                autonomy,
                &available,
                &result,
                $kernel
                    .template
                    .autonomy
                    .as_mut()
                    .expect("enabled autonomy"),
            );
        }
    }};
}

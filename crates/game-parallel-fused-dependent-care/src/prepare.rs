//! Prepare dependent-care reservations from the committed family and people views.
use game_parallel::World;
use game_parallel_family_lifecycle::Runtime;
use game_parallel_fused_data::{Block, NONE};
use game_parallel_production_model::PersonSnapshot;
use rayon::prelude::*;
pub fn prepare(
    world: &World,
    family: Option<&Runtime>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    people: &[PersonSnapshot],
    reserved: &[u64],
    tick: u64,
) -> Result<Option<game_parallel_dependent_care::Draft>, &'static str> {
    let Some(owner) = &world.caregiving else {
        return Ok(None);
    };
    let family = family.ok_or("caregiving requires family runtime")?;
    let mut busy: Vec<_> = blocks
        .par_iter()
        .flat_map_iter(|block| {
            block
                .ids
                .iter()
                .zip(&block.work)
                .filter_map(|(&id, &work)| (work != NONE).then_some(id))
        })
        .collect();
    busy.extend_from_slice(reserved);
    let opinions = world
        .opinions
        .as_ref()
        .filter(|_| owner.rules.pressure_threshold.is_some())
        .map(|owner| owner.index());
    Ok(Some(owner.prepare(
        &family.store,
        family.rules.adult_age,
        people,
        &busy,
        (world.seed, tick),
        (
            |a, b| {
                let a = &blocks[positions[a].1];
                let b = &blocks[positions[b].1];
                match (a.journey, b.journey) {
                    (None, None) => a.region == b.region,
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                }
            },
            |parent, child| {
                opinions.as_ref().map_or(0, |opinions| {
                    opinions
                        .fear(parent, child)
                        .saturating_add(opinions.obligation(parent, child))
                })
            },
        ),
    )))
}

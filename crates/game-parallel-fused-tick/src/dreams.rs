//! Stage personal aspiration reviews after final health and lifecycle outcomes.
use game_parallel::{Dreams, Goals, World};
use game_parallel_fused_data::Block;
pub fn prepare(
    world: &World,
    staged: Option<Goals>,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    tick: u64,
) -> Result<(Option<Dreams>, Option<Goals>), &'static str> {
    let Some(dreams) = &world.dreams else {
        return Ok((None, staged));
    };
    let base = staged
        .as_ref()
        .or(world.goals.as_ref())
        .ok_or("aspirations require goals")?;
    let dense = positions.first().and_then(|first| {
        (positions.last()?.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
            .then_some(first.0)
    });
    let draft = dreams.prepare(base, tick, |person| {
        let index = dense
            .and_then(|first| person.checked_sub(first))
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&i| i < positions.len())
            .or_else(|| {
                if dense.is_none() {
                    positions.binary_search_by_key(&person, |row| row.0).ok()
                } else {
                    None
                }
            });
        index.is_some_and(|i| {
            let (_, block, slot) = positions[i];
            blocks[block].next_health[slot] > 0
        })
    })?;
    let Some(draft) = draft else {
        return Ok((None, staged));
    };
    let goals = match draft.goals {
        Some(mut goals) => {
            if staged.is_none() {
                goals.events.clear();
            }
            Some(goals)
        }
        None => staged,
    };
    Ok((Some(draft.state), goals))
}

//! Read committed physical columns directly for persistent knowledge preparation.
use game_parallel::Region;
use game_parallel_fused_data::Block;
use game_parallel_knowledge::Observation;
use game_parallel_knowledge_columns::{ObservationView, Runtime};
pub struct Context<'a> {
    pub autonomy: Option<&'a mut game_parallel_fused_autonomy::Runtime>,
    pub social: Option<&'a mut game_parallel_social_edges::Runtime>,
    pub runtime: Option<&'a mut Runtime>,
    pub positions: &'a [(u64, usize, usize)],
}
pub struct View<'a> {
    blocks: &'a [Block],
    regions: &'a [Region],
    food: &'a [u32],
    positions: &'a [(u64, usize, usize)],
    dense: Option<u64>,
}
impl<'a> View<'a> {
    pub fn new(
        blocks: &'a [Block],
        regions: &'a [Region],
        food: &'a [u32],
        positions: &'a [(u64, usize, usize)],
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
            food,
            positions,
            dense,
        }
    }
}
impl ObservationView for View<'_> {
    fn read(&self, person: u64) -> Observation {
        let slot = self
            .dense
            .and_then(|base| person.checked_sub(base))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < self.positions.len())
            .or_else(|| {
                if self.dense.is_none() {
                    self.positions.binary_search_by_key(&person, |p| p.0).ok()
                } else {
                    None
                }
            })
            .expect("validated mind identity");
        let (_, block, offset) = self.positions[slot];
        let block = &self.blocks[block];
        Observation {
            person,
            health: block.health[offset],
            local: block
                .journey
                .is_none()
                .then(|| (self.regions[block.region].id, self.food[block.region])),
        }
    }
}

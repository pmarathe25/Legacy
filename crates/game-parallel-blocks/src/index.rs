//! Maintain the shared snapshot using the block-index owner.
use super::Engine;
impl Engine {
    pub(super) fn rebuild_people(&mut self) {
        (self.people, self.positions) =
            game_parallel_block_index::rebuild(&self.blocks, &self.template.regions);
        if let Some(packed) = &mut self.packed_goods {
            packed.extend_people(&self.people);
        }
    }
    pub(super) fn refresh_people(&mut self) {
        game_parallel_block_index::refresh(
            &self.blocks,
            &self.template.regions,
            &mut self.people,
            &self.positions,
        );
    }
}

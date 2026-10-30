use super::Kernel;
use game_parallel::{Move, Shipment};
impl Kernel {
    /// Atomically launch a batch of food shipments.
    pub fn launch_shipments(&mut self, requests: &[Shipment]) -> Result<(), String> {
        if requests.is_empty() {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        game_parallel_block_logistics::launch(&mut self.template, &mut self.food, requests)?;
        self.revision = revision;
        Ok(())
    }
    /// Atomically start an entire batch of journeys.
    pub fn move_people(&mut self, moves: &[Move]) -> Result<(), String> {
        game_parallel_fused_departure::check_player_move(&self.template, moves)?;
        self.move_people_autonomous(moves)
    }
    pub(super) fn move_people_autonomous(&mut self, moves: &[Move]) -> Result<(), String> {
        if moves.is_empty() {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        let packed = if let Some(cargo) = &self.template.cargo
            && cargo.auto_pack_travel
        {
            game_parallel_block_food_cargo::prepare_pack(
                cargo,
                &self.template.regions,
                &self.food,
                &self.people(),
                &self.template.routes,
                &self.template.rules,
                moves,
            )?
        } else {
            None
        };
        game_parallel_fused_departure::move_people(
            &mut self.blocks,
            &self.template,
            &self.jobs,
            moves,
        )?;
        if let Some(packed) = packed {
            self.template.cargo = Some(packed.cargo);
            self.food = packed.food;
        }
        self.positions = game_parallel_fused_import::positions(&self.blocks);
        self.revision = revision;
        Ok(())
    }
}

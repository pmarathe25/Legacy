use super::Engine;
use game_parallel::Move;
use rayon::prelude::*;
impl Engine {
    pub fn move_people(&mut self, moves: &[Move]) -> Result<(), String> {
        game_parallel_block_departure::check_player_move(&self.template, moves)?;
        self.move_people_autonomous(moves)
    }
    pub(super) fn move_people_autonomous(&mut self, moves: &[Move]) -> Result<(), String> {
        if let Some(production) = &self.template.production {
            let mut busy =
                game_parallel_block_production::busy(production, self.packed_jobs.as_ref());
            busy.par_sort_unstable();
            if moves
                .par_iter()
                .any(|request| busy.binary_search(&request.person).is_ok())
            {
                return Err("worker has unfinished work".into());
            }
        }
        let packed = if let Some(cargo) = &self.template.cargo {
            game_parallel_block_food_cargo::prepare_pack(
                cargo,
                &self.template.regions,
                &self.food,
                &self.people,
                &self.template.routes,
                &self.template.rules,
                moves,
            )?
        } else {
            None
        };
        game_parallel_block_departure::move_people(&mut self.blocks, &self.template, moves)?;
        if let Some(packed) = packed {
            self.template.cargo = Some(packed.cargo);
            self.food = packed.food;
        }
        if game_parallel_block_import::needs_people(&self.template) {
            self.rebuild_people();
        }
        Ok(())
    }
}

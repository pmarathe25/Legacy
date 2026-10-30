use super::Kernel;
use game_parallel::{CombatCommand, CombatEncounter, TacticalBattle, TacticalMove};
use game_parallel_fused_conflict::command::Command;
impl Kernel {
    game_parallel_fused_conflict::host_methods!();
    fn conflict_command(&mut self, command: Command<'_>) -> Result<u64, String> {
        game_parallel_fused_conflict::command::execute(
            &mut self.template,
            &self.blocks,
            &self.positions,
            self.owners.conflict.as_mut(),
            &mut self.revision,
            command,
        )
    }
}

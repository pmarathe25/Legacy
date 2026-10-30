//! Checked personal property actions.
use super::Kernel;
pub use game_parallel_block_property::fused::Command as PropertyCommand;
impl Kernel {
    pub fn property_command(&mut self, command: PropertyCommand) -> Result<Option<u64>, String> {
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        game_parallel_fused_conflict::check(&self.template, &[command.actor()])?;
        let people = self.people();
        let outcome =
            game_parallel_block_property::fused::execute(&mut self.template, &people, command)?;
        self.revision = revision;
        Ok(outcome)
    }
}

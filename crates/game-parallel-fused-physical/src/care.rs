//! Checked medical treatment requests.
use super::Kernel;
use game_parallel::CareStart;
impl Kernel {
    pub fn start_care(&mut self, requests: &[CareStart]) -> Result<(), String> {
        if requests.is_empty() {
            return Ok(());
        }
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        game_parallel_fused_care::start(
            &mut self.template,
            &self.blocks,
            &self.positions,
            requests,
        )?;
        self.revision = revision;
        Ok(())
    }
}

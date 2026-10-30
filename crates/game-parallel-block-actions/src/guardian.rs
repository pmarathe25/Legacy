use crate::Context;
use game_parallel::ReleaseDecision;
use game_parallel_production_model::person_index;
impl Context<'_> {
    pub fn request_guardian_release(&mut self, guardian: u64) -> Result<ReleaseDecision, String> {
        if let Some(invocation) = self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.invocation.as_ref())
        {
            invocation.require_control(self.world.tick)?;
        }
        let target = self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.current)
            .map(|id| {
                let region = person_index(self.people, id).and_then(|index| self.people[index].1);
                (id, region)
            });
        self.world
            .guardians
            .as_mut()
            .ok_or("guardians are disabled")?
            .request_release(guardian, target)
    }
}

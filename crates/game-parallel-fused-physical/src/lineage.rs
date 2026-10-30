//! Checked invocation and guardian actions.
use super::Kernel;
use game_parallel::{KnowledgeRecord, ReleaseDecision};
impl Kernel {
    game_parallel_family_host::partnership_preference_method!();
    fn lineage_action<R>(
        &mut self,
        action: impl FnOnce(&mut game_parallel_block_actions::Context<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        let people = self.people();
        let result = action(&mut game_parallel_block_actions::Context {
            world: &mut self.template,
            people: &people,
        })?;
        self.revision = revision;
        Ok(result)
    }
    pub fn invoke_lives(&mut self, lives: &[u64]) -> Result<(), String> {
        self.lineage_action(|ctx| ctx.invoke(lives))
    }
    pub fn dismiss_invocation(&mut self) -> Result<(), String> {
        self.lineage_action(|ctx| ctx.dismiss_invocation())
    }
    pub fn accept_invocation_warning(&mut self, revision: u64) -> Result<(), String> {
        self.lineage_action(|ctx| ctx.accept_invocation_warning(revision))
    }
    pub fn request_guardian_release(&mut self, guardian: u64) -> Result<ReleaseDecision, String> {
        self.lineage_action(|ctx| ctx.request_guardian_release(guardian))
    }
    pub fn borrowed_memories(&mut self) -> Result<Vec<(u64, KnowledgeRecord)>, String> {
        self.owners.borrowed_memories(&self.template)
    }
    /// A person can change their reproductive intention without the partner's permission.
    pub fn set_reproduction(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        self.edit_world(|world| {
            if world
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(world.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let family = world.families.as_mut().ok_or("families are disabled")?;
            let member = family
                .members
                .iter_mut()
                .find(|member| member.id == person && member.alive)
                .ok_or("person must be alive")?;
            member.reproduction_allowed = enabled;
            Ok(())
        })
    }
    /// Either person may leave a partnership, including during a pregnancy.
    pub fn leave_partnership(&mut self, person: u64) -> Result<(), String> {
        self.edit_world(|world| {
            if world
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(world.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let family = world.families.as_mut().ok_or("families are disabled")?;
            let member = family
                .members
                .iter_mut()
                .find(|member| member.id == person && member.alive)
                .ok_or("person must be alive")?;
            let partner = member.partner.take().ok_or("person has no partner")?;
            family
                .members
                .iter_mut()
                .find(|member| member.id == partner)
                .expect("validated partnership")
                .partner = None;
            Ok(())
        })
    }
}

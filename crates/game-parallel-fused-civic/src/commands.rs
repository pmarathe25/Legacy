//! Checked membership commands attached to the composed kernel.
#[macro_export]
macro_rules! membership_methods {
    () => {
        pub fn vote_membership(
            &mut self,
            body: u64,
            applicant: u64,
            voter: u64,
            yes: bool,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_fused_conflict::check(&self.template, &[voter])?;
            let people = self.people();
            game_parallel_block_institutions::Context::new(&mut self.template, &people)
                .vote_application(body, applicant, voter, yes)?;
            self.revision = revision;
            Ok(())
        }
        pub fn leave_institution(&mut self, body: u64, person: u64) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_fused_conflict::check(&self.template, &[person])?;
            let people = self.people();
            game_parallel_block_institutions::Context::new(&mut self.template, &people)
                .leave(body, person)?;
            self.revision = revision;
            Ok(())
        }
        pub fn apply_membership(&mut self, body: u64, applicant: u64) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_fused_conflict::check(&self.template, &[applicant])?;
            let people = self.people();
            game_parallel_block_institutions::Context::new(&mut self.template, &people)
                .apply(body, applicant)?;
            self.revision = revision;
            Ok(())
        }
        pub fn withdraw_membership(&mut self, body: u64, applicant: u64) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_fused_conflict::check(&self.template, &[applicant])?;
            let people = self.people();
            game_parallel_block_institutions::Context::new(&mut self.template, &people)
                .withdraw_application(body, applicant)?;
            self.revision = revision;
            Ok(())
        }
    };
}

//! Only the service owner changes a member's explanations and shared reports.
use crate::*;
use game_core::*;
use game_ecs::Store;
pub struct TargetReport {
    pub speaker: PersonId,
    pub target: PersonId,
    pub area: AreaId,
    pub observed_at: SimTime,
}
pub struct TargetReportFacts {
    pub speaker_exists: bool,
    pub target_exists: bool,
    pub area_exists: bool,
    pub now: SimTime,
}
impl Runtime {
    pub fn validate_target_report(
        &self,
        proposal: &MissionProposal,
        target: PersonId,
        observed_at: SimTime,
        facts: &TargetReportFacts,
    ) -> Result<(), Error> {
        if !facts.speaker_exists
            || !facts.target_exists
            || !facts.area_exists
            || proposal.objective.target_person() != Some(target)
            || observed_at > facts.now
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn prepare_target_report(
        &self,
        store: &Store,
        id: MissionId,
        report: TargetReport,
        facts: &TargetReportFacts,
    ) -> Result<PreparedMission, Error> {
        let old = mission(store, id).ok_or(Error::Unavailable)?;
        if old.sponsor != report.speaker {
            return Err(Error::Unavailable);
        }
        self.validate_target_report(&old.proposal, report.target, report.observed_at, facts)?;
        let mut after = old.clone();
        for member in after.members.values_mut() {
            member.contacts.insert(report.target, report.area);
        }
        let mut plan = PreparedMission::default();
        plan.mission(store, after);
        Ok(plan)
    }
    pub fn prepare_explanation(
        &self,
        store: &Store,
        id: MissionId,
        actor: PersonId,
        message: &str,
    ) -> Result<PreparedMission, Error> {
        let mut after = mission(store, id).ok_or(Error::Unavailable)?.clone();
        after
            .members
            .get_mut(&actor)
            .ok_or(Error::Unavailable)?
            .explanation = message.into();
        after.validate()?;
        let mut plan = PreparedMission::default();
        plan.mission(store, after);
        Ok(plan)
    }
}

impl Runtime {
    pub fn army_retreat_objective(
        &self,
        store: &Store,
        id: MissionId,
    ) -> Result<MissionObjective, Error> {
        Ok(MissionObjective::Visit {
            destination: mission(store, id).ok_or(Error::Unavailable)?.proposal.home,
        })
    }
}

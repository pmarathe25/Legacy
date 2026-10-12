//! Local ally selection preserves communicated service and individual judgment.
use crate::*;
use crate::{mission as stored_mission, recruitment as stored_recruitment};
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug)]
pub struct EncounterFacts {
    pub id: EncounterId,
    pub active: bool,
    pub target_present: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct FighterFacts {
    pub player: bool,
    pub at_area: Option<AreaId>,
    pub can_fight: bool,
    pub in_combat: bool,
    pub recruitment: RecruitmentInput,
}
pub trait EngagementFacts {
    fn encounters(&self, target: PersonId) -> impl Iterator<Item = EncounterFacts>;
    fn fighter(&self, mission: MissionId, person: PersonId) -> FighterFacts;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngagementPlan {
    Keep,
    Approach(EncounterId),
    Start(Vec<(PersonId, u32)>),
}
impl Runtime {
    pub fn force_willing(
        &self,
        store: &Store,
        actor: PersonId,
        id: MissionId,
        target: PersonId,
        mut input: RecruitmentInput,
        player: bool,
    ) -> bool {
        let Some(m) = stored_mission(store, id) else {
            return false;
        };
        let Some(member) = m.members.get(&actor) else {
            return false;
        };
        if member.status != CompanionStatus::Accepted
            || !member.directive.requires_force()
            || member.directive.target_person() != Some(target)
            || input.guardian_duty
        {
            return false;
        }
        input.violent = member.directive.requires_force();
        player
            || stored_recruitment(store, member.recruitment)
                .is_some_and(|offer| recruitment_score(input, &offer.compensation) >= 0)
    }
    pub fn engagement_plan(
        &self,
        store: &Store,
        mission: MissionId,
        target: PersonId,
        area: Option<AreaId>,
        linked: &BTreeSet<MissionId>,
        facts: &impl EngagementFacts,
    ) -> EngagementPlan {
        let Some(area) = area else {
            return EngagementPlan::Keep;
        };
        if let Some(encounter) = facts
            .encounters(target)
            .filter(|e| e.active && e.target_present)
            .min_by_key(|e| e.id)
        {
            return EngagementPlan::Approach(encounter.id);
        }
        let allies: BTreeSet<_> = linked
            .iter()
            .filter_map(|id| stored_mission(store, *id).map(|m| (*id, m)))
            .flat_map(|(id, m)| m.members.values().map(move |member| (id, member)))
            .filter_map(|(id, member)| {
                if member.status != CompanionStatus::Accepted
                    || !member.directive.requires_force()
                    || member.directive.target_person() != Some(target)
                {
                    return None;
                }
                let body = facts.fighter(id, member.person);
                (!body.player
                    && body.at_area == Some(area)
                    && body.can_fight
                    && !body.in_combat
                    && (id == mission
                        || self.force_willing(
                            store,
                            member.person,
                            id,
                            target,
                            body.recruitment,
                            body.player,
                        )))
                .then_some(member.person)
            })
            .collect();
        EngagementPlan::Start(
            allies
                .into_iter()
                .map(|p| (p, 0))
                .chain([(target, 1)])
                .collect(),
        )
    }
}
#[cfg(test)]
mod tests;

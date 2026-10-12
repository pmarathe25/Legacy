//! Arrival receipts and communicated alliances qualify a later initiative entry.
use crate::mission as stored_mission;
use crate::*;
use game_conflict::Encounter;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug)]
pub struct ArrivalFacts {
    pub actor: PersonId,
    pub area: AreaId,
    pub at: SimTime,
}
#[derive(Clone, Copy, Debug)]
pub struct ReinforcementBody {
    pub player: bool,
    pub can_fight: bool,
    pub in_combat: bool,
    pub area: Option<AreaId>,
    pub recruitment: RecruitmentInput,
}
pub trait ReinforcementFacts {
    fn arrivals(&self) -> impl Iterator<Item = ArrivalFacts>;
    fn body(&self, mission: MissionId, person: PersonId) -> ReinforcementBody;
    fn encounters(&self) -> impl Iterator<Item = &Encounter>;
    fn linked_missions(&self, mission: MissionId) -> BTreeSet<MissionId>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reinforcement {
    pub encounter: EncounterId,
    pub actor: PersonId,
    pub team: u32,
    pub from: AreaId,
}
impl Runtime {
    pub fn reinforcements(
        &self,
        store: &Store,
        now: SimTime,
        facts: &impl ReinforcementFacts,
    ) -> Vec<Reinforcement> {
        let arrivals: BTreeSet<_> = facts
            .arrivals()
            .filter(|r| r.at == now)
            .map(|r| (r.actor, r.area))
            .collect();
        let mut candidates = Vec::new();
        for m in missions(store).map(|(_, row)| row) {
            for member in m.members.values() {
                if member.status != CompanionStatus::Accepted
                    || self.membership(store, member.person) != Some(m.id)
                {
                    continue;
                }
                let body = facts.body(m.id, member.person);
                if body.player || !body.can_fight || body.in_combat {
                    continue;
                }
                let Some(from) = member.last_leg_from else {
                    continue;
                };
                let Some(area) = body.area else {
                    continue;
                };
                if !arrivals.contains(&(member.person, area)) {
                    continue;
                }
                let linked = facts.linked_missions(m.id);
                // Encounter and fighter IDs explicitly preserve the old BTreeMap order.
                let found = facts
                    .encounters()
                    .filter(|e| e.active && e.area == area)
                    .filter_map(|encounter| {
                        let ally = encounter
                            .fighters
                            .values()
                            .filter(|f| !f.withdrawn && !f.surrendered)
                            .filter(|f| {
                                linked.iter().any(|id| {
                                    stored_mission(store, *id)
                                        .and_then(|m| m.members.get(&f.actor))
                                        .is_some_and(|p| {
                                            p.status == CompanionStatus::Accepted
                                                && (*id == m.id
                                                    || member
                                                        .directive
                                                        .target_person()
                                                        .is_some_and(|target| {
                                                            self.force_willing(
                                                                store,
                                                                member.person,
                                                                m.id,
                                                                target,
                                                                body.recruitment,
                                                                body.player,
                                                            ) && encounter
                                                                .fighters
                                                                .get(&target)
                                                                .is_some_and(|enemy| {
                                                                    enemy.team != f.team
                                                                })
                                                        }))
                                        })
                                })
                            })
                            .min_by_key(|f| f.actor)?;
                        Some((encounter.id, ally.team))
                    })
                    .min_by_key(|(id, _)| *id);
                if let Some((encounter, team)) = found {
                    candidates.push(Reinforcement {
                        encounter,
                        actor: member.person,
                        team,
                        from,
                    });
                }
            }
        }
        candidates
    }
}
#[cfg(test)]
mod tests;

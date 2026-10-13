//! Initiative driving belongs to Conflict; ports only bind other owners' operations.
use crate::*;
use game_ecs::Store;
pub trait CombatDrivePort {
    type Action;
    fn store(&self) -> &Store;
    fn ruler(&self) -> PersonId;
    fn controlled(&self, actor: PersonId) -> bool;
    fn can_act(&self, actor: PersonId) -> bool;
    fn observe(&mut self, id: EncounterId, actor: PersonId) -> Result<(), Error>;
    fn review_mission(&mut self, id: EncounterId, actor: PersonId);
    fn mission_action(
        &mut self,
        id: EncounterId,
        actor: PersonId,
    ) -> Option<(Option<Point>, Self::Action)>;
    fn force_action(&self, id: EncounterId, actor: PersonId) -> Option<Self::Action>;
    fn autonomy_facts(
        &self,
        id: EncounterId,
        actor: PersonId,
    ) -> Result<CombatAutonomyFacts, Error>;
    fn bind_action(&self, action: CombatAutonomyAction) -> Self::Action;
    fn wait_action(&self) -> Self::Action;
    fn apply_turn(
        &mut self,
        id: EncounterId,
        actor: PersonId,
        movement: Option<Point>,
        action: Self::Action,
        search: Option<SearchState>,
    ) -> Result<(), Error>;
    fn skip(&mut self, id: EncounterId) -> Result<(), Error>;
    fn schedule(&mut self, id: EncounterId, ends: SimTime);
}
impl World {
    pub fn drive_npcs(
        &self,
        id: EncounterId,
        port: &mut impl CombatDrivePort,
    ) -> Result<(), Error> {
        loop {
            let current = encounter(port.store(), id)
                .ok_or(Error::Unavailable)?
                .order
                .get(encounter(port.store(), id).unwrap().cursor)
                .copied();
            let Some(actor) = current else { break };
            match self.npc_turn(
                port.store(),
                id,
                port.ruler(),
                port.controlled(actor),
                port.can_act(actor),
            )? {
                NpcTurn::Finished | NpcTurn::AwaitInput => break,
                NpcTurn::Skip => {
                    port.skip(id)?;
                    continue;
                }
                NpcTurn::Act(actor) => {
                    port.observe(id, actor)?;
                    port.review_mission(id, actor);
                    let (movement, action, search) =
                        if let Some(action) = port.mission_action(id, actor) {
                            (action.0, action.1, None)
                        } else if let Some(action) = port.force_action(id, actor) {
                            (None, action, None)
                        } else {
                            let e = encounter(port.store(), id).ok_or(Error::Unavailable)?;
                            let scene = map(port.store(), e.map).ok_or(Error::Unavailable)?;
                            let facts = port.autonomy_facts(id, actor)?;
                            let decision = default_combat_policy(scene, &self.rules, &facts);
                            (
                                decision.movement,
                                port.bind_action(decision.action),
                                Some(decision.search),
                            )
                        };
                    if port
                        .apply_turn(id, actor, movement, action, search)
                        .is_err()
                    {
                        let wait = port.wait_action();
                        port.apply_turn(id, actor, None, wait, None)?;
                    }
                }
            }
        }
        let ends = encounter(port.store(), id).ok_or(Error::Unavailable)?.ends;
        port.schedule(id, ends);
        Ok(())
    }
}

//! Permission to physically release a captive from actual body and tool facts.
use crate::*;
use game_cargo::{Inventory, InventoryLocation};
#[derive(Clone, Copy, Debug)]
pub struct ReleaseFacts {
    pub actor: PersonId,
    pub target: PersonId,
    pub body: TargetBody,
    pub independent: bool,
    pub available_area: bool,
    pub actor_captive: bool,
    pub contact: bool,
    pub moving: bool,
}
impl Rules {
    pub fn release_available<'a>(
        &self,
        facts: ReleaseFacts,
        captive: Option<&Captivity>,
        tools: impl IntoIterator<Item = &'a Inventory>,
    ) -> bool {
        let Some(captive) = captive else {
            return false;
        };
        facts.available_area
            && facts.independent
            && facts.body.health > self.incapacitation
            && (!facts.actor_captive || facts.actor == facts.target)
            && facts.contact
            && !facts.moving
            && (captive.captor == facts.actor
                || tools.into_iter().any(|stock| {
                    stock.location == InventoryLocation::Carried(facts.actor)
                        && stock.good == Good::Tool
                        && stock.quantity.0 >= 1000
                }))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSearch {
    encounter: EncounterId,
    actor: PersonId,
    position: Point,
    before: SearchState,
    after: SearchState,
}
impl PreparedSearch {
    pub fn new(encounter: EncounterId, fighter: &Fighter, after: SearchState) -> Self {
        Self {
            encounter,
            actor: fighter.actor,
            position: fighter.position,
            before: fighter.search.clone(),
            after,
        }
    }
    pub fn commit(self, world: &mut World) -> Result<(), Error> {
        let map = world
            .encounters
            .get(&self.encounter)
            .and_then(|e| world.map(e.map))
            .ok_or(Error::Unavailable)?;
        if !self.after.valid(map) {
            return Err(Error::Unavailable);
        }
        let fighter = world
            .encounters
            .get_mut(&self.encounter)
            .and_then(|e| e.fighters.get_mut(&self.actor))
            .ok_or(Error::Unavailable)?;
        if fighter.position != self.position || fighter.search != self.before {
            return Err(Error::Unavailable);
        }
        fighter.search = self.after;
        Ok(())
    }
}
#[cfg(test)]
mod tests;

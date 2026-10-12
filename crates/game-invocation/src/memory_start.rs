//! Invocation selects and admits memory work from narrowly supplied physical facts.
use crate::*;
use game_core::*;
use game_ecs::{Batch, Component, StableId, Store};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryPerson {
    pub location: Location,
    pub independent: bool,
    pub in_combat: bool,
    pub restricted: bool,
    pub alive: bool,
    pub health: u64,
    pub idle_or_resting: bool,
    pub teaching_completes: Option<SimTime>,
    pub sociability: u16,
    pub trust: i32,
    pub hunger: u64,
    pub meal_relief: u64,
}
impl MemoryPerson {
    pub fn area(&self) -> Option<AreaId> {
        if !self.independent || self.in_combat || self.restricted {
            return None;
        }
        match self.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        }
    }
}
/// Factual permissions and reservations come from their respective owners.
pub trait MemoryFacts {
    fn person(&self, person: PersonId, other: PersonId) -> Option<MemoryPerson>;
    fn stock_area(&self, inventory: InventoryId) -> Option<AreaId>;
    fn has_access(&self, actor: PersonId, inventory: InventoryId) -> bool;
    fn tool_available(&self, actor: PersonId, inventory: InventoryId) -> bool;
    fn unreserved(&self, actor: PersonId, inventory: InventoryId) -> u64;
    fn carried_quantity(&self, actor: PersonId) -> u128;
    fn carrying_capacity(&self, actor: PersonId) -> u64;
    fn creation_body_capacity(&self, actor: PersonId) -> u64;
    fn creation_transport_capacity(&self, actor: PersonId, quantity: Quantity) -> bool;
}
#[derive(Debug)]
pub struct PreparedMemoryStart {
    at: SimTime,
    request: InvocationAction,
    rules: Rules,
    active: Option<Active>,
    before: Option<MemoryAction>,
    action: MemoryAction,
    material: Option<game_cargo::PreparedConsumptions>,
}
#[derive(Debug)]
pub struct MemoryReplacement {
    actor: PersonId,
}
impl MemoryReplacement {
    pub fn actor(&self) -> PersonId {
        self.actor
    }
}
impl PreparedMemoryStart {
    pub fn stage_into_with_replacement<A: Component>(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl MemoryFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<(MemoryAction, MemoryReplacement), Error> {
        let action = self.stage_into::<A>(runtime, store, facts, now, batch)?;
        let replacement = MemoryReplacement {
            actor: action.actor,
        };
        Ok((action, replacement))
    }
    pub fn action(&self) -> &MemoryAction {
        &self.action
    }
    pub fn stage_into<A: Component>(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl MemoryFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MemoryAction, Error> {
        if now != self.at
            || runtime.control.rules != self.rules
            || runtime.control.active(store) != self.active.as_ref()
            || memory_action(store, self.action.actor) != self.before.as_ref()
        {
            return Err(Error::Unavailable);
        }
        let current = runtime.prepare_memory_start::<A>(
            store,
            facts,
            self.action.actor,
            &self.request,
            now,
        )?;
        if current.action != self.action {
            return Err(Error::Unavailable);
        }
        if let Some(material) = self.material {
            material.stage_into(store, batch)?;
        }
        batch.insert(StableId(self.action.actor.0), self.action.clone());
        Ok(self.action)
    }
}
impl Runtime {
    pub fn can_read_record(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
        id: RecordId,
    ) -> bool {
        let Some(record) = record(store, id).filter(|record| record.intact) else {
            return false;
        };
        let Some(stock) = game_cargo::inventory(store, record.inventory) else {
            return false;
        };
        facts.person(actor, actor).is_some_and(|person| {
            person.alive
                && person.health > 0
                && person
                    .area()
                    .is_some_and(|area| facts.stock_area(stock.id) == Some(area))
        }) && stock.good == Good::Record
            && stock.quantity.0 >= 1000
            && facts.has_access(actor, stock.id)
    }
    pub(crate) fn local_input(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
        inventory: InventoryId,
        good: Good,
    ) -> bool {
        game_cargo::inventory(store, inventory).is_some_and(|stock| {
            stock.good == good
                && stock.quantity.0 >= 1000
                && (good != Good::Timber || facts.unreserved(actor, inventory) >= 1000)
                && (good != Good::Tool || facts.tool_available(actor, inventory))
                && facts.has_access(actor, inventory)
                && facts.stock_area(inventory)
                    == facts.person(actor, actor).and_then(|person| person.area())
        })
    }
    pub fn prepare_memory_start<A: Component>(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
        request: &InvocationAction,
        now: SimTime,
    ) -> Result<PreparedMemoryStart, Error> {
        let person = facts.person(actor, actor).ok_or(Error::Unavailable)?;
        if !person.alive
            || person.health == 0
            || controls_actor(
                self.control
                    .active(store)
                    .and_then(|active| active.episode.as_ref()),
                actor,
                now,
                person.alive,
            )
        {
            return Err(Error::Unavailable);
        }
        let area = person.area().ok_or(Error::Unavailable)?;
        let recalled_stock =
            |life: PersonId, inventory: InventoryId| -> Result<RecordedMemory, Error> {
                if !self
                    .control
                    .active(store)
                    .is_some_and(|active| active.actor == actor && active.lives.contains(&life))
                {
                    return Err(Error::Unavailable);
                }
                let fact = archive::<A>(store, life)
                    .and_then(|past| past.stocks.known.get(&inventory))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                Ok(RecordedMemory::Stock { life, fact })
            };
        let write = |content: RecordedMemory,
                     material: InventoryId,
                     tool: InventoryId|
         -> Result<(MemoryWork, SimDuration), Error> {
            if !self.local_input(store, facts, actor, material, Good::Timber)
                || !self.local_input(store, facts, actor, tool, Good::Tool)
            {
                return Err(Error::Unavailable);
            }
            Ok((
                MemoryWork::Write {
                    content,
                    material,
                    tool,
                },
                self.control.rules.writing_time,
            ))
        };
        let (work, duration) = match request {
            InvocationAction::RecordMemory {
                life,
                inventory,
                material,
                tool,
            } => write(recalled_stock(*life, *inventory)?, *material, *tool)?,
            InvocationAction::RecordRoute {
                life,
                origin,
                destination,
                material,
                tool,
            } => {
                if !self
                    .control
                    .active(store)
                    .is_some_and(|active| active.actor == actor && active.lives.contains(life))
                {
                    return Err(Error::Unavailable);
                }
                let duration = archive::<A>(store, *life)
                    .and_then(|past| past.mind.routes.get(origin))
                    .and_then(|links| links.get(destination))
                    .copied()
                    .ok_or(Error::Unavailable)?;
                write(
                    RecordedMemory::Route {
                        life: *life,
                        origin: *origin,
                        destination: *destination,
                        duration,
                    },
                    *material,
                    *tool,
                )?
            }
            InvocationAction::ReadRecord { record } => {
                if !self.can_read_record(store, facts, actor, *record) {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Read { record: *record },
                    self.control.rules.reading_time,
                )
            }
            InvocationAction::MoveRecord {
                record: id,
                destination,
            } => {
                if !self.can_read_record(store, facts, actor, *id) {
                    return Err(Error::Unavailable);
                }
                match destination {
                    StockPosition::Area(target) if *target != area => {
                        return Err(Error::Unavailable);
                    }
                    StockPosition::Carried(target)
                        if facts
                            .person(*target, actor)
                            .and_then(|person| person.area())
                            != Some(area) =>
                    {
                        return Err(Error::Unavailable);
                    }
                    _ => {}
                }
                let source = game_cargo::inventory(store, record(store, *id).unwrap().inventory)
                    .ok_or(Error::Unavailable)?;
                let desired = match destination {
                    StockPosition::Area(area) => game_cargo::InventoryLocation::Area(*area),
                    StockPosition::Carried(actor) => game_cargo::InventoryLocation::Carried(*actor),
                };
                if source.location == desired
                    || !matches!(source.location,game_cargo::InventoryLocation::Carried(holder) if holder==actor)
                        && source.location != game_cargo::InventoryLocation::Area(area)
                {
                    return Err(Error::Unavailable);
                }
                if let StockPosition::Carried(carrier) = destination
                    && facts.carried_quantity(*carrier) + 1000
                        > u128::from(facts.carrying_capacity(*carrier))
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Move {
                        record: *id,
                        destination: destination.clone(),
                    },
                    self.control.rules.moving_time,
                )
            }
            InvocationAction::TeachMemory {
                life,
                inventory,
                recipient,
            } => {
                let content = recalled_stock(*life, *inventory)?;
                let recipient_facts = facts.person(*recipient, actor).ok_or(Error::Unavailable)?;
                if *recipient == actor
                    || recipient_facts.area() != Some(area)
                    || !recipient_facts.alive
                    || !recipient_facts.idle_or_resting
                    || !game_characters::memories::accepts_memory_teaching(
                        recipient_facts.sociability,
                        recipient_facts.trust,
                        recipient_facts.hunger,
                        recipient_facts.meal_relief,
                    )
                {
                    return Err(Error::Unavailable);
                }
                (
                    MemoryWork::Teach {
                        content,
                        recipient: *recipient,
                    },
                    self.control.rules.teaching_time,
                )
            }
            _ => return Err(Error::Unavailable),
        };
        let action = MemoryAction {
            actor,
            area,
            completes: SimTime(now.0.checked_add(duration.0).ok_or(Error::Overflow)?),
            work,
        };
        let material = if let MemoryWork::Write { material, .. } = action.work {
            Some(game_cargo::prepare_consumptions(
                store,
                &[(material, Quantity(1000))],
            )?)
        } else {
            None
        };
        action.validate()?;
        Ok(PreparedMemoryStart {
            at: now,
            request: request.clone(),
            rules: self.control.rules.clone(),
            active: self.control.active(store).cloned(),
            before: memory_action(store, actor).cloned(),
            action,
            material,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests;

impl Runtime {
    pub fn available_memory_actions(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
        now: SimTime,
    ) -> Vec<InvocationAction> {
        if facts.person(actor, actor).is_none_or(|person| {
            controls_actor(
                self.control
                    .active(store)
                    .and_then(|active| active.episode.as_ref()),
                actor,
                now,
                person.alive,
            )
        }) {
            return Vec::new();
        }
        records(store)
            .filter(|(id, _)| self.can_read_record(store, facts, actor, *id))
            .take(32)
            .map(|(record, _)| InvocationAction::ReadRecord { record })
            .collect()
    }
}

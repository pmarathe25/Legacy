//! Deadline selection, physical revalidation, and completion belong to Invocation.
use crate::*;
use game_core::*;
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryEffect {
    Recorded {
        record: RecordId,
        actor: PersonId,
        inventory: InventoryId,
    },
    Read {
        record: RecordId,
        actor: PersonId,
    },
    Taught {
        actor: PersonId,
        recipient: PersonId,
        life: PersonId,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Interrupted {
        actor: PersonId,
    },
    Cancelled,
}
#[derive(Debug)]
pub struct PreparedMemoryCompletion {
    at: SimTime,
    action: MemoryAction,
    active: Option<Active>,
    allocation_start: u64,
    allocation_demand: u64,
    effect: MemoryEffect,
    record_patch: Option<(Option<Record>, Record)>,
    cargo: Option<game_cargo::PreparedInventoryCreation>,
    learning: Option<game_characters::memories::PreparedMemoryLearning>,
    ending: PreparedMemoryEnding,
}
#[derive(Debug)]
pub struct MemoryCompletion {
    pub effect: MemoryEffect,
    pub participants: Vec<PersonId>,
    pub allocation_demand: u64,
    learning: Option<game_characters::memories::MemoryLearningFinalization>,
    reads: ReadGuard,
}
impl MemoryCompletion {
    pub fn validate_batch<A: Component>(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.reads.validate(store, batch)?;
        if let Some(learning) = &self.learning {
            learning.validate_batch::<A>(store, batch)?;
        }
        store.validate_batch(batch)
    }
    /// Canonical learning rows were staged with the memory source transaction.
    /// Kept for callers that finalize an admitted completion after publication.
    pub fn finish_learning<A: Component>(self, store: &mut Store) {
        if let Some(learning) = self.learning {
            learning.finish::<A>(store);
        }
    }
}
impl Runtime {
    pub fn due_memory_actions(&self, store: &Store, now: SimTime) -> Vec<PersonId> {
        memory_actions(store)
            .filter(|(_, action)| action.completes <= now)
            .map(|(actor, _)| actor)
            .collect()
    }
    pub fn memory_boundary(&self, store: &Store, now: SimTime, limit: SimTime) -> SimTime {
        memory_actions(store)
            .filter(|(_, action)| action.completes > now)
            .map(|(_, action)| action.completes)
            .fold(limit, SimTime::min)
    }
    pub fn selected_memory_tool(&self, store: &Store, actor: PersonId) -> Option<InventoryId> {
        match memory_action(store, actor)?.work {
            MemoryWork::Write { tool, .. } => Some(tool),
            _ => None,
        }
    }
    pub fn memory_allocation_bound(&self, store: &Store, now: SimTime) -> u64 {
        memory_actions(store)
            .filter(|(_, action)| action.completes <= now)
            .map(|(_, action)| match action.work {
                MemoryWork::Write { .. } => 2,
                MemoryWork::Move { .. } => 1,
                _ => 0,
            })
            .sum()
    }
    pub fn prepare_memory_completion<A: Component>(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
        now: SimTime,
        allocation_start: u64,
    ) -> Result<PreparedMemoryCompletion, Error> {
        let action = memory_action(store, actor)
            .filter(|action| action.completes <= now)
            .cloned()
            .ok_or(Error::Unavailable)?;
        let mut effect = MemoryEffect::Cancelled;
        let mut record_patch = None;
        let mut cargo = None;
        let mut learning = None;
        let mut allocation_demand = 0;
        let local = facts
            .person(actor, actor)
            .is_some_and(|person| person.alive && person.area() == Some(action.area));
        if local {
            effect = MemoryEffect::Interrupted { actor };
            let recalls = |content: &RecordedMemory| {
                self.control.active(store).is_some_and(|active| {
                    active.actor == actor && active.lives.contains(&content.life())
                })
            };
            let has_room = |holder: PersonId| {
                facts.carried_quantity(holder) + 1000 <= u128::from(facts.carrying_capacity(holder))
            };
            let new_stock = |id, location, access, quantity| game_cargo::Inventory {
                id,
                location,
                quantity,
                capacity: Quantity(10_000),
                access,
                last_granted: None,
                good: Good::Record,
                spoils_at: None,
            };
            match &action.work {
                MemoryWork::Write { content, tool, .. }
                    if recalls(content)
                        && self.local_input(store, facts, actor, *tool, Good::Tool)
                        && has_room(actor)
                        && facts.carried_quantity(actor) + 1000
                            <= u128::from(facts.creation_body_capacity(actor))
                        && facts.creation_transport_capacity(actor, Quantity(1000)) =>
                {
                    let id = RecordId(allocation_start);
                    let inventory =
                        InventoryId(allocation_start.checked_add(1).ok_or(Error::Overflow)?);
                    allocation_start.checked_add(2).ok_or(Error::Overflow)?;
                    if allocation_start == 0 || store.contains(StableId(id.0)) {
                        return Err(Error::Unavailable);
                    }
                    cargo = Some(game_cargo::prepare_inventory_creation(
                        store,
                        new_stock(
                            inventory,
                            game_cargo::InventoryLocation::Carried(actor),
                            BTreeSet::from([actor]),
                            Quantity(1000),
                        ),
                    )?);
                    let row = Record {
                        id,
                        inventory,
                        content: content.clone(),
                        author: actor,
                        written_at: now,
                        readers: BTreeSet::new(),
                        intact: true,
                    };
                    record_patch = Some((None, row));
                    effect = MemoryEffect::Recorded {
                        record: id,
                        actor,
                        inventory,
                    };
                    allocation_demand = 2;
                }
                MemoryWork::Read { record: id }
                    if self.can_read_record(store, facts, actor, *id) =>
                {
                    let before = record(store, *id).unwrap().clone();
                    learning = Some(before.content.prepare_learning::<A>(
                        store,
                        actor,
                        now,
                        KnowledgeSource::Recorded(*id),
                    )?);
                    let mut after = before.clone();
                    after.readers.insert(actor);
                    record_patch = Some((Some(before), after));
                    effect = MemoryEffect::Read { record: *id, actor };
                }
                MemoryWork::Teach { content, recipient }
                    if recalls(content)
                        && facts.person(*recipient, actor).is_some_and(|person| {
                            person.area() == Some(action.area)
                                && person.teaching_completes == Some(action.completes)
                        }) =>
                {
                    learning = Some(content.prepare_learning::<A>(
                        store,
                        *recipient,
                        now,
                        KnowledgeSource::Reported(actor),
                    )?);
                    effect = MemoryEffect::Taught {
                        actor,
                        recipient: *recipient,
                        life: content.life(),
                    };
                }
                MemoryWork::Move {
                    record: id,
                    destination,
                } if self.can_read_record(store, facts, actor, *id) => {
                    let valid = match destination {
                        StockPosition::Area(area) => *area == action.area,
                        StockPosition::Carried(holder) => {
                            facts
                                .person(*holder, actor)
                                .is_some_and(|person| person.area() == Some(action.area))
                                && has_room(*holder)
                        }
                    };
                    if valid {
                        let before = record(store, *id).unwrap().clone();
                        let source = game_cargo::inventory(store, before.inventory)
                            .ok_or(Error::Unavailable)?;
                        let mut access = source.access.clone();
                        let location = match destination {
                            StockPosition::Area(area) => game_cargo::InventoryLocation::Area(*area),
                            StockPosition::Carried(holder) => {
                                access.insert(*holder);
                                game_cargo::InventoryLocation::Carried(*holder)
                            }
                        };
                        allocation_start.checked_add(1).ok_or(Error::Overflow)?;
                        let inventory = InventoryId(allocation_start);
                        cargo = Some(game_cargo::prepare_inventory_split(
                            store,
                            before.inventory,
                            new_stock(inventory, location, access, Quantity(0)),
                            Quantity(1000),
                        )?);
                        let mut after = before.clone();
                        after.inventory = inventory;
                        record_patch = Some((Some(before.clone()), after));
                        effect = MemoryEffect::Transferred {
                            actor,
                            source: before.inventory,
                            destination: inventory,
                            quantity: Quantity(1000),
                        };
                        allocation_demand = 1;
                    }
                }
                _ => {}
            }
        }
        Ok(PreparedMemoryCompletion {
            at: now,
            action,
            active: self.control.active(store).cloned(),
            allocation_start,
            allocation_demand,
            effect,
            record_patch,
            cargo,
            learning,
            ending: self.prepare_memory_ending(store, &[actor]),
        })
    }
}
impl PreparedMemoryCompletion {
    pub fn effect(&self) -> &MemoryEffect {
        &self.effect
    }
    pub fn allocation_demand(&self) -> u64 {
        self.allocation_demand
    }
    pub fn activity_resets(&self, references: &impl MemoryReferences) -> Vec<PersonId> {
        self.ending.activity_resets(references)
    }
    pub fn participants(&self) -> &[PersonId] {
        self.ending.participants()
    }
    pub fn stage_into<A: Component>(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl MemoryFacts,
        now: SimTime,
        batch: &mut Batch,
    ) -> Result<MemoryCompletion, Error> {
        if now != self.at
            || runtime.control.active(store) != self.active.as_ref()
            || memory_action(store, self.action.actor) != Some(&self.action)
        {
            return Err(Error::Unavailable);
        }
        let current = runtime.prepare_memory_completion::<A>(
            store,
            facts,
            self.action.actor,
            now,
            self.allocation_start,
        )?;
        if current.effect != self.effect
            || current.record_patch != self.record_patch
            || current.learning != self.learning
            || current.ending != self.ending
        {
            return Err(Error::Unavailable);
        }
        if let Some(learning) = &self.learning {
            learning.validate::<A>(store)?;
        }
        let mut reads = ReadSet::new();
        let mut admitted = ReadFootprint::new();
        reads.membership::<ControlSession>(store, |_, _| true);
        for (actor, _) in sessions(store) {
            reads.read::<ControlSession>(store, StableId(actor.0));
        }
        let participants = self.ending.participants().to_vec();
        reads.membership::<MemoryAction>(store, move |_, action| {
            participants.contains(&action.actor)
                || matches!(action.work, MemoryWork::Teach { recipient, .. }
                    if participants.contains(&recipient))
        });
        for actor in self.ending.owners() {
            reads.read::<MemoryAction>(store, StableId(actor.0));
            admitted.protect::<MemoryAction>(StableId(actor.0));
        }
        for actor in self.ending.participants() {
            reads.read::<Location>(store, StableId(actor.0));
        }
        match self.action.work {
            MemoryWork::Write { tool, .. } => {
                reads.read::<game_cargo::Inventory>(store, StableId(tool.0));
            }
            MemoryWork::Read { record: id } | MemoryWork::Move { record: id, .. } => {
                if let Some(record) = reads.read::<Record>(store, StableId(id.0)) {
                    reads.read::<game_cargo::Inventory>(store, StableId(record.inventory.0));
                }
            }
            MemoryWork::Teach { .. } => {}
        }
        if let Some((before, after)) = &self.record_patch {
            reads.expect_row(StableId(after.id.0), before.clone());
            admitted.protect::<Record>(StableId(after.id.0));
        }
        match self.effect {
            MemoryEffect::Recorded {
                record, inventory, ..
            } => {
                reads.exclusive_entity(store, StableId(record.0))?;
                reads.exclusive_entity(store, StableId(inventory.0))?;
                reads.read::<game_cargo::Inventory>(store, StableId(inventory.0));
                admitted.protect::<game_cargo::Inventory>(StableId(inventory.0));
            }
            MemoryEffect::Transferred {
                source,
                destination,
                ..
            } => {
                reads.read::<game_cargo::Inventory>(store, StableId(source.0));
                reads.read::<game_cargo::Inventory>(store, StableId(destination.0));
                reads.exclusive_entity(store, StableId(destination.0))?;
                admitted.protect::<game_cargo::Inventory>(StableId(source.0));
                admitted.protect::<game_cargo::Inventory>(StableId(destination.0));
            }
            _ => {}
        }

        if let Some((before, after)) = self.record_patch {
            if record(store, after.id) != before.as_ref() {
                return Err(Error::Unavailable);
            }
            batch.insert(StableId(after.id.0), after);
        }
        if let Some(cargo) = self.cargo {
            cargo.stage_into(store, batch)?;
        }
        let participants = self.ending.stage_into(runtime, store, None, batch)?;
        let learning = self
            .learning
            .map(|learning| learning.stage_into::<A>(store, batch))
            .transpose()?;
        Ok(MemoryCompletion {
            effect: self.effect,
            participants,
            allocation_demand: self.allocation_demand,
            learning,
            reads: reads.bind(store, batch, &admitted)?,
        })
    }
}

#[cfg(test)]
mod tests;

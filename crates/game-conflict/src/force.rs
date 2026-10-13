//! Physical yield, timed bindings, escape attempts, and intervention decisions.
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Conservative room for the existing map/encounter intervention envelope.
pub const fn intervention_reserve() -> (u64, usize) {
    (2, 128)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub restraint_time: SimDuration,
    pub escape_time: SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            restraint_time: SimDuration(6_000),
            escape_time: SimDuration(6_000),
        }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.restraint_time.0 == 0 || self.escape_time.0 == 0 {
            return Err(Error::InvalidConfiguration(
                "zero force work duration".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escape {
    pub target: PersonId,
    pub actor: PersonId,
    pub tool: Option<InventoryId>,
    pub began: SimTime,
    pub completes: SimTime,
    pub contested: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restraint {
    pub target: PersonId,
    pub binding: InventoryId,
    pub area: AreaId,
    pub location: Location,
    pub position: Point,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Yielded {
    pub captors: BTreeSet<PersonId>,
    pub area: AreaId,
    pub at: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
    pub last_event: u64,
}
impl Runtime {
    pub fn new(last_event: u64) -> Self {
        Self {
            rules: Rules::default(),
            last_event,
        }
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Yielded>("force/yielded", |row| {
        if row.captors.is_empty() || row.area.0 == 0 || row.captors.iter().any(|id| id.0 == 0) {
            return Err(Error::InvalidSave("invalid yielding declaration".into()));
        }
        Ok(())
    })?;
    schema.register::<Restraint>("force/restraint", |row| {
        if row.target.0 == 0 || row.binding.0 == 0 || row.area.0 == 0 {
            return Err(Error::InvalidSave("invalid restraint work".into()));
        }
        Ok(())
    })?;
    schema.register::<Escape>("force/escape", |row| {
        if row.target.0 == 0
            || row.actor.0 == 0
            || row.began >= row.completes
            || row.tool.is_some_and(|tool| tool.0 == 0)
        {
            return Err(Error::InvalidSave("invalid escape attempt".into()));
        }
        Ok(())
    })
}
pub fn yielded(store: &Store, actor: PersonId) -> Option<&Yielded> {
    store.get(StableId(actor.0))
}
pub fn restraint(store: &Store, actor: PersonId) -> Option<&Restraint> {
    store.get(StableId(actor.0))
}
pub fn escape(store: &Store, target: PersonId) -> Option<&Escape> {
    store.get(StableId(target.0))
}
pub fn yields(store: &Store) -> impl Iterator<Item = (PersonId, &Yielded)> {
    store
        .iter::<Yielded>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn restraints(store: &Store) -> impl Iterator<Item = (PersonId, &Restraint)> {
    store
        .iter::<Restraint>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn escapes(store: &Store) -> impl Iterator<Item = (PersonId, &Escape)> {
    store
        .iter::<Escape>()
        .map(|(id, row)| (PersonId(id.0), row))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Yielded { actor: PersonId, captor: PersonId },
    YieldWithdrawn { actor: PersonId },
    EscapeAttempted { actor: PersonId },
    EscapeInterrupted { actor: PersonId },
    WorkInterrupted { actor: PersonId },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Work {
    Restrain { actor: PersonId, row: Restraint },
    Escape { actor: PersonId, row: Escape },
}
#[derive(Clone, Debug)]
enum Change {
    Yield(PersonId, Option<Yielded>),
    Restraint(PersonId, Option<Restraint>),
    Escape(PersonId, Option<Escape>),
    Captivity(PersonId),
}
#[derive(Debug)]
pub struct PreparedChange {
    reads: ReadSet,
    changes: Vec<Change>,
    pub effects: Vec<Effect>,
    work: Option<Work>,
}
impl PreparedChange {
    fn new() -> Self {
        Self {
            reads: ReadSet::new(),
            changes: vec![],
            effects: vec![],
            work: None,
        }
    }
    fn yield_row(&mut self, store: &Store, actor: PersonId, row: Option<Yielded>) {
        self.reads.read::<Yielded>(store, StableId(actor.0));
        self.changes.push(Change::Yield(actor, row));
    }
    fn restraint_row(&mut self, store: &Store, actor: PersonId, row: Option<Restraint>) {
        self.reads.read::<Restraint>(store, StableId(actor.0));
        self.changes.push(Change::Restraint(actor, row));
    }
    fn escape_row(&mut self, store: &Store, target: PersonId, row: Option<Escape>) {
        self.reads.read::<Escape>(store, StableId(target.0));
        self.changes.push(Change::Escape(target, row));
    }
    fn replace_with(&mut self, next: PreparedChange) {
        self.reads.extend(next.reads);
        for change in next.changes {
            self.changes.retain(|old| match (old, &change) {
                (Change::Yield(a, _), Change::Yield(b, _))
                | (Change::Restraint(a, _), Change::Restraint(b, _))
                | (Change::Escape(a, _), Change::Escape(b, _))
                | (Change::Captivity(a), Change::Captivity(b)) => a != b,
                _ => true,
            });
            self.changes.push(change);
        }
        self.effects.extend(next.effects);
        if next.work.is_some() {
            self.work = next.work;
        }
    }
    pub fn include_release_cleanup(
        mut self,
        store: &Store,
        target: PersonId,
    ) -> Result<Self, Error> {
        self.replace_with(prepare_release_cleanup(store, target)?);
        Ok(self)
    }
    pub fn include_interruptions(mut self, store: &Store, actors: &[PersonId]) -> Self {
        self.replace_with(prepare_interrupt_many(store, actors));
        self
    }
    /// Admission retains room for the existing delayed binding/map or the
    /// possible intervention even when no opening is selected immediately.
    pub fn start_reserve(&self) -> (u64, usize) {
        match &self.work {
            Some(Work::Restrain { .. }) => (2, 3),
            Some(Work::Escape { .. }) => intervention_reserve(),
            None => (0, 0),
        }
    }
    pub fn scheduled_work(&self) -> Option<(PersonId, SimTime)> {
        match &self.work {
            Some(Work::Restrain { actor, row }) => Some((*actor, row.completes)),
            Some(Work::Escape { actor, row }) if !row.contested => Some((*actor, row.completes)),
            _ => None,
        }
    }
    pub fn work(&self) -> Option<&Work> {
        self.work.as_ref()
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<Receipt, Error> {
        self.stage_with_motion(store, batch, None)
    }
    pub fn stage_after_motion(
        self,
        store: &Store,
        batch: &mut Batch,
        motion: &game_transport::MotionFinalization,
    ) -> Result<Receipt, Error> {
        self.stage_with_motion(store, batch, Some(motion))
    }
    fn stage_with_motion(
        self,
        store: &Store,
        batch: &mut Batch,
        motion: Option<&game_transport::MotionFinalization>,
    ) -> Result<Receipt, Error> {
        self.reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        if let Some(motion) = motion {
            motion.validate_footprint(store, batch)?;
            for id in motion.location_ids() {
                admitted.protect::<Location>(id);
            }
        }
        let mut released = BTreeSet::new();
        for change in self.changes {
            match change {
                Change::Yield(actor, row) => {
                    let id = StableId(actor.0);
                    admitted.protect::<Yielded>(id);
                    if let Some(row) = row {
                        batch.insert(id, row);
                    } else {
                        batch.remove::<Yielded>(id);
                    }
                }
                Change::Restraint(actor, row) => {
                    let id = StableId(actor.0);
                    admitted.protect::<Restraint>(id);
                    if let Some(row) = row {
                        batch.insert(id, row);
                    } else {
                        batch.remove::<Restraint>(id);
                    }
                }
                Change::Captivity(actor) => {
                    released.insert(actor);
                    admitted.protect::<crate::Captivity>(StableId(actor.0));
                    batch.remove::<crate::Captivity>(StableId(actor.0));
                }
                Change::Escape(actor, row) => {
                    let id = StableId(actor.0);
                    admitted.protect::<Escape>(id);
                    if let Some(row) = row {
                        batch.insert(id, row);
                    } else {
                        batch.remove::<Escape>(id);
                    }
                }
            }
        }
        Ok(Receipt {
            reads: self.reads.bind(store, batch, &admitted)?,
            effects: self.effects,
            released,
        })
    }
    pub fn commit(self, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let receipt = self.stage_into(store, &mut batch)?;
        receipt.validate_batch(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(receipt.effects)
    }
}
#[derive(Debug)]
pub struct Receipt {
    reads: ReadGuard,
    pub effects: Vec<Effect>,
    released: BTreeSet<PersonId>,
}
impl Receipt {
    pub fn released_targets(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.released.iter().copied()
    }
    pub fn into_guard(self) -> ReadGuard {
        self.reads
    }
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.reads.validate(store, batch)
    }
}
pub fn prepare_interrupt(store: &Store, actor: PersonId) -> PreparedChange {
    prepare_interrupt_many(store, &[actor])
}
/// Shared rescue attempts are removed once even when both participants are
/// interrupted in the same owner transaction. Contested attempts remain bound
/// to their actual initiative encounter.
pub fn prepare_interrupt_many(store: &Store, actors: &[PersonId]) -> PreparedChange {
    let requested = actors.iter().copied().collect::<BTreeSet<_>>();
    let mut plan = PreparedChange::new();
    for actor in &requested {
        plan.reads.read::<Restraint>(store, StableId(actor.0));
        if restraint(store, *actor).is_some() {
            plan.restraint_row(store, *actor, None);
        }
    }
    let scope = requested.clone();
    plan.reads.membership::<Escape>(store, move |id, row| {
        (scope.contains(&PersonId(id.0)) || scope.contains(&row.actor)) && !row.contested
    });
    for (target, _) in escapes(store).filter(|(target, row)| {
        (requested.contains(target) || requested.contains(&row.actor)) && !row.contested
    }) {
        plan.escape_row(store, target, None);
    }
    plan
}
pub fn prepare_released(store: &Store, target: PersonId) -> PreparedChange {
    let mut plan = PreparedChange::new();
    plan.reads.read::<Yielded>(store, StableId(target.0));
    plan.reads.read::<Escape>(store, StableId(target.0));
    if yielded(store, target).is_some() {
        plan.yield_row(store, target, None);
    }
    if escape(store, target).is_some() {
        plan.escape_row(store, target, None);
    }
    plan
}
mod policy;
pub use policy::*;
mod validation;
pub use validation::*;
#[cfg(test)]
mod tests;

/// Captivity and its force attempts end together. Transport supplies its own
/// prepared receiving reaction to the same release before publication.
pub fn prepare_release_cleanup(store: &Store, target: PersonId) -> Result<PreparedChange, Error> {
    crate::captivity(store, target).ok_or(Error::Unavailable)?;
    let mut plan = prepare_released(store, target);
    plan.reads
        .read::<crate::Captivity>(store, StableId(target.0));
    plan.changes.push(Change::Captivity(target));
    Ok(plan)
}

/// A retained map proposal is validated by its native confinement owner.
pub fn restraint_context(
    map: &crate::Map,
    location: &Location,
    target_position: Point,
) -> Result<crate::ConfinementContext, Error> {
    if !map.contains(target_position) {
        return Err(Error::Unavailable);
    }
    Ok(crate::ConfinementContext {
        map: map.id,
        area: map.area,
        location: location.clone(),
        target_position,
    })
}

/// A failed physical admission ends the timed attempt without treating overflow
/// or invalid configuration as an in-world interruption.
pub fn restraint_failure(
    mut removal: PreparedChange,
    actor: PersonId,
    error: Error,
) -> Result<PreparedChange, Error> {
    if error != Error::Unavailable {
        return Err(error);
    }
    removal.effects.push(Effect::WorkInterrupted { actor });
    Ok(removal)
}

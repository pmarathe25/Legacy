//! Sparse opening witnesses and owner-bound projections of a composed batch.
use crate::{Batch, BatchKey, Component, ReadFootprint, StableId, Store};
use game_core::Error;
use std::{collections::BTreeSet, sync::Arc};

trait Dependency: Send + Sync {
    fn validate(&self, store: &Store) -> bool;
    fn bind(
        &self,
        store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error>;
}
trait Projection: Send + Sync {
    fn validate(&self, store: &Store, batch: &Batch) -> Result<bool, Error>;
}

/// A plan's selected opening rows, absent rows, entity presence, and scoped
/// membership. Only requested values are cloned; unrelated edits remain valid.
#[derive(Clone, Default)]
pub struct ReadSet {
    dependencies: Vec<Arc<dyn Dependency>>,
}
impl std::fmt::Debug for ReadSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReadSet")
            .field("dependencies", &self.dependencies.len())
            .finish()
    }
}
impl ReadSet {
    pub fn new() -> Self {
        Self::default()
    }
    /// Combine factual inputs retained by cooperating owner interfaces.
    pub fn extend(&mut self, other: Self) {
        self.dependencies.extend(other.dependencies);
    }
    pub fn read<'a, C: Component>(&mut self, store: &'a Store, id: StableId) -> Option<&'a C> {
        let row = store.get::<C>(id);
        self.expect_row(id, row.cloned());
        row
    }
    /// Import an opening witness already retained by an owner.
    pub fn expect_row<C: Component>(&mut self, id: StableId, opening: Option<C>) {
        self.dependencies.push(Arc::new(Row { id, opening }));
    }
    pub fn entity(&mut self, store: &Store, id: StableId) -> bool {
        let present = store.contains(id);
        self.dependencies.push(Arc::new(Entity { id, present }));
        present
    }
    /// Track IDs matching a factual scope. Membership checks enumerate only this
    /// component column; use row reads for selected member values. The predicate
    /// must be pure and retain all scope inputs rather than reading mutable state.
    pub fn membership<C: Component>(
        &mut self,
        store: &Store,
        predicate: impl Fn(StableId, &C) -> bool + Send + Sync + 'static,
    ) -> BTreeSet<StableId> {
        let predicate: Arc<Scope<C>> = Arc::new(predicate);
        let members = members(store, &*predicate);
        self.dependencies.push(Arc::new(Membership {
            predicate,
            members: members.clone(),
        }));
        members
    }
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self.dependencies.iter().all(|read| read.validate(store)) {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    /// Bind immediately after this owner's staging, before foreign edits are
    /// appended. Every intentional edit must be explicitly declared in admitted;
    /// tracked rows cannot silently admit preexisting foreign writes. The guard
    /// retains their exact projected values and edit counts. Membership allows
    /// declared own additions/removals and protects the resulting scoped set.
    pub fn bind(
        self,
        store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<ReadGuard, Error> {
        self.validate(store)?;
        if !admitted.has_one_edit_per_row(batch) {
            return Err(Error::Unavailable);
        }
        let projections = self
            .dependencies
            .iter()
            .map(|read| read.bind(store, batch, admitted).map(Arc::from))
            .collect::<Result<_, _>>()?;
        let writes = admitted
            .rows
            .iter()
            .flat_map(|key| batch.edits_at(*key))
            .map(|edit| Arc::from(edit.cloned()))
            .collect();
        Ok(ReadGuard {
            key: batch.key(),
            opening: self,
            projections,
            writes,
        })
    }
}

/// An owner's admitted sparse projection, validated once all owners have staged.
#[derive(Clone)]
pub struct ReadGuard {
    key: BatchKey,
    opening: ReadSet,
    projections: Vec<Arc<dyn Projection>>,
    writes: Vec<Arc<dyn crate::backend::Edit>>,
}
impl std::fmt::Debug for ReadGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReadGuard")
            .field("opening", &self.opening)
            .field("writes", &self.writes.len())
            .finish()
    }
}
impl ReadGuard {
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        if self.key != batch.key() {
            return Err(Error::Unavailable);
        }
        self.opening.validate(store)?;
        for expected in &self.writes {
            let mut actual = batch.edits_at(expected.key());
            let Some(actual_edit) = actual.next() else {
                return Err(Error::Unavailable);
            };
            if actual.next().is_some() || !expected.same_value(actual_edit) {
                return Err(Error::Unavailable);
            }
        }
        for projection in &self.projections {
            if !projection.validate(store, batch)? {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
}

struct Row<C> {
    id: StableId,
    opening: Option<C>,
}
struct ProjectedRow<C> {
    id: StableId,
    expected: Option<C>,
    edits: usize,
}
impl<C: Component> Dependency for Row<C> {
    fn validate(&self, store: &Store) -> bool {
        store.get::<C>(self.id) == self.opening.as_ref()
    }
    fn bind(
        &self,
        store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        let edits = batch.row_edit_count((self.id, std::any::TypeId::of::<C>()));
        if edits > 0 && !admitted.contains::<C>(self.id) {
            return Err(Error::Unavailable);
        }
        Ok(Box::new(ProjectedRow {
            id: self.id,
            expected: batch.projected::<C>(store, self.id)?.cloned(),
            edits,
        }))
    }
}
impl<C: Component> Projection for ProjectedRow<C> {
    fn validate(&self, store: &Store, batch: &Batch) -> Result<bool, Error> {
        Ok(
            batch.row_edit_count((self.id, std::any::TypeId::of::<C>())) == self.edits
                && batch.projected::<C>(store, self.id)? == self.expected.as_ref(),
        )
    }
}

struct Entity {
    id: StableId,
    present: bool,
}
struct ProjectedEntity {
    id: StableId,
    present: bool,
}
fn projected_entity(store: &Store, batch: &Batch, id: StableId) -> bool {
    store.contains(id)
        || batch
            .edits_on(id)
            .any(|edit| edit.inserted_value().is_some())
}
impl Dependency for Entity {
    fn validate(&self, store: &Store) -> bool {
        store.contains(self.id) == self.present
    }
    fn bind(
        &self,
        store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        let creates = batch
            .edits_on(self.id)
            .any(|edit| edit.inserted_value().is_some());
        let admitted_creation = batch
            .edits_on(self.id)
            .any(|edit| edit.inserted_value().is_some() && admitted.rows.contains(&edit.key()));
        if !self.present && creates && !admitted_creation {
            return Err(Error::Unavailable);
        }
        Ok(Box::new(ProjectedEntity {
            id: self.id,
            present: projected_entity(store, batch, self.id),
        }))
    }
}
impl Projection for ProjectedEntity {
    fn validate(&self, store: &Store, batch: &Batch) -> Result<bool, Error> {
        Ok(projected_entity(store, batch, self.id) == self.present)
    }
}

type Scope<C> = dyn Fn(StableId, &C) -> bool + Send + Sync;
struct Membership<C> {
    predicate: Arc<Scope<C>>,
    members: BTreeSet<StableId>,
}
fn members<C: Component>(store: &Store, predicate: &Scope<C>) -> BTreeSet<StableId> {
    store
        .iter::<C>()
        .filter_map(|(id, row)| predicate(id, row).then_some(id))
        .collect()
}
fn projected_members<C: Component>(
    store: &Store,
    batch: &Batch,
    predicate: &Scope<C>,
) -> Result<BTreeSet<StableId>, Error> {
    let mut result = members(store, predicate);
    for id in batch.edited_ids::<C>() {
        result.remove(&id);
        if batch
            .projected::<C>(store, id)?
            .is_some_and(|row| predicate(id, row))
        {
            result.insert(id);
        }
    }
    Ok(result)
}
impl<C: Component> Dependency for Membership<C> {
    fn validate(&self, store: &Store) -> bool {
        members(store, &*self.predicate) == self.members
    }
    fn bind(
        &self,
        store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        for id in batch.edited_ids::<C>() {
            let before = self.members.contains(&id);
            let after = batch
                .projected::<C>(store, id)?
                .is_some_and(|row| (self.predicate)(id, row));
            if before != after && !admitted.contains::<C>(id) {
                return Err(Error::Unavailable);
            }
        }
        Ok(Box::new(Membership {
            predicate: self.predicate.clone(),
            members: projected_members(store, batch, &*self.predicate)?,
        }))
    }
}
impl<C: Component> Projection for Membership<C> {
    fn validate(&self, store: &Store, batch: &Batch) -> Result<bool, Error> {
        Ok(projected_members(store, batch, &*self.predicate)? == self.members)
    }
}

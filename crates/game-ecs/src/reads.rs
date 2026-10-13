//! Sparse opening witnesses and owner-bound projections of a composed batch.
use crate::{Batch, BatchKey, Component, ReadFootprint, Schema, StableId, Store};
use game_core::Error;
use std::{
    any::TypeId,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
        self.expect_entity(id, present);
        present
    }
    pub(crate) fn expect_entity(&mut self, id: StableId, present: bool) {
        self.dependencies.push(Arc::new(Entity { id, present }));
    }
    pub(crate) fn expect_exclusive_entity(&mut self, id: StableId) {
        self.dependencies.push(Arc::new(ExclusiveEntity { id }));
    }
    pub(crate) fn expect_schema(&mut self, schema: Schema) {
        self.dependencies
            .push(Arc::new(RegistrationWitness { schema }));
    }
    /// A complete selected entity retains all logical column absences in one
    /// shape witness. Partial imports omit that check but retain the same exact
    /// publication census; unimported canonical values remain independent.
    pub(crate) fn expect_columns(&mut self, id: StableId, shape: Option<Vec<TypeId>>) {
        self.dependencies
            .push(Arc::new(EntityColumns { id, shape }));
    }
    /// Reserve an owner-exclusive new entity shape. At binding, every component
    /// edit at this identity must be a declared insertion. The resulting guard
    /// retains that exact typed footprint. Declare a combined shape explicitly
    /// when cooperating owners intentionally attach several components.
    /// Ordinary `entity` reads continue to permit shared creation.
    pub fn exclusive_entity(&mut self, store: &Store, id: StableId) -> Result<(), Error> {
        if id.0 == 0 {
            return Err(Error::InvalidConfiguration("zero stable entity ID".into()));
        }
        if store.contains(id) {
            return Err(Error::Unavailable);
        }
        self.dependencies.push(Arc::new(ExclusiveEntity { id }));
        Ok(())
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

#[derive(Clone)]
struct RegistrationWitness {
    schema: Schema,
}
impl Dependency for RegistrationWitness {
    fn validate(&self, store: &Store) -> bool {
        self.schema.same_registrations(store.schema())
    }
    fn bind(
        &self,
        _store: &Store,
        _batch: &Batch,
        _admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        Ok(Box::new(self.clone()))
    }
}
impl Projection for RegistrationWitness {
    fn validate(&self, store: &Store, _batch: &Batch) -> Result<bool, Error> {
        Ok(self.schema.same_registrations(store.schema()))
    }
}

struct EntityColumns {
    id: StableId,
    shape: Option<Vec<TypeId>>,
}
struct ProjectedColumns {
    id: StableId,
    edits: BTreeMap<TypeId, bool>,
}
impl Dependency for EntityColumns {
    fn validate(&self, store: &Store) -> bool {
        self.shape
            .as_ref()
            .is_none_or(|shape| store.backend.has_component_shape(self.id, shape))
    }
    fn bind(
        &self,
        _store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        let mut edits = BTreeMap::new();
        for edit in batch.edits_on(self.id) {
            if !admitted.rows.contains(&edit.key())
                || edits
                    .insert(edit.key().1, edit.inserted_value().is_some())
                    .is_some()
            {
                return Err(Error::Unavailable);
            }
        }
        Ok(Box::new(ProjectedColumns { id: self.id, edits }))
    }
}
impl Projection for ProjectedColumns {
    fn validate(&self, _store: &Store, batch: &Batch) -> Result<bool, Error> {
        // Shape alone cannot see absent removals, equal rewrites, or swapped
        // insertion/removal pairs. Retain the exact signed, unique key census;
        // ReadGuard's admitted edit witnesses independently retain their values.
        Ok(batch.edit_count(self.id) == self.edits.len()
            && batch.edits_on(self.id).all(|edit| {
                self.edits.get(&edit.key().1) == Some(&edit.inserted_value().is_some())
                    && batch.row_edit_count(edit.key()) == 1
            }))
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

struct ExclusiveEntity {
    id: StableId,
}
struct ProjectedShape {
    id: StableId,
    components: BTreeSet<std::any::TypeId>,
}
impl Dependency for ExclusiveEntity {
    fn validate(&self, store: &Store) -> bool {
        !store.contains(self.id)
    }
    fn bind(
        &self,
        _store: &Store,
        batch: &Batch,
        admitted: &ReadFootprint,
    ) -> Result<Box<dyn Projection>, Error> {
        let mut components = BTreeSet::new();
        for edit in batch.edits_on(self.id) {
            if edit.inserted_value().is_none()
                || !admitted.rows.contains(&edit.key())
                || !components.insert(edit.key().1)
            {
                return Err(Error::Unavailable);
            }
        }
        if components.is_empty() {
            return Err(Error::Unavailable);
        }
        Ok(Box::new(ProjectedShape {
            id: self.id,
            components,
        }))
    }
}
impl Projection for ProjectedShape {
    fn validate(&self, _store: &Store, batch: &Batch) -> Result<bool, Error> {
        Ok(batch.edit_count(self.id) == self.components.len()
            && batch.edits_on(self.id).all(|edit| {
                edit.inserted_value().is_some() && self.components.contains(&edit.key().1)
            }))
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

#[cfg(test)]
mod shape_tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Sparse<const COLUMN: usize>(u64);

    #[test]
    fn registered_absences_do_not_expand_full_entity_dependency_count() {
        let mut schema = Schema::new();
        schema.register::<u64>("present", |_| Ok(())).unwrap();
        macro_rules! absent {
            ($($column:literal),*) => {$(
                schema.register::<Sparse<$column>>(concat!("absent/", stringify!($column)), |_| Ok(())).unwrap();
            )*};
        }
        absent!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
        let mut source = Store::with_schema(schema);
        source.insert_or_spawn(StableId(1), 10_u64).unwrap();
        let opening = source.project_entities([StableId(1)]).unwrap();
        let mut preview = opening.clone();
        preview.insert(StableId(1), 11_u64).unwrap();
        let mut batch = Batch::new();
        let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
        // A schema-wide absence capture would allocate at least seventeen row
        // witnesses here. The budget covers attached values, shape, and affinity.
        assert!(reads.dependencies.len() < 8);
        let guard = reads.bind(&source, &batch, &writes).unwrap();
        guard.validate(&source, &batch).unwrap();
        source.insert(StableId(1), Sparse::<15>(15)).unwrap();
        assert_eq!(guard.validate(&source, &batch), Err(Error::Unavailable));
    }

    #[test]
    fn retained_census_rejects_operation_swaps_even_with_the_same_key_and_count() {
        let mut schema = Schema::new();
        schema.register::<u64>("number", |_| Ok(())).unwrap();
        let mut source = Store::with_schema(schema);
        source.insert_or_spawn(StableId(1), 10_u64).unwrap();
        let opening = source.project_entities([StableId(1)]).unwrap();
        let mut preview = opening.clone();
        preview.insert(StableId(1), 11_u64).unwrap();
        let mut batch = Batch::new();
        let (reads, writes) = preview.diff_into(&opening, &mut batch).unwrap();
        let guard = reads.bind(&source, &batch, &writes).unwrap();
        let census = ProjectedColumns {
            id: StableId(1),
            edits: BTreeMap::from([(TypeId::of::<u64>(), true)]),
        };
        assert_eq!(census.validate(&source, &batch), Ok(true));
        // Keep the opaque batch identity and sparse indexes, changing only the
        // operation. This exercises the census independently of value guards.
        batch.edits[0] = crate::backend::remove_edit::<u64>(StableId(1));
        assert_eq!(census.validate(&source, &batch), Ok(false));
        assert_eq!(guard.validate(&source, &batch), Err(Error::Unavailable));
    }
}

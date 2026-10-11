//! Private vendor adapter. No engine types escape the storage/query contracts.
use super::*;
use bevy_ecs::{
    component::Component as NativeComponent,
    entity::Entity,
    query::QueryState,
    world::{World, WorldId},
};
use std::{
    any::{Any, TypeId},
    sync::Arc,
};

#[derive(NativeComponent)]
struct Native<C: Component>(C);
#[derive(NativeComponent, PartialEq, Eq)]
#[component(immutable)]
struct NativeId(StableId);

type ReadRow = fn(&World, Entity) -> Result<Option<Box<RawValue>>, Error>;
type EqualRow = fn(&World, Entity, &World, Entity) -> bool;
type CloneRow = fn(&World, Entity, &mut World, Entity);
type RestoreRow = dyn Fn(&mut World, Entity, &RawValue) -> Result<(), Error> + Send + Sync;
#[derive(Clone)]
pub(super) struct Registration {
    component_type: TypeId,
    // Immutable validator callbacks are erased here. Components remain typed
    // native storage; no Any world or component bag is exposed to consumers.
    validator: Arc<dyn Any + Send + Sync>,
    initialize: fn(&mut World),
    read: ReadRow,
    equal: EqualRow,
    clone_row: CloneRow,
    restore: Arc<RestoreRow>,
}
pub(super) fn is_stable_id<C: Component>() -> bool {
    TypeId::of::<C>() == TypeId::of::<StableId>()
}
impl Registration {
    pub(super) fn matches<C: Component>(&self) -> bool {
        self.component_type == TypeId::of::<C>()
    }
    pub(super) fn same_type(&self, other: &Self) -> bool {
        self.component_type == other.component_type
    }
    pub(super) fn new<C: Component>(validate: fn(&C) -> Result<(), Error>) -> Self {
        fn initialize<C: Component>(world: &mut World) {
            world.register_component::<Native<C>>();
        }
        fn read<C: Component>(
            world: &World,
            entity: Entity,
        ) -> Result<Option<Box<RawValue>>, Error> {
            world
                .get::<Native<C>>(entity)
                .map(|component| serde_json::value::to_raw_value(&component.0))
                .transpose()
                .map_err(|error| Error::InvalidSave(format!("cannot serialize component: {error}")))
        }
        fn equal<C: Component>(a: &World, a_entity: Entity, b: &World, b_entity: Entity) -> bool {
            a.get::<Native<C>>(a_entity).map(|component| &component.0)
                == b.get::<Native<C>>(b_entity).map(|component| &component.0)
        }
        fn clone_row<C: Component>(
            source: &World,
            source_entity: Entity,
            destination: &mut World,
            destination_entity: Entity,
        ) {
            if let Some(component) = source.get::<Native<C>>(source_entity) {
                destination
                    .entity_mut(destination_entity)
                    .insert(Native(component.0.clone()));
            }
        }
        let restore = move |world: &mut World, entity: Entity, raw: &RawValue| {
            let component: C = serde_json::from_str(raw.get())
                .map_err(|error| Error::InvalidSave(format!("invalid component data: {error}")))?;
            validate(&component)
                .map_err(|error| Error::InvalidSave(format!("invalid component: {error:?}")))?;
            world.entity_mut(entity).insert(Native(component));
            Ok(())
        };
        Self {
            component_type: TypeId::of::<C>(),
            validator: Arc::new(validate),
            initialize: initialize::<C>,
            read: read::<C>,
            equal: equal::<C>,
            clone_row: clone_row::<C>,
            restore: Arc::new(restore),
        }
    }
}
// A query retains its generation allocation, so an old token cannot be
// confused with a later allocation even after arbitrarily many changes.
#[derive(Clone)]
struct Generation(Arc<()>);
impl Default for Generation {
    fn default() -> Self {
        Self(Arc::new(()))
    }
}
impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Generation {}

pub(super) struct Storage {
    world: World,
    entities: BTreeMap<StableId, Entity>,
    schema: Schema,
    structural_revision: Generation,
}
// Pending operations are erased only inside this adapter. They validate against
// typed registrations and publish into native typed columns, never a value bag.
pub(super) trait Edit: Send {
    fn key(&self) -> (StableId, TypeId);
    fn preflight(&self, store: &Storage) -> Result<bool, Error>;
    fn publish(self: Box<Self>, store: &mut Storage);
}
struct Insert<C: Component> {
    id: StableId,
    value: C,
}
impl<C: Component> Edit for Insert<C> {
    fn key(&self) -> (StableId, TypeId) {
        (self.id, TypeId::of::<C>())
    }
    fn preflight(&self, store: &Storage) -> Result<bool, Error> {
        let registration = store.schema.registration::<C>()?;
        (registration
            .validator
            .downcast_ref::<fn(&C) -> Result<(), Error>>()
            .expect("registered validator type"))(&self.value)?;
        Ok(store.get::<C>(self.id).is_none())
    }
    fn publish(self: Box<Self>, store: &mut Storage) {
        let entity = match store.entities.get(&self.id) {
            Some(entity) => *entity,
            None => {
                let entity = store.world.spawn(NativeId(self.id)).id();
                store.entities.insert(self.id, entity);
                entity
            }
        };
        store.world.entity_mut(entity).insert(Native(self.value));
    }
}
struct Remove<C: Component> {
    id: StableId,
    marker: std::marker::PhantomData<C>,
}
impl<C: Component> Edit for Remove<C> {
    fn key(&self) -> (StableId, TypeId) {
        (self.id, TypeId::of::<C>())
    }
    fn preflight(&self, store: &Storage) -> Result<bool, Error> {
        store.schema.registration::<C>()?;
        Ok(store.get::<C>(self.id).is_some())
    }
    fn publish(self: Box<Self>, store: &mut Storage) {
        if let Some(entity) = store.entities.get(&self.id) {
            store.world.entity_mut(*entity).remove::<Native<C>>();
        }
    }
}
pub(super) fn insert_edit<C: Component>(id: StableId, value: C) -> Box<dyn Edit> {
    Box::new(Insert { id, value })
}
pub(super) fn remove_edit<C: Component>(id: StableId) -> Box<dyn Edit> {
    Box::new(Remove::<C> {
        id,
        marker: std::marker::PhantomData,
    })
}
impl Storage {
    pub(super) fn new(schema: Schema) -> Self {
        let mut world = World::new();
        world.register_component::<NativeId>();
        for registration in schema.components.values() {
            (registration.initialize)(&mut world);
        }
        Self {
            world,
            entities: BTreeMap::new(),
            schema,
            structural_revision: Generation::default(),
        }
    }
    pub(super) fn schema(&self) -> &Schema {
        &self.schema
    }
    pub(super) fn len(&self) -> usize {
        self.entities.len()
    }
    pub(super) fn ids(&self) -> impl Iterator<Item = StableId> + '_ {
        self.entities.keys().copied()
    }
    pub(super) fn contains(&self, id: StableId) -> bool {
        self.entities.contains_key(&id)
    }
    pub(super) fn validate_batch(&self, edits: &[Box<dyn Edit>]) -> Result<(), Error> {
        self.batch_is_structural(edits).map(|_| ())
    }
    fn batch_is_structural(&self, edits: &[Box<dyn Edit>]) -> Result<bool, Error> {
        let mut keys = std::collections::BTreeSet::new();
        let mut structural = false;
        for edit in edits {
            let key = edit.key();
            if key.0.0 == 0 || !keys.insert(key) {
                return Err(Error::InvalidConfiguration(
                    "zero or duplicate component batch key".into(),
                ));
            }
            structural |= edit.preflight(self)?;
        }
        Ok(structural)
    }
    pub(super) fn apply_batch(&mut self, edits: Vec<Box<dyn Edit>>) -> Result<(), Error> {
        let revision = self.batch_is_structural(&edits)?.then(Generation::default);
        for edit in edits {
            edit.publish(self);
        }
        if let Some(revision) = revision {
            self.structural_revision = revision;
        }
        Ok(())
    }
    pub(super) fn spawn(&mut self, id: StableId) -> Result<(), Error> {
        if id.0 == 0 {
            return Err(Error::InvalidConfiguration("zero stable entity ID".into()));
        }
        if self.contains(id) {
            return Err(Error::SequenceConflict);
        }
        let revision = Some(Generation::default());
        let entity = self.world.spawn(NativeId(id)).id();
        self.entities.insert(id, entity);
        if let Some(revision) = revision {
            self.structural_revision = revision;
        }
        Ok(())
    }
    pub(super) fn get<C: Component>(&self, id: StableId) -> Option<&C> {
        self.world
            .get::<Native<C>>(*self.entities.get(&id)?)
            .map(|component| &component.0)
    }
    pub(super) fn get_mut<C: Component>(&mut self, id: StableId) -> Option<&mut C> {
        self.schema.registration::<C>().ok()?;
        self.world
            .get_mut::<Native<C>>(*self.entities.get(&id)?)
            .map(|component| &mut component.into_inner().0)
    }
    pub(super) fn insert<C: Component>(&mut self, id: StableId, component: C) -> Result<(), Error> {
        self.insert_component(id, component, false)
    }
    pub(super) fn insert_or_spawn<C: Component>(
        &mut self,
        id: StableId,
        component: C,
    ) -> Result<(), Error> {
        self.insert_component(id, component, true)
    }
    fn insert_component<C: Component>(
        &mut self,
        id: StableId,
        component: C,
        create_missing: bool,
    ) -> Result<(), Error> {
        let registration = self.schema.registration::<C>()?;
        let validate = registration
            .validator
            .downcast_ref::<fn(&C) -> Result<(), Error>>()
            .expect("registered validator type");
        validate(&component)?;
        let existing = self.entities.get(&id).copied();
        if id.0 == 0 {
            return Err(Error::InvalidConfiguration("zero stable entity ID".into()));
        }
        if existing.is_none() && !create_missing {
            return Err(Error::Unavailable);
        }
        let revision = existing
            .is_none_or(|entity| self.world.get::<Native<C>>(entity).is_none())
            .then(Generation::default);
        let entity = match existing {
            Some(entity) => entity,
            None => {
                let entity = self.world.spawn(NativeId(id)).id();
                self.entities.insert(id, entity);
                entity
            }
        };
        self.world.entity_mut(entity).insert(Native(component));
        if let Some(revision) = revision {
            self.structural_revision = revision;
        }
        Ok(())
    }
    pub(super) fn remove<C: Component>(&mut self, id: StableId) -> Result<Option<C>, Error> {
        self.schema.registration::<C>()?;
        let entity = *self.entities.get(&id).ok_or(Error::Unavailable)?;
        if self.world.get::<Native<C>>(entity).is_none() {
            return Ok(None);
        }
        let revision = Some(Generation::default());
        let removed = self
            .world
            .entity_mut(entity)
            .take::<Native<C>>()
            .map(|component| component.0);
        if let Some(revision) = revision {
            self.structural_revision = revision;
        }
        Ok(removed)
    }
    pub(super) fn insert_pair_or_spawn<A: Component, B: Component>(
        &mut self,
        id: StableId,
        first: A,
        second: B,
    ) -> Result<(), Error> {
        if id.0 == 0 || TypeId::of::<A>() == TypeId::of::<B>() {
            return Err(Error::InvalidConfiguration("invalid component pair".into()));
        }
        let validate = |registration: &Registration| {
            if registration.matches::<A>() {
                (registration
                    .validator
                    .downcast_ref::<fn(&A) -> Result<(), Error>>()
                    .unwrap())(&first)
            } else {
                (registration
                    .validator
                    .downcast_ref::<fn(&B) -> Result<(), Error>>()
                    .unwrap())(&second)
            }
        };
        validate(self.schema.registration::<A>()?)?;
        validate(self.schema.registration::<B>()?)?;
        let existing = self.entities.get(&id).copied();
        let structural = existing.is_none_or(|entity| {
            self.world.get::<Native<A>>(entity).is_none()
                || self.world.get::<Native<B>>(entity).is_none()
        });
        let revision = structural.then(Generation::default);
        let entity = match existing {
            Some(entity) => entity,
            None => {
                let entity = self.world.spawn(NativeId(id)).id();
                self.entities.insert(id, entity);
                entity
            }
        };
        self.world
            .entity_mut(entity)
            .insert((Native(first), Native(second)));
        if let Some(revision) = revision {
            self.structural_revision = revision;
        }
        Ok(())
    }
    pub(super) fn iter<C: Component>(&self) -> impl Iterator<Item = (StableId, &C)> {
        let mut rows = Vec::new();
        if let Some(component) = self.world.components().get_id(TypeId::of::<Native<C>>()) {
            for archetype in self
                .world
                .archetypes()
                .iter()
                .filter(|archetype| archetype.contains(component))
            {
                for row in archetype.entities() {
                    let entity = row.id();
                    let id = self
                        .world
                        .get::<NativeId>(entity)
                        .expect("registered stable identity")
                        .0;
                    let value = &self
                        .world
                        .get::<Native<C>>(entity)
                        .expect("matched native component")
                        .0;
                    rows.push((id, value));
                }
            }
        }
        rows.sort_unstable_by_key(|(id, _)| *id);
        rows.into_iter()
    }
    pub(super) fn count<C: Component>(&self) -> usize {
        let Some(component) = self.world.components().get_id(TypeId::of::<Native<C>>()) else {
            return 0;
        };
        self.world
            .archetypes()
            .iter()
            .filter(|archetype| archetype.contains(component))
            .map(|archetype| archetype.len() as usize)
            .sum()
    }
    pub(super) fn snapshot(&self) -> Result<Snapshot, Error> {
        let mut columns = BTreeMap::new();
        for (id, entity) in &self.entities {
            if self.world.get::<NativeId>(*entity) != Some(&NativeId(*id)) {
                return Err(Error::InvalidSave(
                    "inconsistent stable entity index".into(),
                ));
            }
        }
        for (key, registration) in &self.schema.components {
            let mut rows = Vec::new();
            for (id, entity) in &self.entities {
                if let Some(value) = (registration.read)(&self.world, *entity)? {
                    rows.push(ComponentRow {
                        entity: id.0,
                        value,
                    });
                }
            }
            columns.insert(key.clone(), rows);
        }
        Ok(Snapshot {
            version: 1,
            entities: self.ids().map(|id| id.0).collect(),
            columns,
        })
    }
    pub(super) fn restore(schema: Schema, snapshot: Snapshot) -> Result<Self, Error> {
        let invalid = || Error::InvalidSave("invalid component storage snapshot".into());
        if snapshot.version != 1
            || snapshot.entities.contains(&0)
            || snapshot.entities.windows(2).any(|pair| pair[0] >= pair[1])
            || snapshot.columns.keys().ne(schema.components.keys())
        {
            return Err(invalid());
        }
        let mut restored = Self::new(schema);
        for id in snapshot.entities {
            restored.spawn(StableId(id)).map_err(|_| invalid())?;
        }
        for (key, rows) in snapshot.columns {
            if rows.windows(2).any(|pair| pair[0].entity >= pair[1].entity) {
                return Err(invalid());
            }
            let registration = &restored.schema.components[&key];
            for row in rows {
                let entity = *restored
                    .entities
                    .get(&StableId(row.entity))
                    .ok_or_else(invalid)?;
                (registration.restore)(&mut restored.world, entity, &row.value)?;
            }
        }
        // No cached queries exist in a freshly restored store.
        Ok(restored)
    }
}
impl Clone for Storage {
    fn clone(&self) -> Self {
        let mut cloned = Self::new(self.schema.clone());
        for (id, source_entity) in &self.entities {
            cloned.spawn(*id).expect("stored stable IDs are valid");
            let destination_entity = cloned.entities[id];
            for registration in self.schema.components.values() {
                (registration.clone_row)(
                    &self.world,
                    *source_entity,
                    &mut cloned.world,
                    destination_entity,
                );
            }
        }
        cloned
    }
}
impl PartialEq for Storage {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.entities.keys().eq(other.entities.keys())
            && self.schema.components.values().all(|registration| {
                self.entities.iter().all(|(id, entity)| {
                    (registration.equal)(&self.world, *entity, &other.world, other.entities[id])
                })
            })
    }
}
fn cache_error() -> Error {
    Error::InvalidConfiguration("component query belongs to another store or needs refresh".into())
}
macro_rules! queries {
    ($name:ident; $($type:ident $value:ident),+) => {
        pub(super) struct $name<$($type: Component),+> {
            state: QueryState<(&'static NativeId, $(&'static Native<$type>),+)>,
            world: WorldId,
            revision: Generation,
        }
        impl<$($type: Component),+> $name<$($type),+> {
            pub(super) fn new(store: &mut Storage) -> Self {
                Self { state: store.world.query(), world: store.world.id(), revision: store.structural_revision.clone() }
            }
            pub(super) fn refresh(&mut self, store: &Storage) -> Result<(), Error> {
                if self.world != store.world.id() { return Err(cache_error()); }
                self.state.update_archetypes(&store.world);
                self.revision = store.structural_revision.clone();
                Ok(())
            }
            pub(super) fn iter<'a>(&'a self, store: &'a Storage) -> Result<impl Iterator<Item = (StableId, $(&'a $type),+)> + 'a, Error> {
                if self.world != store.world.id() || self.revision != store.structural_revision { return Err(cache_error()); }
                Ok(self.state.iter_manual(&store.world).map(|(id, $($value),+)| (id.0, $(&$value.0),+)))
            }
        }
    }
}
queries!(Query1; A a);
queries!(Query2; A a, B b);
queries!(Query3; A a, B b, C c);

type OptionalState<A, B, C> = QueryState<(
    &'static NativeId,
    &'static Native<A>,
    &'static Native<B>,
    Option<&'static Native<C>>,
)>;
pub(super) struct Query2Optional<A: Component, B: Component, C: Component> {
    state: OptionalState<A, B, C>,
    world: WorldId,
    revision: Generation,
}
impl<A: Component, B: Component, C: Component> Query2Optional<A, B, C> {
    pub(super) fn new(store: &mut Storage) -> Self {
        Self {
            state: store.world.query(),
            world: store.world.id(),
            revision: store.structural_revision.clone(),
        }
    }
    pub(super) fn refresh(&mut self, store: &Storage) -> Result<(), Error> {
        if self.world != store.world.id() {
            return Err(cache_error());
        }
        self.state.update_archetypes(&store.world);
        self.revision = store.structural_revision.clone();
        Ok(())
    }
    pub(super) fn iter<'a>(
        &'a self,
        store: &'a Storage,
    ) -> Result<impl Iterator<Item = (StableId, &'a A, &'a B, Option<&'a C>)> + 'a, Error> {
        if self.world != store.world.id() || self.revision != store.structural_revision {
            return Err(cache_error());
        }
        Ok(self
            .state
            .iter_manual(&store.world)
            .map(|(id, a, b, c)| (id.0, &a.0, &b.0, c.map(|component| &component.0))))
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[test]
    fn rejected_batch_keeps_generation_and_successful_structure_invalidates_old_queries() {
        let mut schema = Schema::new();
        schema
            .register::<u64>("test/count", |row| {
                if *row == 0 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Storage::new(schema);
        store.insert_or_spawn(StableId(1), 10_u64).unwrap();
        let mut query = Query1::<u64>::new(&mut store);
        let before = store.structural_revision.clone();
        let edits = vec![
            insert_edit(StableId(1), 20_u64),
            insert_edit(StableId(2), 0_u64),
        ];
        assert_eq!(store.apply_batch(edits), Err(Error::Unavailable));
        assert!(store.structural_revision == before);
        assert_eq!(store.get::<u64>(StableId(1)), Some(&10));
        assert!(!store.contains(StableId(2)));
        assert_eq!(query.iter(&store).unwrap().count(), 1);
        store
            .apply_batch(vec![insert_edit(StableId(1), 20_u64)])
            .unwrap();
        assert!(store.structural_revision == before);
        assert_eq!(query.iter(&store).unwrap().count(), 1);
        for id in 2..=32 {
            store
                .apply_batch(vec![insert_edit(StableId(id), id)])
                .unwrap();
            assert!(store.structural_revision != before);
            assert!(query.iter(&store).is_err());
        }
        query.refresh(&store).unwrap();
        assert_eq!(query.iter(&store).unwrap().count(), 32);
    }
}

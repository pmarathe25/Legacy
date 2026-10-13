//! Canonical character identity, authored dispositions and cumulative work.
//! Recipe progress remains Workshop-owned; this metric is personal evidence used
//! by later decisions. Runtime indexes may retain IDs, never duplicate these rows.
use game_core::{Error, InventoryId, Location, PersonId, Role};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const WORK_MINUTE: u64 = 60_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: PersonId,
    pub role: Role,
    pub inventory: InventoryId,
    pub work_units: u64,
    pub work_remainder: u64,
    pub work_rate: u64,
    pub hunger_exempt: bool,
}
impl Person {
    /// Intrinsic identity and rate carry invariants. Cargo linkage, allocation
    /// bounds and the single current ruler require supplied factual context.
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.inventory.0 == 0
            || self.id.0 == self.inventory.0
            || self.work_remainder >= WORK_MINUTE
            || self.hunger_exempt != (self.role == Role::Guardian)
        {
            Err(Error::InvalidConfiguration(
                "invalid character person".into(),
            ))
        } else {
            Ok(())
        }
    }
}
pub fn register_people(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Person>("characters/person", Person::validate)
}
pub fn person(store: &Store, actor: PersonId) -> Option<&Person> {
    store.get(StableId(actor.0))
}
pub fn person_mut(store: &mut Store, actor: PersonId) -> Option<&mut Person> {
    store.get_mut(StableId(actor.0))
}
pub fn persons(store: &Store) -> impl Iterator<Item = (PersonId, &Person)> {
    store
        .iter::<Person>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn person_ids(store: &Store) -> Vec<PersonId> {
    persons(store).map(|(id, _)| id).collect()
}
pub fn insert_person(store: &mut Store, row: Person) -> Result<(), Error> {
    row.validate()?;
    store.insert_or_spawn(StableId(row.id.0), row)
}
pub fn remove_person(store: &mut Store, actor: PersonId) -> Result<Option<Person>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}

#[derive(Clone, Copy, Debug)]
pub struct PersonContext {
    pub ruler: PersonId,
    pub next_id: u64,
}
/// The cargo port supplies only the actual holder of a person's bound inventory.
/// Receiving Characters owns membership, identity, role and binding validation.
pub fn validate_people(
    store: &Store,
    context: PersonContext,
    inventory_holder: impl Fn(InventoryId) -> Option<PersonId>,
) -> Result<(), Error> {
    let invalid = || Error::InvalidSave("inconsistent character identities".into());
    let mut rulers = 0;
    let mut inventories = BTreeSet::new();
    for (actor, row) in persons(store) {
        row.validate().map_err(|_| invalid())?;
        if actor != row.id
            || actor.0 >= context.next_id
            || row.inventory.0 >= context.next_id
            || !inventories.insert(row.inventory)
            || inventory_holder(row.inventory) != Some(actor)
        {
            return Err(invalid());
        }
        if row.role == Role::Ruler {
            rulers += 1;
            if actor != context.ruler {
                return Err(invalid());
            }
        }
    }
    if rulers != 1 || context.ruler.0 == 0 {
        return Err(invalid());
    }
    Ok(())
}

pub struct PreparedNewbornPerson {
    reads: ReadSet,
    row: Person,
}
pub fn prepare_newborn_person(
    store: &Store,
    child: PersonId,
    inventory: InventoryId,
) -> Result<PreparedNewbornPerson, Error> {
    let mut reads = ReadSet::new();
    let id = StableId(child.0);
    reads.entity(store, id);
    if reads.read::<Person>(store, id).is_some() {
        return Err(Error::Unavailable);
    }
    let row = Person {
        id: child,
        role: Role::Citizen,
        inventory,
        work_units: 0,
        work_remainder: 0,
        work_rate: 1000,
        hunger_exempt: false,
    };
    row.validate()?;
    Ok(PreparedNewbornPerson { reads, row })
}
impl PreparedNewbornPerson {
    pub fn with_role(mut self, role: Role) -> Self {
        self.row.role = role;
        self.row.hunger_exempt = role == Role::Guardian;
        self
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        self.row.validate()?;
        let id = StableId(self.row.id.0);
        if batch.touches::<Person>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Person>(id);
        batch.insert(id, self.row);
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Foreign owners retain the sparse rows determining their supplied readiness.
/// Characters reads canonical placement and chooses whether its rate applies.
#[derive(Clone, Debug)]
pub struct PhysicalWorkFacts {
    pub alive: bool,
    pub health: u64,
    pub working_activity: bool,
}
#[derive(Clone, Debug)]
pub struct WorkFacts {
    pub physical: Option<PhysicalWorkFacts>,
    pub equipment_available: bool,
    pub reads: ReadSet,
}
pub trait WorkPort {
    fn opening_reads(&self) -> ReadSet {
        ReadSet::new()
    }
    fn work_facts(&self, actor: PersonId) -> WorkFacts;
}
/// Numeric compatibility contract for the existing cumulative work benchmark.
#[derive(Clone, Copy, Debug)]
pub struct CumulativeInput {
    pub units: u64,
    pub remainder: u64,
    pub units_per_minute: u64,
    pub elapsed_ms: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CumulativeOutput {
    pub units: u64,
    pub remainder: u64,
}
pub fn cumulative_progress(input: CumulativeInput) -> Result<CumulativeOutput, Error> {
    let amount = u128::from(input.units_per_minute) * u128::from(input.elapsed_ms)
        + u128::from(input.remainder);
    Ok(CumulativeOutput {
        units: input
            .units
            .checked_add(
                u64::try_from(amount / u128::from(WORK_MINUTE)).map_err(|_| Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?,
        remainder: (amount % u128::from(WORK_MINUTE)) as u64,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressInput {
    before: Person,
    elapsed_ms: u64,
    effective_rate: u64,
}
#[derive(Clone, Debug)]
pub struct EvaluatedProgress {
    input: ProgressInput,
    units: u64,
    remainder: u64,
}
impl ProgressInput {
    pub fn actor(&self) -> PersonId {
        self.before.id
    }
    pub fn evaluate(&self) -> Result<EvaluatedProgress, Error> {
        self.before.validate()?;
        let output = cumulative_progress(CumulativeInput {
            units: self.before.work_units,
            remainder: self.before.work_remainder,
            units_per_minute: self.effective_rate,
            elapsed_ms: self.elapsed_ms,
        })?;
        Ok(EvaluatedProgress {
            input: self.clone(),
            units: output.units,
            remainder: output.remainder,
        })
    }
}
impl EvaluatedProgress {
    pub fn actor(&self) -> PersonId {
        self.input.actor()
    }
    pub fn units(&self) -> u64 {
        self.units
    }
    pub fn remainder(&self) -> u64 {
        self.remainder
    }
}
#[derive(Clone, Debug)]
pub struct ProgressInputs {
    jobs: Vec<(PersonId, ProgressInput)>,
    reads: ReadSet,
}
/// Build stable keyed work once from canonical identity rows. Personal readiness,
/// source selection and the authoritative merge remain owner operations.
pub fn progress_inputs(
    store: &Store,
    elapsed_ms: u64,
    port: &impl WorkPort,
) -> Result<ProgressInputs, Error> {
    let mut reads = port.opening_reads();
    let members = reads.membership::<Person>(store, |_, _| true);
    let mut jobs = Vec::with_capacity(members.len());
    for id in members {
        let before = reads
            .read::<Person>(store, id)
            .cloned()
            .ok_or(Error::Unavailable)?;
        before.validate()?;
        if before.id.0 != id.0 {
            return Err(Error::Unavailable);
        }
        let at_area = matches!(
            reads
                .read::<Location>(store, id)
                .ok_or(Error::Unavailable)?,
            Location::AtArea(_)
        );
        let facts = port.work_facts(before.id);
        reads.extend(facts.reads);
        let effective_rate = if at_area
            && facts
                .physical
                .as_ref()
                .is_none_or(|body| body.alive && body.health > 0 && body.working_activity)
            && facts.equipment_available
        {
            before.work_rate
        } else {
            0
        };
        jobs.push((
            before.id,
            ProgressInput {
                before,
                elapsed_ms,
                effective_rate,
            },
        ));
    }
    reads.validate(store)?;
    Ok(ProgressInputs { jobs, reads })
}
impl ProgressInputs {
    pub fn jobs(&self) -> &[(PersonId, ProgressInput)] {
        &self.jobs
    }
    pub fn evaluate(&self) -> Result<Vec<EvaluatedProgress>, Error> {
        self.jobs
            .iter()
            .map(|(_, input)| input.evaluate())
            .collect()
    }
    /// Reject stale sources, missing/duplicate/wrong evaluations and conflicting
    /// row writes before admitting a single composed projection.
    pub fn stage_into(
        self,
        store: &Store,
        results: Vec<EvaluatedProgress>,
        batch: &mut Batch,
    ) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        if results.len() != self.jobs.len() {
            return Err(Error::Unavailable);
        }
        let mut keyed = BTreeMap::new();
        for result in results {
            if keyed.insert(result.actor(), result).is_some() {
                return Err(Error::Unavailable);
            }
        }
        for (actor, input) in &self.jobs {
            if keyed.get(actor).is_none_or(|result| result.input != *input)
                || batch.touches::<Person>(StableId(actor.0))
            {
                return Err(Error::Unavailable);
            }
        }
        let mut admitted = ReadFootprint::new();
        for (actor, input) in self.jobs {
            let result = keyed
                .remove(&actor)
                .expect("checked complete keyed evaluations");
            let mut after = input.before;
            after.work_units = result.units;
            after.work_remainder = result.remainder;
            admitted.protect::<Person>(StableId(actor.0));
            batch.insert(StableId(actor.0), after);
        }
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store, results: Vec<EvaluatedProgress>) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, results, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Receiving role changes are prepared separately from the political owner's
/// selection of a successor. Identity, cargo and cumulative evidence are retained.
#[derive(Clone, Copy, Debug)]
pub struct RoleChange {
    pub actor: PersonId,
    pub role: Role,
}
pub struct PreparedRoles {
    reads: ReadSet,
    changes: Vec<Person>,
}
pub fn prepare_roles(
    store: &Store,
    changes: impl IntoIterator<Item = RoleChange>,
) -> Result<PreparedRoles, Error> {
    let mut reads = ReadSet::new();
    let mut staged = BTreeMap::new();
    for change in changes {
        if staged.contains_key(&change.actor) {
            return Err(Error::Unavailable);
        }
        let mut after = reads
            .read::<Person>(store, StableId(change.actor.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
        if after.id != change.actor {
            return Err(Error::Unavailable);
        }
        after.role = change.role;
        after.hunger_exempt = change.role == Role::Guardian;
        after.validate()?;
        staged.insert(change.actor, after);
    }
    Ok(PreparedRoles {
        reads,
        changes: staged.into_values().collect(),
    })
}
impl PreparedRoles {
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.reads.validate(store)?;
        if self
            .changes
            .iter()
            .any(|row| batch.touches::<Person>(StableId(row.id.0)))
        {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        for row in self.changes {
            admitted.protect::<Person>(StableId(row.id.0));
            batch.insert(StableId(row.id.0), row);
        }
        self.reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Readiness {
        working: bool,
        equipment: bool,
    }
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Foreign(u64);
    fn schema() -> Schema {
        let mut schema = Schema::new();
        register_people(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        schema
            .register::<Readiness>("test/readiness", |_| Ok(()))
            .unwrap();
        schema
            .register::<Foreign>("test/foreign", |_| Ok(()))
            .unwrap();
        schema
    }
    fn row(actor: u64, role: Role) -> Person {
        Person {
            id: PersonId(actor),
            role,
            inventory: InventoryId(actor + 100),
            work_units: 7,
            work_remainder: 59999,
            work_rate: 1007,
            hunger_exempt: role == Role::Guardian,
        }
    }
    fn fixture() -> Store {
        let mut store = Store::with_schema(schema());
        for (actor, role) in [(3, Role::Guardian), (1, Role::Ruler), (2, Role::Citizen)] {
            insert_person(&mut store, row(actor, role)).unwrap();
            game_spatial::set_location(
                &mut store,
                StableId(actor),
                Location::AtArea(game_core::AreaId(10)),
            )
            .unwrap();
            store
                .insert_or_spawn(
                    StableId(actor),
                    Readiness {
                        working: true,
                        equipment: true,
                    },
                )
                .unwrap();
        }
        store
    }
    struct Port<'a>(&'a Store);
    impl WorkPort for Port<'_> {
        fn work_facts(&self, actor: PersonId) -> WorkFacts {
            let mut reads = ReadSet::new();
            let row = reads.read::<Readiness>(self.0, StableId(actor.0)).unwrap();
            WorkFacts {
                physical: Some(PhysicalWorkFacts {
                    alive: true,
                    health: 1,
                    working_activity: row.working,
                }),
                equipment_available: row.equipment,
                reads,
            }
        }
    }
    fn advance(store: &mut Store, elapsed: u64, reverse: bool) {
        let prepared = progress_inputs(store, elapsed, &Port(store)).unwrap();
        let mut results = prepared.evaluate().unwrap();
        if reverse {
            results.reverse();
        }
        prepared.commit(store, results).unwrap();
    }
    #[test]
    fn native_person_schema_restores_borrowed_identity_and_preserves_foreign_rows() {
        let mut store = fixture();
        store.insert_or_spawn(StableId(1), Foreign(42)).unwrap();
        assert_eq!(
            person_ids(&store),
            vec![PersonId(1), PersonId(2), PersonId(3)]
        );
        assert!(std::ptr::eq(
            person(&store, PersonId(2)).unwrap(),
            store.get::<Person>(StableId(2)).unwrap()
        ));
        let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
        assert_eq!(restored, store);
        remove_person(&mut store, PersonId(1)).unwrap();
        assert!(store.contains(StableId(1)));
        assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(42)));
        assert!(register_people(&mut schema()).is_err());
        let mut snapshot = restored.snapshot().unwrap();
        let original = &snapshot.columns["characters/person"][0].value;
        let mut bad: serde_json::Value = serde_json::from_str(original.get()).unwrap();
        bad.as_object_mut()
            .unwrap()
            .insert("shadow".into(), serde_json::json!(1));
        snapshot.columns.get_mut("characters/person").unwrap()[0].value =
            serde_json::value::RawValue::from_string(bad.to_string()).unwrap();
        assert!(Store::restore(schema(), snapshot).is_err());
    }
    #[test]
    fn intrinsic_and_contextual_identity_checks_reject_bad_carry_role_and_bindings() {
        let store = fixture();
        let context = PersonContext {
            ruler: PersonId(1),
            next_id: 200,
        };
        validate_people(&store, context, |inventory| {
            Some(PersonId(inventory.0 - 100))
        })
        .unwrap();
        assert!(validate_people(&store, context, |_| None).is_err());
        for mut invalid in [
            row(0, Role::Citizen),
            row(1, Role::Guardian),
            row(1, Role::Citizen),
        ] {
            match invalid.id.0 {
                0 => {}
                _ if invalid.role == Role::Guardian => invalid.hunger_exempt = false,
                _ => invalid.work_remainder = 60000,
            }
            let mut store = store.clone();
            let before = store.clone();
            assert!(insert_person(&mut store, invalid).is_err());
            assert_eq!(store, before);
        }
        let mut wrong_key = store.clone();
        let mut mismatched = row(2, Role::Citizen);
        mismatched.id = PersonId(4);
        wrong_key.insert_or_spawn(StableId(2), mismatched).unwrap();
        assert!(
            validate_people(&wrong_key, context, |inventory| Some(PersonId(
                inventory.0 - 100
            )))
            .is_err()
        );
        let mut two_rulers = store.clone();
        prepare_roles(
            &two_rulers,
            [RoleChange {
                actor: PersonId(2),
                role: Role::Ruler,
            }],
        )
        .unwrap()
        .commit(&mut two_rulers)
        .unwrap();
        assert!(
            validate_people(&two_rulers, context, |inventory| Some(PersonId(
                inventory.0 - 100
            )))
            .is_err()
        );
    }
    #[test]
    fn owner_progress_is_slice_order_storage_and_resume_equivalent() {
        let mut whole = fixture();
        let mut sliced = whole.clone();
        advance(&mut whole, 1_234_567, false);
        for elapsed in [11, 59_989, 74_567, 1_100_000] {
            advance(&mut sliced, elapsed, true);
        }
        assert_eq!(sliced, whole);
        let mut resumed = Store::restore(schema(), sliced.snapshot().unwrap()).unwrap();
        advance(&mut resumed, 60_001, true);
        advance(&mut whole, 60_001, false);
        assert_eq!(resumed, whole);
        let prepared = progress_inputs(&whole, 10, &Port(&whole)).unwrap();
        assert_eq!(
            prepared
                .jobs()
                .iter()
                .map(|(actor, _)| *actor)
                .collect::<Vec<_>>(),
            person_ids(&whole)
        );
    }
    #[test]
    fn receiving_progress_owns_placement_and_physical_readiness_selection() {
        let mut store = fixture();
        game_spatial::set_location(
            &mut store,
            StableId(2),
            Location::Carried {
                carrier: PersonId(1),
            },
        )
        .unwrap();
        store.get_mut::<Readiness>(StableId(3)).unwrap().equipment = false;
        advance(&mut store, 60_000, false);
        assert_eq!(person(&store, PersonId(1)).unwrap().work_units, 1014);
        for actor in [PersonId(2), PersonId(3)] {
            let person = person(&store, actor).unwrap();
            assert_eq!(person.work_units, 7);
            assert_eq!(person.work_remainder, 59999);
        }
    }
    #[test]
    fn progress_admission_rejects_stale_reads_roster_changes_and_wrong_or_overwritten_outputs() {
        let mut store = fixture();
        let prepared = progress_inputs(&store, 60_000, &Port(&store)).unwrap();
        let results = prepared.evaluate().unwrap();
        store.get_mut::<Readiness>(StableId(2)).unwrap().working = false;
        let before = store.clone();
        assert!(prepared.commit(&mut store, results).is_err());
        assert_eq!(store, before);
        let prepared = progress_inputs(&store, 60_000, &Port(&store)).unwrap();
        let results = prepared.evaluate().unwrap();
        insert_person(&mut store, row(4, Role::Citizen)).unwrap();
        let before = store.clone();
        assert!(prepared.commit(&mut store, results).is_err());
        assert_eq!(store, before);
        let mut store = fixture();
        let first = progress_inputs(&store, 60_000, &Port(&store)).unwrap();
        let results = first.evaluate().unwrap();
        let wrong = progress_inputs(&store, 30_000, &Port(&store)).unwrap();
        let before = store.clone();
        assert!(wrong.commit(&mut store, results).is_err());
        assert_eq!(store, before);
        let prepared = progress_inputs(&store, 60_000, &Port(&store)).unwrap();
        let results = prepared.evaluate().unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&store, results, &mut batch).unwrap();
        batch.insert(StableId(90), Foreign(42));
        guard.validate(&store, &batch).unwrap();
        assert!(guard.validate(&store, &Batch::new()).is_err());
        batch.insert(StableId(2), person(&store, PersonId(2)).unwrap().clone());
        assert!(guard.validate(&store, &batch).is_err());
        assert_eq!(store, before);
    }
    #[test]
    fn overflow_is_prepublication_and_role_reactions_preserve_other_personal_evidence() {
        let mut store = fixture();
        person_mut(&mut store, PersonId(2)).unwrap().work_units = u64::MAX;
        let before = store.clone();
        let inputs = progress_inputs(&store, 60_000, &Port(&store)).unwrap();
        assert!(inputs.evaluate().is_err());
        assert_eq!(store, before);
        let previous = person(&store, PersonId(3)).unwrap().clone();
        prepare_roles(
            &store,
            [RoleChange {
                actor: PersonId(3),
                role: Role::Citizen,
            }],
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
        let current = person(&store, PersonId(3)).unwrap();
        assert_eq!(current.role, Role::Citizen);
        assert!(!current.hunger_exempt);
        assert_eq!(current.inventory, previous.inventory);
        assert_eq!(current.work_units, previous.work_units);
        assert_eq!(current.work_remainder, previous.work_remainder);
    }
}

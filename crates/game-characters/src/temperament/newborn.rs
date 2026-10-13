//! Newborn temperament is prepared before the child or its final Mind is visible.
//! The owner exposes a plain disposition override; the Mind receiver admits that
//! override with all other child fields in one native component write.
use super::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet};

pub struct PreparedNewbornTemperament {
    definitions: Definitions,
    child: PersonId,
    row: PersonTemperament,
    personality: crate::decisions::Personality,
    reads: ReadSet,
    #[cfg(feature = "genetics")]
    alleles: Vec<[i32; 2]>,
}
pub struct StagedNewbornTemperament {
    prepared: PreparedNewbornTemperament,
    guard: ReadGuard,
}
impl Runtime {
    fn newborn_baseline(
        &self,
        store: &Store,
        child: PersonId,
    ) -> Result<PreparedNewbornTemperament, Error> {
        self.definitions.validate()?;
        let mut reads = ReadSet::new();
        let id = StableId(child.0);
        reads.entity(store, id);
        if reads.read::<PersonTemperament>(store, id).is_some() {
            return Err(Error::Unavailable);
        }
        let personality = self.definitions.newborn_personality.clone();
        let row = PersonTemperament {
            baseline: Temperament::from(&personality),
            learned_offsets: [0; 4],
        };
        row.validate()?;
        Ok(PreparedNewbornTemperament {
            definitions: self.definitions.clone(),
            child,
            row,
            personality,
            reads,
            #[cfg(feature = "genetics")]
            alleles: vec![],
        })
    }
    /// Used when temperament exists without a genetic substrate.
    pub fn prepare_newborn_baseline(
        &self,
        store: &Store,
        child: PersonId,
    ) -> Result<PreparedNewbornTemperament, Error> {
        self.newborn_baseline(store, child)
    }
    #[cfg(feature = "genetics")]
    pub fn prepare_newborn_expression(
        &self,
        store: &Store,
        child: PersonId,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedNewbornTemperament, Error> {
        let mut prepared = self.newborn_baseline(store, child)?;
        prepared.alleles = self
            .definitions
            .traits
            .iter()
            .map(|definition| source.alleles_named(child, &definition.inheritance.locus))
            .collect::<Result<_, _>>()?;
        let mut pairs = prepared.alleles.iter();
        let expression = self.express_record(&prepared.row, |_| {
            let [first, second] = *pairs.next().ok_or(Error::Unavailable)?;
            Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
        })?;
        expression.apply_to(&mut prepared.personality);
        if !prepared.personality.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid newborn personality expression".into(),
            ));
        }
        Ok(prepared)
    }
}
impl PreparedNewbornTemperament {
    pub fn personality(&self) -> &crate::decisions::Personality {
        &self.personality
    }
    pub fn validate_runtime(&self, runtime: &Runtime) -> Result<(), Error> {
        if self.definitions != runtime.definitions {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
    #[cfg(feature = "genetics")]
    pub fn validate_genes(&self, source: &impl game_genetics::AlleleSource) -> Result<(), Error> {
        if self.alleles.is_empty() {
            return Ok(());
        }
        if self.alleles.len() != self.definitions.traits.len() {
            return Err(Error::Unavailable);
        }
        for (definition, expected) in self.definitions.traits.iter().zip(&self.alleles) {
            if source.alleles_named(self.child, &definition.inheritance.locus)? != *expected {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedNewbornTemperament, Error> {
        self.validate_runtime(runtime)?;
        self.reads.validate(store)?;
        let id = StableId(self.child.0);
        if batch.touches::<PersonTemperament>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<PersonTemperament>(id);
        batch.insert(id, self.row.clone());
        let guard = self.reads.clone().bind(store, batch, &admitted)?;
        Ok(StagedNewbornTemperament {
            prepared: self,
            guard,
        })
    }
}
impl StagedNewbornTemperament {
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.prepared.validate_runtime(runtime)?;
        self.guard.validate(store, batch)
    }
    #[cfg(feature = "genetics")]
    pub fn validate_genes(&self, source: &impl game_genetics::AlleleSource) -> Result<(), Error> {
        self.prepared.validate_genes(source)
    }
    pub fn native_guard(&self) -> &ReadGuard {
        &self.guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::{Aspiration, Mind, Personality, insert_mind, mind, register_minds};
    use crate::people::{person, prepare_newborn_person, register_people};
    fn fixture() -> (Runtime, Store) {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        register_minds::<u8>(&mut schema).unwrap();
        register_people(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_mind::<u8>(
            &mut store,
            PersonId(1),
            Mind::new(Personality::industrious(), Aspiration::Comfort).unwrap(),
        )
        .unwrap();
        (
            Runtime::new(super::super::tests::definitions()).unwrap(),
            store,
        )
    }
    #[test]
    fn unborn_person_mind_and_temperament_share_one_projection_without_duplicate_mind_edits() {
        let (runtime, mut store) = fixture();
        let child = PersonId(2);
        let temperament = runtime.prepare_newborn_baseline(&store, child).unwrap();
        let expected = temperament.personality().clone();
        let personal = prepare_newborn_person(&store, child, game_core::InventoryId(20))
            .unwrap()
            .with_role(game_core::Role::Ruler);
        let newborn_mind = crate::orchestration::prepare_newborn_mind::<u8>(
            &store,
            child,
            PersonId(1),
            [PersonId(1), PersonId(1)],
            game_core::SimTime(30),
            &[PersonId(1)],
        )
        .unwrap()
        .with_personality(expected.clone())
        .unwrap();
        let mut batch = Batch::new();
        let person_guard = personal.stage_into(&store, &mut batch).unwrap();
        let mind_guard = newborn_mind.stage_into(&store, &mut batch).unwrap();
        let temperament_guard = temperament
            .stage_into(&runtime, &store, &mut batch)
            .unwrap();
        person_guard.validate(&store, &batch).unwrap();
        mind_guard.validate(&store, &batch).unwrap();
        temperament_guard
            .validate(&runtime, &store, &batch)
            .unwrap();
        store.apply_batch(batch).unwrap();
        assert_eq!(person(&store, child).unwrap().role, game_core::Role::Ruler);
        assert!(!person(&store, child).unwrap().hunger_exempt);
        assert_eq!(mind::<u8>(&store, child).unwrap().personality, expected);
        assert_eq!(
            person_temperament(&store, child).unwrap().learned_offsets,
            [0; 4]
        );
    }
    #[test]
    fn newborn_temperament_rejects_late_definitions_and_destination_overwrites() {
        let (mut runtime, store) = fixture();
        let prepared = runtime
            .prepare_newborn_baseline(&store, PersonId(2))
            .unwrap();
        runtime.definitions.newborn_personality.empathy = 1;
        let before = store.clone();
        assert!(
            prepared
                .stage_into(&runtime, &store, &mut Batch::new())
                .is_err()
        );
        assert_eq!(store, before);
        let prepared = runtime
            .prepare_newborn_baseline(&store, PersonId(2))
            .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        batch.insert(
            StableId(2),
            PersonTemperament {
                baseline: Temperament::default(),
                learned_offsets: [0; 4],
            },
        );
        assert!(guard.validate(&runtime, &store, &batch).is_err());
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn newborn_expression_uses_unpublished_opaque_alleles_and_guards_them() {
        struct Source(i32);
        impl game_genetics::AlleleSource for Source {
            fn alleles_named(&self, _: PersonId, _: &str) -> Result<[i32; 2], Error> {
                Ok([self.0; 2])
            }
        }
        let (runtime, store) = fixture();
        let prepared = runtime
            .prepare_newborn_expression(&store, PersonId(2), &Source(1000))
            .unwrap();
        assert_eq!(prepared.personality().empathy, 700);
        prepared.validate_genes(&Source(1000)).unwrap();
        assert!(prepared.validate_genes(&Source(0)).is_err());
        let mut batch = Batch::new();
        let staged = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
        staged.validate(&runtime, &store, &batch).unwrap();
        staged.validate_genes(&Source(1000)).unwrap();
        assert!(staged.validate_genes(&Source(0)).is_err());
    }
}

//! Lifecycle interruption ends this owner's frozen transmission record only.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
#[derive(Clone, Debug)]
pub struct PreparedPregnancyEnd {
    pregnancy: PregnancyId,
    reads: ReadSet,
    after: Option<Inheritance>,
}
impl PreparedPregnancyEnd {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        self.reads.validate(store)
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.stage_guarded_into(store, batch).map(|_| ())
    }
    pub fn stage_guarded_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.validate(store)?;
        let mut writes = ReadFootprint::new();
        if let Some(after) = self.after {
            let id = StableId(self.pregnancy.0);
            if batch.touches::<Inheritance>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, after);
            writes.protect::<Inheritance>(id);
        }
        self.reads.bind(store, batch, &writes)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_guarded_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
impl Runtime {
    pub fn prepare_end_pregnancy(
        &self,
        store: &Store,
        pregnancy: PregnancyId,
        now: SimTime,
    ) -> PreparedPregnancyEnd {
        let mut reads = ReadSet::new();
        let before = reads
            .read::<Inheritance>(store, StableId(pregnancy.0))
            .cloned();
        let after = before
            .as_ref()
            .filter(|row| row.born.is_none() && row.ended_at.is_none() && now >= row.conceived_at)
            .map(|row| {
                let mut row = row.clone();
                row.ended_at = Some(now);
                row
            });
        PreparedPregnancyEnd {
            pregnancy,
            reads,
            after,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Runtime, Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut runtime = Runtime::new(
            Rules::default(),
            vec![LocusDefinition {
                id: LocusId("opaque".into()),
                chromosome: "a".into(),
                position: 0,
                min: 0,
                max: 1,
                initial_alleles: vec![0, 1],
                mutation_step: 1,
                mutable: false,
            }],
        )
        .unwrap();
        runtime.seed_person(&mut store, 1, PersonId(1)).unwrap();
        runtime.seed_person(&mut store, 1, PersonId(2)).unwrap();
        runtime
            .conceive(
                &mut store,
                1,
                PregnancyId(3),
                [PersonId(1), PersonId(2)],
                SimTime(1),
            )
            .unwrap();
        (runtime, store)
    }
    #[test]
    fn transmission_end_stages_with_other_rows_and_rejects_stale_or_invalid_changes() {
        let (runtime, mut store) = setup();
        let prepared = runtime.prepare_end_pregnancy(&store, PregnancyId(3), SimTime(2));
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        prepared.clone().stage_into(&store, &mut batch).unwrap();
        batch.insert(
            StableId(9),
            PersonGenome {
                genome: Genome {
                    chromosomes: Vec::new(),
                },
            },
        );
        // Genome structure is contextual to the locus schema, so use a zero key
        // as the generic foreign batch failure rather than inventing a validator.
        batch.remove::<PersonGenome>(StableId(0));
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        prepared.clone().commit(&mut store).unwrap();
        assert_eq!(
            inheritance(&store, PregnancyId(3)).unwrap().ended_at,
            Some(SimTime(2))
        );
        let before = store.snapshot().unwrap();
        assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
        assert_eq!(store.snapshot().unwrap(), before);
        runtime
            .prepare_end_pregnancy(&store, PregnancyId(3), SimTime(3))
            .commit(&mut store)
            .unwrap();
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn interruption_guard_rejects_overwritten_inheritance_before_publication() {
        let (runtime, store) = setup();
        let prepared = runtime.prepare_end_pregnancy(&store, PregnancyId(3), SimTime(2));
        let mut batch = Batch::new();
        let guard = prepared.stage_guarded_into(&store, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        assert!(guard.validate(&store, &Batch::new()).is_err());
        batch.insert(
            StableId(3),
            inheritance(&store, PregnancyId(3)).unwrap().clone(),
        );
        assert!(guard.validate(&store, &batch).is_err());
        assert_eq!(inheritance(&store, PregnancyId(3)).unwrap().ended_at, None);
    }
}

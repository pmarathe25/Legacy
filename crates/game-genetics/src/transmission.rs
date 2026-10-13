//! Owner-prepared opaque transmission and admission to the shared store.
use crate::*;
use game_ecs::{ReadFootprint, ReadGuard, ReadSet};

/// Only metadata actually consulted by this transmission is witnessed. Another
/// unrelated conception can add variants without invalidating this effect.
#[derive(Clone, Debug)]
struct RuntimeReads {
    rules: Rules,
    definitions: Vec<LocusDefinition>,
    variants: BTreeMap<VariantId, Option<Variant>>,
}
impl RuntimeReads {
    fn new(runtime: &Runtime, ids: impl IntoIterator<Item = VariantId>) -> Self {
        Self {
            rules: runtime.rules.clone(),
            definitions: runtime.definitions.clone(),
            variants: ids
                .into_iter()
                .map(|id| (id, runtime.variants.get(&id).cloned()))
                .collect(),
        }
    }
    fn validate(&self, runtime: &Runtime) -> Result<(), Error> {
        if self.rules != runtime.rules
            || self.definitions != runtime.definitions
            || self
                .variants
                .iter()
                .any(|(id, before)| runtime.variants.get(id) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PreparedConception {
    pregnancy: PregnancyId,
    after: Option<Inheritance>,
    mutations: BTreeMap<VariantId, Variant>,
    rows: ReadSet,
    metadata: RuntimeReads,
}

pub struct StagedConception {
    rows: ReadGuard,
    metadata: RuntimeReads,
    mutations: BTreeMap<VariantId, Variant>,
}
impl PreparedConception {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedConception, Error> {
        self.metadata.validate(runtime)?;
        self.rows.validate(store)?;
        let mut writes = ReadFootprint::new();
        if let Some(after) = self.after {
            let id = StableId(self.pregnancy.0);
            if batch.touches::<Inheritance>(id) {
                return Err(Error::Unavailable);
            }
            batch.insert(id, after);
            writes.protect::<Inheritance>(id);
        }
        Ok(StagedConception {
            rows: self.rows.bind(store, batch, &writes)?,
            metadata: self.metadata,
            mutations: self.mutations,
        })
    }
    pub fn commit(self, runtime: &mut Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let staged = self.stage_into(runtime, store, &mut batch)?;
        staged.validate(runtime, store, &batch)?;
        store.apply_batch(batch)?;
        staged.finish(runtime);
        Ok(())
    }
}
impl StagedConception {
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.metadata.validate(runtime)?;
        self.rows.validate(store, batch)
    }
    /// Publish prevalidated variant metadata after the shared batch succeeds.
    pub fn finish(self, runtime: &mut Runtime) {
        runtime.variants.extend(self.mutations);
    }
}

#[derive(Clone, Debug)]
pub struct PreparedBirth {
    pregnancy: PregnancyId,
    child: PersonId,
    after: Option<(Inheritance, PersonGenome)>,
    rows: ReadSet,
    metadata: RuntimeReads,
}
pub struct StagedBirth {
    rows: ReadGuard,
    metadata: RuntimeReads,
}
impl PreparedBirth {
    /// The receiving owner can interpret the inherited row before publication.
    pub fn genome(&self) -> Option<&PersonGenome> {
        self.after.as_ref().map(|(_, genome)| genome)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedBirth, Error> {
        self.metadata.validate(runtime)?;
        self.rows.validate(store)?;
        let mut writes = ReadFootprint::new();
        if let Some((record, genome)) = self.after {
            let pregnancy = StableId(self.pregnancy.0);
            let child = StableId(self.child.0);
            if batch.touches::<Inheritance>(pregnancy) || batch.touches::<PersonGenome>(child) {
                return Err(Error::Unavailable);
            }
            batch.insert(pregnancy, record);
            batch.insert(child, genome);
            writes.protect::<Inheritance>(pregnancy);
            writes.protect::<PersonGenome>(child);
        }
        Ok(StagedBirth {
            rows: self.rows.bind(store, batch, &writes)?,
            metadata: self.metadata,
        })
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let staged = self.stage_into(runtime, store, &mut batch)?;
        staged.validate(runtime, store, &batch)?;
        store.apply_batch(batch)
    }
}
impl StagedBirth {
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.metadata.validate(runtime)?;
        self.rows.validate(store, batch)
    }
}

impl Runtime {
    pub fn prepare_conception(
        &self,
        store: &Store,
        seed: u64,
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
        now: SimTime,
    ) -> Result<PreparedConception, Error> {
        if !self.rules.valid() || canonicalize(self.definitions.clone())? != self.definitions {
            return Err(configuration("invalid genetic transmission configuration"));
        }
        let mut rows = ReadSet::new();
        rows.entity(store, StableId(pregnancy.0));
        let existing = rows.read::<Inheritance>(store, StableId(pregnancy.0));
        if let Some(existing) = existing {
            return if existing.parents == parents && existing.conceived_at == now {
                Ok(PreparedConception {
                    pregnancy,
                    after: None,
                    mutations: BTreeMap::new(),
                    rows,
                    metadata: RuntimeReads::new(self, []),
                })
            } else {
                Err(Error::SequenceConflict)
            };
        }
        if parents[0] == parents[1] {
            return Err(Error::Unavailable);
        }
        let mut used = Vec::new();
        for parent in parents {
            let genome = rows
                .read::<PersonGenome>(store, StableId(parent.0))
                .ok_or(Error::Unavailable)?;
            used.extend(
                genome
                    .genome
                    .chromosomes
                    .iter()
                    .flat_map(|pair| pair.homologs.iter())
                    .flat_map(|homolog| homolog.variants.iter().copied()),
            );
        }
        let a = self.gamete(store, seed, pregnancy, 0, parents[0])?;
        let b = self.gamete(store, seed, pregnancy, 1, parents[1])?;
        let mut mutations = a.mutations;
        for (id, variant) in b.mutations {
            insert_variant(&mut mutations, id, variant)?;
        }
        used.extend(mutations.keys().copied());
        let genome = Genome {
            chromosomes: a
                .chromosomes
                .into_iter()
                .zip(b.chromosomes)
                .map(|(a, b)| ChromosomePair { homologs: [a, b] })
                .collect(),
        };
        let after = Inheritance {
            parents,
            conceived_at: now,
            genome: Some(genome),
            transmissions: [a.transmissions, b.transmissions],
            born: None,
            ended_at: None,
        };
        after.validate()?;
        Ok(PreparedConception {
            pregnancy,
            after: Some(after),
            mutations,
            rows,
            metadata: RuntimeReads::new(self, used),
        })
    }

    pub fn prepare_birth(
        &self,
        store: &Store,
        pregnancy: PregnancyId,
        child: PersonId,
        now: SimTime,
    ) -> Result<PreparedBirth, Error> {
        if !self.rules.valid() || canonicalize(self.definitions.clone())? != self.definitions {
            return Err(configuration("invalid genetic transmission configuration"));
        }
        if child.0 == pregnancy.0 {
            return Err(Error::Unavailable);
        }
        let mut rows = ReadSet::new();
        rows.entity(store, StableId(child.0));
        let record = rows
            .read::<Inheritance>(store, StableId(pregnancy.0))
            .ok_or(Error::Unavailable)?
            .clone();
        let before_child = rows.read::<PersonGenome>(store, StableId(child.0));
        if record.born == Some(child) {
            return Ok(PreparedBirth {
                pregnancy,
                child,
                after: None,
                rows,
                metadata: RuntimeReads::new(self, []),
            });
        }
        if record.born.is_some()
            || record.ended_at.is_some()
            || before_child.is_some()
            || record.parents.contains(&child)
            || now < record.conceived_at
        {
            return Err(Error::Unavailable);
        }
        let genome = record.genome.clone().ok_or(Error::Unavailable)?;
        if !self.genome_valid(&genome) {
            return Err(configuration("malformed inherited genome"));
        }
        let used = genome
            .chromosomes
            .iter()
            .flat_map(|pair| pair.homologs.iter())
            .flat_map(|homolog| homolog.variants.iter().copied());
        let metadata = RuntimeReads::new(self, used);
        let mut record = record;
        record.genome = None;
        record.born = Some(child);
        record.validate()?;
        Ok(PreparedBirth {
            pregnancy,
            child,
            after: Some((record, PersonGenome { genome })),
            rows,
            metadata,
        })
    }
}

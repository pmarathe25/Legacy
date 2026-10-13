//! Temperament predisposition and experience. Genes are optional raw evidence;
//! acquired offsets and the interpretation of that evidence belong to characters.
use game_core::{Error, PersonId};
#[cfg(feature = "genetics")]
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet};
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod newborn;
pub use newborn::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneBinding {
    pub locus: String,
    pub chromosome: String,
    pub position: u16,
    pub min: i32,
    pub max: i32,
    pub initial_alleles: Vec<i32>,
    pub mutation_step: u16,
    pub mutable: bool,
}
impl GeneBinding {
    pub fn valid(&self) -> bool {
        !self.locus.is_empty()
            && !self.chromosome.is_empty()
            && self.min == 0
            && self.max == 1_000
            && !self.initial_alleles.is_empty()
            && self
                .initial_alleles
                .iter()
                .all(|n| (self.min..=self.max).contains(n))
            && self
                .initial_alleles
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                == self.initial_alleles.len()
    }
}
#[cfg(feature = "genetics")]
impl game_genetics::HeritableCharacteristic for GeneBinding {
    fn loci(&self) -> Vec<game_genetics::LocusDefinition> {
        vec![game_genetics::LocusDefinition {
            id: game_genetics::LocusId(self.locus.clone()),
            chromosome: self.chromosome.clone(),
            position: self.position,
            min: self.min,
            max: self.max,
            initial_alleles: self.initial_alleles.clone(),
            mutation_step: self.mutation_step,
            mutable: self.mutable,
        }]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperamentTrait {
    Discipline,
    Sociability,
    Empathy,
    RiskTolerance,
}
impl TemperamentTrait {
    fn index(self) -> usize {
        match self {
            Self::Discipline => 0,
            Self::Sociability => 1,
            Self::Empathy => 2,
            Self::RiskTolerance => 3,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Temperament {
    pub discipline: u16,
    pub sociability: u16,
    pub empathy: u16,
    pub risk_tolerance: u16,
}
impl Default for Temperament {
    fn default() -> Self {
        Self {
            discipline: 500,
            sociability: 500,
            empathy: 500,
            risk_tolerance: 500,
        }
    }
}
impl Temperament {
    pub fn valid(self) -> bool {
        self.values().iter().all(|n| *n <= 1_000)
    }
    fn values(self) -> [u16; 4] {
        [
            self.discipline,
            self.sociability,
            self.empathy,
            self.risk_tolerance,
        ]
    }
    fn from_values(values: [u16; 4]) -> Self {
        Self {
            discipline: values[0],
            sociability: values[1],
            empathy: values[2],
            risk_tolerance: values[3],
        }
    }
}
impl From<&crate::decisions::Personality> for Temperament {
    fn from(p: &crate::decisions::Personality) -> Self {
        Self {
            discipline: p.discipline,
            sociability: p.sociability,
            empathy: p.empathy,
            risk_tolerance: p.risk_tolerance,
        }
    }
}
impl Temperament {
    pub fn apply_to(self, personality: &mut crate::decisions::Personality) {
        personality.discipline = self.discipline;
        personality.sociability = self.sociability;
        personality.empathy = self.empathy;
        personality.risk_tolerance = self.risk_tolerance;
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitDefinition {
    pub kind: TemperamentTrait,
    pub inheritance: GeneBinding,
    pub influence: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningDefinition {
    pub id: String,
    /// Changes to discipline, sociability, empathy and risk tolerance.
    pub deltas: [i16; 4],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub newborn_personality: crate::decisions::Personality,
    pub learned_limit: u16,
    pub traits: Vec<TraitDefinition>,
    pub learning: Vec<LearningDefinition>,
}
impl Definitions {
    pub fn validate(&self) -> Result<(), Error> {
        let mut kinds = BTreeSet::new();
        let mut loci = BTreeSet::new();
        let mut learning = BTreeSet::new();
        if !self.newborn_personality.valid()
            || self.learned_limit > 1_000
            || self.traits.iter().any(|t| {
                t.influence > 1_000
                    || !t.inheritance.valid()
                    || !kinds.insert(t.kind)
                    || !loci.insert(&t.inheritance.locus)
            })
            || self.learning.iter().any(|l| {
                l.id.is_empty()
                    || !learning.insert(&l.id)
                    || l.deltas.iter().any(|d| d.unsigned_abs() > 1_000)
            })
        {
            return Err(Error::InvalidConfiguration(
                "invalid temperament definitions".into(),
            ));
        }
        Ok(())
    }
}
#[cfg(feature = "genetics")]
impl game_genetics::HeritableCharacteristic for Definitions {
    fn loci(&self) -> Vec<game_genetics::LocusDefinition> {
        self.traits
            .iter()
            .flat_map(|t| game_genetics::HeritableCharacteristic::loci(&t.inheritance))
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonTemperament {
    pub baseline: Temperament,
    pub learned_offsets: [i16; 4],
}
impl PersonTemperament {
    pub fn validate(&self) -> Result<(), Error> {
        if !self.baseline.valid()
            || self
                .learned_offsets
                .iter()
                .any(|offset| offset.unsigned_abs() > 1_000)
        {
            return Err(Error::InvalidConfiguration(
                "invalid personal temperament".into(),
            ));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<PersonTemperament>("characters/temperament", PersonTemperament::validate)
}
pub fn person_temperament(store: &Store, person: PersonId) -> Option<&PersonTemperament> {
    store.get(StableId(person.0))
}
pub fn person_temperament_mut(
    store: &mut Store,
    person: PersonId,
) -> Option<&mut PersonTemperament> {
    store.get_mut(StableId(person.0))
}
pub fn all_person_temperaments(
    store: &Store,
) -> impl Iterator<Item = (PersonId, &PersonTemperament)> {
    store
        .iter::<PersonTemperament>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_person_temperament(
    store: &mut Store,
    person: PersonId,
    temperament: PersonTemperament,
) -> Result<(), Error> {
    temperament.validate()?;
    store.insert_or_spawn(StableId(person.0), temperament)
}
pub fn remove_person_temperament(
    store: &mut Store,
    person: PersonId,
) -> Result<Option<PersonTemperament>, Error> {
    if !store.contains(StableId(person.0)) {
        return Ok(None);
    }
    store.remove(StableId(person.0))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub definitions: Definitions,
}
impl Runtime {
    pub fn new(definitions: Definitions) -> Result<Self, Error> {
        definitions.validate()?;
        Ok(Self { definitions })
    }
    pub fn register(
        &self,
        store: &mut Store,
        person: PersonId,
        baseline: Temperament,
    ) -> Result<(), Error> {
        if !baseline.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid temperament baseline".into(),
            ));
        }
        if person_temperament(store, person).is_some() {
            return Err(Error::Unavailable);
        }
        insert_person_temperament(
            store,
            person,
            PersonTemperament {
                baseline,
                learned_offsets: [0; 4],
            },
        )?;
        Ok(())
    }
    pub fn register_newborn(
        &self,
        store: &mut Store,
        person: PersonId,
    ) -> Result<crate::decisions::Personality, Error> {
        let personality = self.definitions.newborn_personality.clone();
        self.register(store, person, Temperament::from(&personality))?;
        Ok(personality)
    }
    pub fn express(
        &self,
        store: &Store,
        person: PersonId,
        scores: &BTreeMap<String, i32>,
    ) -> Result<Temperament, Error> {
        self.express_with(store, person, |locus| {
            scores.get(locus).copied().ok_or(Error::Unavailable)
        })
    }
    fn express_with(
        &self,
        store: &Store,
        person: PersonId,
        score: impl FnMut(&str) -> Result<i32, Error>,
    ) -> Result<Temperament, Error> {
        let record = person_temperament(store, person).ok_or(Error::Unavailable)?;
        self.express_record(record, score)
    }
    fn express_record(
        &self,
        record: &PersonTemperament,
        mut score: impl FnMut(&str) -> Result<i32, Error>,
    ) -> Result<Temperament, Error> {
        let mut values = record.baseline.values().map(i32::from);
        for t in &self.definitions.traits {
            let score = score(&t.inheritance.locus)?;
            if !(0..=1_000).contains(&score) {
                return Err(Error::Unavailable);
            }
            let index = t.kind.index();
            values[index] = (values[index] * (1_000 - i32::from(t.influence))
                + score * i32::from(t.influence))
                / 1_000;
        }
        Ok(Temperament::from_values(std::array::from_fn(|i| {
            (values[i] + i32::from(record.learned_offsets[i])).clamp(0, 1_000) as u16
        })))
    }
    #[cfg(feature = "genetics")]
    pub fn express_from_genes(
        &self,
        store: &Store,
        person: PersonId,
        genes: &impl game_genetics::AlleleSource,
    ) -> Result<Temperament, Error> {
        self.express_with(store, person, |locus| {
            let alleles = genes.alleles_named(person, locus)?;
            Ok(((i64::from(alleles[0]) + i64::from(alleles[1])) / 2) as i32)
        })
    }
    pub fn learn(
        &self,
        store: &mut Store,
        person: PersonId,
        experience: &str,
    ) -> Result<(), Error> {
        let deltas = self
            .definitions
            .learning
            .iter()
            .find(|l| l.id == experience)
            .ok_or(Error::Unavailable)?
            .deltas;
        let mut record = person_temperament(store, person)
            .ok_or(Error::Unavailable)?
            .clone();
        self.apply_learning(&mut record, deltas);
        insert_person_temperament(store, person, record)?;
        Ok(())
    }
    fn apply_learning(&self, record: &mut PersonTemperament, deltas: [i16; 4]) {
        let bound = i32::from(self.definitions.learned_limit);
        for (offset, delta) in record.learned_offsets.iter_mut().zip(deltas) {
            *offset = (i32::from(*offset) + i32::from(delta)).clamp(-bound, bound) as i16;
        }
    }
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self.definitions.validate().is_err()
            || all_person_temperaments(store).any(|(_, p)| {
                !p.baseline.valid()
                    || p.learned_offsets
                        .iter()
                        .any(|n| n.unsigned_abs() > self.definitions.learned_limit)
            })
        {
            return Err(Error::InvalidSave("inconsistent temperament state".into()));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        #[cfg(feature = "genetics")]
        game_genetics::register_components(&mut schema).unwrap();
        Store::with_schema(schema)
    }
    pub(super) fn definitions() -> Definitions {
        Definitions {
            newborn_personality: crate::decisions::Personality::industrious(),
            learned_limit: 2,
            traits: vec![TraitDefinition {
                kind: TemperamentTrait::Empathy,
                influence: 400,
                inheritance: GeneBinding {
                    locus: "temperament.test".into(),
                    chromosome: "mind".into(),
                    position: 0,
                    min: 0,
                    max: 1_000,
                    initial_alleles: vec![0, 500, 1_000],
                    mutation_step: 100,
                    mutable: true,
                },
            }],
            learning: vec![LearningDefinition {
                id: "care".into(),
                deltas: [1, 0, 1, 0],
            }],
        }
    }
    #[test]
    fn experience_is_bounded_and_does_not_rewrite_inherited_evidence() {
        let mut store = store();
        let world = Runtime::new(definitions()).unwrap();
        world
            .register(&mut store, PersonId(1), Temperament::default())
            .unwrap();
        let scores = BTreeMap::from([("temperament.test".into(), 1_000)]);
        assert_eq!(
            world.express(&store, PersonId(1), &scores).unwrap().empathy,
            700
        );
        for _ in 0..10 {
            world.learn(&mut store, PersonId(1), "care").unwrap();
        }
        assert_eq!(
            world.express(&store, PersonId(1), &scores).unwrap().empathy,
            702
        );
        assert_eq!(
            person_temperament(&store, PersonId(1))
                .unwrap()
                .learned_offsets,
            [2, 0, 2, 0]
        );
        assert_eq!(scores["temperament.test"], 1_000);
        world.validate(&store).unwrap();
    }
    #[test]
    fn noninherited_dispositions_remain_with_the_character() {
        let mut store = store();
        let world = Runtime::new(Definitions {
            traits: vec![],
            ..definitions()
        })
        .unwrap();
        world
            .register(
                &mut store,
                PersonId(1),
                Temperament {
                    empathy: 900,
                    ..Temperament::default()
                },
            )
            .unwrap();
        assert_eq!(
            world
                .express(&store, PersonId(1), &BTreeMap::new())
                .unwrap()
                .empathy,
            900
        );
        let mut personality = crate::decisions::Personality::industrious();
        let ambition = personality.ambition;
        world
            .express(&store, PersonId(1), &BTreeMap::new())
            .unwrap()
            .apply_to(&mut personality);
        assert_eq!(personality.empathy, 900);
        assert_eq!(personality.ambition, ambition);
    }
    #[test]
    fn newborn_baseline_comes_from_character_content() {
        let mut store = store();
        let mut definitions = definitions();
        definitions.newborn_personality.ambition = 123;
        definitions.newborn_personality.empathy = 456;
        let world = Runtime::new(definitions).unwrap();
        let personality = world.register_newborn(&mut store, PersonId(7)).unwrap();
        assert_eq!(personality.ambition, 123);
        assert_eq!(
            person_temperament(&store, PersonId(7))
                .unwrap()
                .baseline
                .empathy,
            456
        );
        assert_eq!(
            person_temperament(&store, PersonId(7))
                .unwrap()
                .learned_offsets,
            [0; 4]
        );
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn optional_genetics_and_experience_have_separate_state() {
        let mut store = store();
        use game_genetics::HeritableCharacteristic;
        let definitions = definitions();
        let mut genes =
            game_genetics::Runtime::new(game_genetics::Rules::default(), definitions.loci())
                .unwrap();
        genes.seed_person(&mut store, 7, PersonId(1)).unwrap();
        genes
            .set_alleles(
                &mut store,
                PersonId(1),
                &game_genetics::LocusId("temperament.test".into()),
                [1_000, 1_000],
            )
            .unwrap();
        let before = genes.clone();
        let genome_before = game_genetics::person_genome(&store, PersonId(1))
            .unwrap()
            .clone();
        let mind = Runtime::new(definitions).unwrap();
        mind.register(&mut store, PersonId(1), Temperament::default())
            .unwrap();
        assert_eq!(
            mind.express_from_genes(&store, PersonId(1), &genes.allele_source(&store))
                .unwrap()
                .empathy,
            700
        );
        mind.learn(&mut store, PersonId(1), "care").unwrap();
        assert_eq!(
            mind.express_from_genes(&store, PersonId(1), &genes.allele_source(&store))
                .unwrap()
                .empathy,
            701
        );
        assert_eq!(genes, before);
        assert_eq!(
            game_genetics::person_genome(&store, PersonId(1)),
            Some(&genome_before)
        );
        let acquired_before = person_temperament(&store, PersonId(1)).unwrap().clone();
        genes
            .set_alleles(
                &mut store,
                PersonId(1),
                &game_genetics::LocusId("temperament.test".into()),
                [0, 0],
            )
            .unwrap();
        assert_eq!(
            person_temperament(&store, PersonId(1)),
            Some(&acquired_before)
        );
    }
    #[test]
    fn unknown_experience_is_inert_and_save_round_trips() {
        let mut store = store();
        let world = Runtime::new(definitions()).unwrap();
        world
            .register(&mut store, PersonId(1), Temperament::default())
            .unwrap();
        let before = (world.clone(), store.clone());
        assert!(world.learn(&mut store, PersonId(1), "missing").is_err());
        assert_eq!((world.clone(), store.clone()), before);
        let bytes = serde_json::to_vec(&world).unwrap();
        assert_eq!(serde_json::from_slice::<Runtime>(&bytes).unwrap(), world);
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Foreign(u64);

    #[test]
    fn component_schema_roundtrip_and_removal_preserve_other_owner_columns() {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        assert_eq!(
            schema.keys().collect::<Vec<_>>(),
            vec!["characters/temperament"]
        );
        assert!(register_components(&mut schema).is_err());
        schema
            .register::<Foreign>("other/column", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema.clone());
        store.insert_or_spawn(StableId(1), Foreign(77)).unwrap();
        let runtime = Runtime::new(definitions()).unwrap();
        runtime
            .register(&mut store, PersonId(1), Temperament::default())
            .unwrap();
        runtime.learn(&mut store, PersonId(1), "care").unwrap();
        assert_eq!(all_person_temperaments(&store).count(), 1);
        let restored_runtime: Runtime =
            serde_json::from_slice(&serde_json::to_vec(&runtime).unwrap()).unwrap();
        let restored = Store::restore(
            schema,
            serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(restored, store);
        restored_runtime.validate(&restored).unwrap();
        assert!(
            remove_person_temperament(&mut store, PersonId(1))
                .unwrap()
                .is_some()
        );
        assert!(store.contains(StableId(1)));
        assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(77)));
        assert!(
            remove_person_temperament(&mut store, PersonId(99))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn intrinsic_and_contextual_temperament_validation_remain_separate() {
        let runtime = Runtime::new(definitions()).unwrap();
        let mut store = store();
        let before = store.clone();
        assert!(
            insert_person_temperament(
                &mut store,
                PersonId(1),
                PersonTemperament {
                    baseline: Temperament::default(),
                    learned_offsets: [1_001; 4],
                }
            )
            .is_err()
        );
        assert!(
            runtime
                .register(&mut store, PersonId(0), Temperament::default())
                .is_err()
        );
        assert_eq!(store, before);
        insert_person_temperament(
            &mut store,
            PersonId(1),
            PersonTemperament {
                baseline: Temperament::default(),
                learned_offsets: [3; 4],
            },
        )
        .unwrap();
        let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
        assert!(matches!(
            runtime.validate(&restored),
            Err(Error::InvalidSave(_))
        ));
        person_temperament_mut(&mut store, PersonId(1))
            .unwrap()
            .baseline
            .empathy = 1_001;
        assert!(Store::restore(store.schema().clone(), store.snapshot().unwrap()).is_err());
        let mut json = serde_json::to_value(runtime).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("people".into(), serde_json::json!({}));
        assert!(serde_json::from_value::<Runtime>(json).is_err());
    }

    #[test]
    fn schema_rejection_of_learning_is_atomic() {
        let mut schema = Schema::new();
        schema
            .register::<PersonTemperament>("characters/temperament", |row| {
                row.validate()?;
                if row.learned_offsets != [0; 4] {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        let runtime = Runtime::new(definitions()).unwrap();
        runtime
            .register(&mut store, PersonId(1), Temperament::default())
            .unwrap();
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            runtime.learn(&mut store, PersonId(1), "care"),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[cfg(feature = "genetics")]
#[derive(Clone, Debug)]
struct TemperamentRow {
    actor: PersonId,
    before: PersonTemperament,
    after: PersonTemperament,
    personality: Option<crate::decisions::Personality>,
    next_personality: Option<crate::decisions::Personality>,
    alleles: Vec<[i32; 2]>,
    write_trait: bool,
}
#[cfg(feature = "genetics")]
#[derive(Clone, Debug)]
pub struct PreparedTemperament<A> {
    definitions: Definitions,
    rows: Vec<TemperamentRow>,
    factual_reads: ReadSet,
    action: std::marker::PhantomData<fn() -> A>,
}

#[cfg(feature = "genetics")]
impl Runtime {
    fn prepare_temperament_rows<A: game_ecs::Component>(
        &self,
        store: &Store,
        staged: BTreeMap<PersonId, (PersonTemperament, PersonTemperament, bool)>,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedTemperament<A>, Error> {
        let mut rows = Vec::with_capacity(staged.len());
        for (actor, (before, after, write_trait)) in staged {
            before.validate()?;
            after.validate()?;
            if after
                .learned_offsets
                .iter()
                .any(|offset| offset.unsigned_abs() > self.definitions.learned_limit)
            {
                return Err(Error::InvalidConfiguration(
                    "invalid personal temperament".into(),
                ));
            }
            let current = crate::decisions::mind::<A>(store, actor);
            if let Some(mind) = current {
                mind.validate()?;
            }
            let personality = current.map(|mind| mind.personality.clone());
            let mut next_personality = personality.clone();
            let mut alleles = Vec::new();
            if let Some(next) = &mut next_personality {
                alleles = self
                    .definitions
                    .traits
                    .iter()
                    .map(|definition| source.alleles_named(actor, &definition.inheritance.locus))
                    .collect::<Result<_, _>>()?;
                let mut pairs = alleles.iter();
                let expression = self.express_record(&after, |_| {
                    let [first, second] = *pairs.next().ok_or(Error::Unavailable)?;
                    Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
                })?;
                expression.apply_to(next);
                if !next.valid() {
                    return Err(Error::InvalidConfiguration(
                        "invalid expressed personality".into(),
                    ));
                }
            }
            rows.push(TemperamentRow {
                actor,
                before,
                after,
                personality,
                next_personality,
                alleles,
                write_trait,
            });
        }
        Ok(PreparedTemperament {
            definitions: self.definitions.clone(),
            rows,
            factual_reads: ReadSet::new(),
            action: std::marker::PhantomData,
        })
    }
    /// Experience IDs are authored content. Unknown IDs and absent personal
    /// temperament rows produce no learning or fabricated records.
    pub fn prepare_learning<A: game_ecs::Component>(
        &self,
        store: &Store,
        experiences: &[(PersonId, &str)],
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedTemperament<A>, Error> {
        self.definitions.validate()?;
        let mut staged: BTreeMap<PersonId, (PersonTemperament, PersonTemperament, bool)> =
            BTreeMap::new();
        for (actor, id) in experiences {
            let Some(definition) = self
                .definitions
                .learning
                .iter()
                .find(|definition| definition.id == *id)
            else {
                continue;
            };
            let row = match staged.entry(*actor) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let Some(before) = person_temperament(store, *actor) else {
                        continue;
                    };
                    entry.insert((before.clone(), before.clone(), true))
                }
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            };
            self.apply_learning(&mut row.1, definition.deltas);
        }
        self.prepare_temperament_rows(store, staged, source)
    }
    /// Express current substrate without inventing an experience or changing
    /// learned offsets. This also serves founder and newborn admission.
    pub fn prepare_expression<A: game_ecs::Component>(
        &self,
        store: &Store,
        actors: &[PersonId],
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedTemperament<A>, Error> {
        self.definitions.validate()?;
        let mut staged = BTreeMap::new();
        for actor in actors {
            if let Some(row) = person_temperament(store, *actor) {
                staged.insert(*actor, (row.clone(), row.clone(), false));
            }
        }
        self.prepare_temperament_rows(store, staged, source)
    }
}

#[cfg(feature = "genetics")]
impl<A: game_ecs::Component> PreparedTemperament<A> {
    pub fn with_factual_reads(mut self, reads: ReadSet) -> Self {
        self.factual_reads.extend(reads);
        self
    }
    pub fn affected(&self) -> impl Iterator<Item = PersonId> + '_ {
        self.rows.iter().map(|row| row.actor)
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<(), Error> {
        if runtime.definitions != self.definitions {
            return Err(Error::Unavailable);
        }
        self.factual_reads.validate(store)?;
        for row in &self.rows {
            if person_temperament(store, row.actor) != Some(&row.before) {
                return Err(Error::Unavailable);
            }
            row.after.validate()?;
            if row
                .after
                .learned_offsets
                .iter()
                .any(|offset| offset.unsigned_abs() > self.definitions.learned_limit)
            {
                return Err(Error::Unavailable);
            }
            let current = crate::decisions::mind::<A>(store, row.actor);
            if current.map(|mind| &mind.personality) != row.personality.as_ref() {
                return Err(Error::Unavailable);
            }
            if let Some(current) = current {
                current.validate()?;
            }
            if row
                .next_personality
                .as_ref()
                .is_some_and(|next| !next.valid())
            {
                return Err(Error::InvalidConfiguration(
                    "invalid expressed personality".into(),
                ));
            }
            for (definition, expected) in self.definitions.traits.iter().zip(&row.alleles) {
                if source.alleles_named(row.actor, &definition.inheritance.locus)? != *expected {
                    return Err(Error::Unavailable);
                }
            }
        }
        Ok(())
    }
    /// Native trait and Mind rows receive their registered schema admission
    /// together; no field update bypasses the component validators.
    pub fn commit(
        self,
        runtime: &Runtime,
        store: &mut Store,
        read_alleles: impl Fn(&Store, PersonId, &str) -> Result<[i32; 2], Error>,
    ) -> Result<(), Error> {
        struct Source<'a, F> {
            store: &'a Store,
            read: &'a F,
        }
        impl<F: Fn(&Store, PersonId, &str) -> Result<[i32; 2], Error>> game_genetics::AlleleSource
            for Source<'_, F>
        {
            fn alleles_named(&self, actor: PersonId, locus: &str) -> Result<[i32; 2], Error> {
                (self.read)(self.store, actor, locus)
            }
        }
        let mut batch = Batch::new();
        let staged = self.stage_into(
            runtime,
            store,
            &mut batch,
            &Source {
                store,
                read: &read_alleles,
            },
        )?;
        staged.validate(
            runtime,
            store,
            &batch,
            &Source {
                store,
                read: &read_alleles,
            },
        )?;
        store.apply_batch(batch)
    }
    /// Fold expressed temperament into an opaque receiving Mind builder before
    /// any Mind write is staged. Existing prepared fields retain their values.
    pub fn fold_minds(
        self,
        store: &Store,
        mut minds: crate::receivers::PreparedMindEffects<A>,
    ) -> Result<PreparedTemperamentMind<A>, Error> {
        for row in &self.rows {
            let Some(next) = &row.next_personality else {
                continue;
            };
            let mut personality = minds
                .projected_mind(row.actor)
                .or_else(|| crate::decisions::mind::<A>(store, row.actor))
                .ok_or(Error::Unavailable)?
                .personality
                .clone();
            Temperament::from(next).apply_to(&mut personality);
            minds.set_personality(store, row.actor, personality)?;
        }
        Ok(PreparedTemperamentMind {
            traits: self,
            minds,
        })
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<StagedTemperament<A>, Error> {
        self.fold_minds(store, crate::receivers::PreparedMindEffects::default())?
            .stage_into(runtime, store, batch, source)
    }
}

#[cfg(feature = "genetics")]
pub struct PreparedTemperamentMind<A> {
    traits: PreparedTemperament<A>,
    minds: crate::receivers::PreparedMindEffects<A>,
}
#[cfg(feature = "genetics")]
pub struct StagedTemperament<A> {
    definitions: Definitions,
    alleles: Vec<(PersonId, Vec<[i32; 2]>)>,
    guards: [ReadGuard; 2],
    action: std::marker::PhantomData<fn() -> A>,
}
#[cfg(feature = "genetics")]
impl<A: game_ecs::Component> PreparedTemperamentMind<A> {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<StagedTemperament<A>, Error> {
        self.traits.validate(runtime, store, source)?;
        let mut reads = self.traits.factual_reads;
        let mut admitted = ReadFootprint::new();
        for row in &self.traits.rows {
            let id = StableId(row.actor.0);
            reads.expect_row::<PersonTemperament>(id, Some(row.before.clone()));
            if row.personality.is_none() {
                reads.expect_row::<crate::decisions::Mind<A>>(id, None);
            }
            if row.write_trait {
                if batch.touches::<PersonTemperament>(id) {
                    return Err(Error::Unavailable);
                }
                batch.insert(id, row.after.clone());
                admitted.protect::<PersonTemperament>(id);
            }
        }
        let trait_guard = reads.bind(store, batch, &admitted)?;
        let mind_guard = self.minds.stage_into(store, batch)?;
        Ok(StagedTemperament {
            definitions: self.traits.definitions,
            alleles: self
                .traits
                .rows
                .into_iter()
                .map(|row| (row.actor, row.alleles))
                .collect(),
            guards: [trait_guard, mind_guard],
            action: std::marker::PhantomData,
        })
    }
}
#[cfg(feature = "genetics")]
impl<A: game_ecs::Component> StagedTemperament<A> {
    pub fn row_guards(&self) -> &[ReadGuard; 2] {
        &self.guards
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<(), Error> {
        if self.definitions != runtime.definitions {
            return Err(Error::Unavailable);
        }
        for (actor, alleles) in &self.alleles {
            for (definition, expected) in self.definitions.traits.iter().zip(alleles) {
                if source.alleles_named(*actor, &definition.inheritance.locus)? != *expected {
                    return Err(Error::Unavailable);
                }
            }
        }
        for guard in &self.guards {
            guard.validate(store, batch)?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "genetics"))]
mod system_tests;

/// Typed lived events; Characters owns their interpretation as authored learning.
#[derive(Clone, Copy, Debug)]
pub enum ExperienceEvent {
    Care {
        caregiver: PersonId,
        child: PersonId,
    },
    Company {
        first: PersonId,
        second: PersonId,
    },
    Practice {
        actor: PersonId,
    },
    Guidance {
        teacher: PersonId,
        student: PersonId,
    },
    Harm {
        target: PersonId,
    },
}
impl ExperienceEvent {
    #[cfg(feature = "genetics")]
    fn experiences(self) -> Vec<(PersonId, &'static str)> {
        match self {
            Self::Care { caregiver, child } => {
                vec![(caregiver, "gave_care"), (child, "received_care")]
            }
            Self::Company { first, second } => vec![(first, "company"), (second, "company")],
            Self::Practice { actor } => vec![(actor, "practice")],
            Self::Guidance { teacher, student } => {
                vec![(teacher, "guidance"), (student, "guidance")]
            }
            Self::Harm { target } => vec![(target, "experienced_harm")],
        }
    }
}
impl Runtime {
    pub fn register_newborn_mind<A: game_ecs::Component>(
        &self,
        store: &mut Store,
        child: PersonId,
    ) -> Result<crate::decisions::Personality, Error> {
        let personality = self.definitions.newborn_personality.clone();
        let mind_before = crate::decisions::mind::<A>(store, child).cloned();
        let mut batch = game_ecs::Batch::new();
        if person_temperament(store, child).is_some() {
            return Err(Error::Unavailable);
        }
        batch.insert(
            StableId(child.0),
            PersonTemperament {
                baseline: Temperament::from(&personality),
                learned_offsets: [0; 4],
            },
        );
        if let Some(mut mind) = mind_before {
            mind.personality = personality.clone();
            batch.insert(StableId(child.0), mind);
        }
        store.apply_batch(batch)?;
        Ok(personality)
    }
    #[cfg(feature = "genetics")]
    pub fn prepare_event<A: game_ecs::Component>(
        &self,
        store: &Store,
        event: ExperienceEvent,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedTemperament<A>, Error> {
        self.prepare_learning(store, &event.experiences(), source)
    }
    #[cfg(feature = "genetics")]
    pub fn prepare_events<A: game_ecs::Component>(
        &self,
        store: &Store,
        events: &[ExperienceEvent],
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedTemperament<A>, Error> {
        let experiences: Vec<_> = events
            .iter()
            .flat_map(|event| event.experiences())
            .collect();
        self.prepare_learning(store, &experiences, source)
    }
    #[cfg(feature = "genetics")]
    pub fn validate_references(
        &self,
        store: &Store,
        people: &BTreeSet<PersonId>,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<(), Error> {
        self.validate(store)?;
        if all_person_temperaments(store)
            .map(|(actor, _)| actor)
            .collect::<BTreeSet<_>>()
            != *people
        {
            return Err(Error::InvalidSave(
                "temperament membership differs from people".into(),
            ));
        }
        for actor in people {
            self.express_from_genes(store, *actor, source)?;
        }
        Ok(())
    }
}

#[cfg(feature = "genetics")]
pub struct PreparedFounderTemperament<A> {
    definitions: Definitions,
    actor: PersonId,
    record: PersonTemperament,
    alleles: Vec<[i32; 2]>,
    reads: ReadSet,
    minds: crate::receivers::PreparedMindEffects<A>,
}
#[cfg(feature = "genetics")]
impl Runtime {
    /// Founder baselines come from their authored native personality. Expression
    /// and the initial substrate receive one registered component admission.
    pub fn prepare_founder_expression<A: game_ecs::Component>(
        &self,
        store: &Store,
        actor: PersonId,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedFounderTemperament<A>, Error> {
        self.definitions.validate()?;
        let mut reads = ReadSet::new();
        let id = StableId(actor.0);
        if reads.read::<PersonTemperament>(store, id).is_some() {
            return Err(Error::Unavailable);
        }
        let mind = crate::decisions::mind::<A>(store, actor).ok_or(Error::Unavailable)?;
        mind.validate()?;
        let record = PersonTemperament {
            baseline: Temperament::from(&mind.personality),
            learned_offsets: [0; 4],
        };
        let alleles = self
            .definitions
            .traits
            .iter()
            .map(|definition| source.alleles_named(actor, &definition.inheritance.locus))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pairs = alleles.iter();
        let expression = self.express_record(&record, |_| {
            let [first, second] = *pairs.next().ok_or(Error::Unavailable)?;
            Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
        })?;
        let mut personality = mind.personality.clone();
        expression.apply_to(&mut personality);
        let mut minds = crate::receivers::PreparedMindEffects::default();
        minds.set_personality(store, actor, personality)?;
        Ok(PreparedFounderTemperament {
            definitions: self.definitions.clone(),
            actor,
            record,
            alleles,
            reads,
            minds,
        })
    }
}
#[cfg(feature = "genetics")]
impl<A: game_ecs::Component> PreparedFounderTemperament<A> {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<StagedTemperament<A>, Error> {
        if self.definitions != runtime.definitions {
            return Err(Error::Unavailable);
        }
        for (definition, expected) in self.definitions.traits.iter().zip(&self.alleles) {
            if source.alleles_named(self.actor, &definition.inheritance.locus)? != *expected {
                return Err(Error::Unavailable);
            }
        }
        self.reads.validate(store)?;
        let id = StableId(self.actor.0);
        if batch.touches::<PersonTemperament>(id) {
            return Err(Error::Unavailable);
        }
        let mut admitted = ReadFootprint::new();
        admitted.protect::<PersonTemperament>(id);
        batch.insert(id, self.record);
        let traits = self.reads.bind(store, batch, &admitted)?;
        let minds = self.minds.stage_into(store, batch)?;
        Ok(StagedTemperament {
            definitions: self.definitions,
            alleles: vec![(self.actor, self.alleles)],
            guards: [traits, minds],
            action: std::marker::PhantomData,
        })
    }
}

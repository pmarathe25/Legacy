//! Trait-independent diploid inheritance, linked crossover, and mutation.
//! Trait owners supply locus definitions and interpret raw allele values.
use game_core::{Error, PersonId, PregnancyId, SimTime};
use game_ecs::{Batch, StableId, Store};
mod references;
mod storage;
pub use references::{PregnancyReference, TransmissionStage};
mod transmission;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use storage::*;
pub use transmission::{PreparedBirth, PreparedConception, StagedBirth, StagedConception};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocusId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocusDefinition {
    pub id: LocusId,
    pub chromosome: String,
    pub position: u16,
    pub min: i32,
    pub max: i32,
    pub initial_alleles: Vec<i32>,
    pub mutation_step: u16,
    pub mutable: bool,
}

/// Implemented by owners of inheritable characteristics. Values remain opaque
/// to genetics; expression, development, and eligibility belong to those owners.
pub trait HeritableCharacteristic {
    fn loci(&self) -> Vec<LocusDefinition>;
}

/// A trait owner's read interface to opaque allele values.
pub trait AlleleSource {
    fn alleles_named(&self, person: PersonId, locus: &str) -> Result<[i32; 2], Error>;
}

/// Borrows the genetics runtime and the host's shared component store.
pub struct AlleleReader<'a> {
    runtime: &'a Runtime,
    store: &'a Store,
}

impl AlleleSource for AlleleReader<'_> {
    fn alleles_named(&self, person: PersonId, locus: &str) -> Result<[i32; 2], Error> {
        self.runtime.alleles_named(self.store, person, locus)
    }
}

/// Reads one prepared person row before it is admitted to the shared store.
pub struct GenomeReader<'a> {
    runtime: &'a Runtime,
    person: PersonId,
    genome: &'a PersonGenome,
}
impl AlleleSource for GenomeReader<'_> {
    fn alleles_named(&self, person: PersonId, locus: &str) -> Result<[i32; 2], Error> {
        if person != self.person {
            return Err(Error::Unavailable);
        }
        self.runtime.genome_alleles_named(self.genome, locus)
    }
}

/// Plain founder data and standing variants staged for one host admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFounder {
    pub genome: PersonGenome,
    pub variants: BTreeMap<VariantId, Variant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VariantId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub locus: LocusId,
    pub value: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chromosome {
    pub variants: Vec<VariantId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromosomePair {
    pub homologs: [Chromosome; 2],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genome {
    pub chromosomes: Vec<ChromosomePair>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Mutation probability per transmitted allele, on a 0..10,000 scale.
    pub mutation_chance: u16,
    /// Crossover probability between linked loci, on a 0..1,000 scale.
    pub crossover_chance: u16,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            mutation_chance: 10,
            crossover_chance: 350,
        }
    }
}
impl Rules {
    pub fn valid(&self) -> bool {
        self.mutation_chance <= 10_000 && self.crossover_chance <= 1_000
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonGenome {
    pub genome: Genome,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transmission {
    pub locus: LocusId,
    pub source_homolog: u8,
    pub source_variant: VariantId,
    pub transmitted_variant: VariantId,
    pub crossed_before: bool,
    pub mutated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inheritance {
    pub parents: [PersonId; 2],
    pub conceived_at: SimTime,
    pub genome: Option<Genome>,
    pub transmissions: [Vec<Transmission>; 2],
    pub born: Option<PersonId>,
    pub ended_at: Option<SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
    pub definitions: Vec<LocusDefinition>,
    pub variants: BTreeMap<VariantId, Variant>,
}

fn configuration(message: &str) -> Error {
    Error::InvalidConfiguration(message.into())
}
fn canonicalize(mut definitions: Vec<LocusDefinition>) -> Result<Vec<LocusDefinition>, Error> {
    let mut ids = BTreeSet::new();
    let mut positions = BTreeSet::new();
    for definition in &mut definitions {
        if definition.id.0.trim().is_empty()
            || definition.id.0.trim() != definition.id.0
            || definition.chromosome.trim().is_empty()
            || definition.chromosome.trim() != definition.chromosome
            || !ids.insert(definition.id.clone())
            || !positions.insert((definition.chromosome.clone(), definition.position))
            || definition.min > definition.max
            || definition.initial_alleles.is_empty()
            || definition.mutable && definition.mutation_step == 0
            || definition
                .initial_alleles
                .iter()
                .any(|value| *value < definition.min || *value > definition.max)
        {
            return Err(configuration(
                "invalid or duplicate genetic locus definition",
            ));
        }
        definition.initial_alleles.sort_unstable();
        if definition
            .initial_alleles
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(configuration("duplicate initial allele"));
        }
    }
    definitions.sort_by(|a, b| {
        (&a.chromosome, a.position, &a.id).cmp(&(&b.chromosome, b.position, &b.id))
    });
    Ok(definitions)
}
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
fn name_key(name: &str) -> u64 {
    name.as_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}
fn key(
    seed: u64,
    identity: u64,
    side: u64,
    chromosome: &str,
    locus: &LocusId,
    purpose: u64,
) -> u64 {
    mix(seed
        ^ mix(identity)
        ^ mix(side.wrapping_add(0x9e3779b97f4a7c15))
        ^ mix(name_key(chromosome))
        ^ mix(name_key(&locus.0))
        ^ mix(purpose))
}
fn standing_variant(locus: &LocusId, value: i32) -> VariantId {
    VariantId((mix(name_key(&locus.0) ^ mix(value as u32 as u64)) & !(1 << 63)).max(1))
}
fn insert_variant(
    variants: &mut BTreeMap<VariantId, Variant>,
    id: VariantId,
    value: Variant,
) -> Result<(), Error> {
    if variants.get(&id).is_some_and(|existing| existing != &value) {
        return Err(configuration("genetic variant identity collision"));
    }
    variants.insert(id, value);
    Ok(())
}
struct Gamete {
    chromosomes: Vec<Chromosome>,
    transmissions: Vec<Transmission>,
    mutations: BTreeMap<VariantId, Variant>,
}
impl Runtime {
    pub fn allele_source<'a>(&'a self, store: &'a Store) -> AlleleReader<'a> {
        AlleleReader {
            runtime: self,
            store,
        }
    }

    pub fn genome_source<'a>(
        &'a self,
        person: PersonId,
        genome: &'a PersonGenome,
    ) -> GenomeReader<'a> {
        GenomeReader {
            runtime: self,
            person,
            genome,
        }
    }

    pub fn new(rules: Rules, definitions: Vec<LocusDefinition>) -> Result<Self, Error> {
        if !rules.valid() {
            return Err(configuration("invalid genetic rules"));
        }
        let definitions = canonicalize(definitions)?;
        let mut variants = BTreeMap::new();
        for definition in &definitions {
            for value in &definition.initial_alleles {
                insert_variant(
                    &mut variants,
                    standing_variant(&definition.id, *value),
                    Variant {
                        locus: definition.id.clone(),
                        value: *value,
                    },
                )?;
            }
        }
        Ok(Self {
            rules,
            definitions,
            variants,
        })
    }
    fn groups(&self) -> Vec<&[LocusDefinition]> {
        let mut groups = Vec::new();
        let mut start = 0;
        while start < self.definitions.len() {
            let end = start
                + self.definitions[start..]
                    .iter()
                    .take_while(|definition| {
                        definition.chromosome == self.definitions[start].chromosome
                    })
                    .count();
            groups.push(&self.definitions[start..end]);
            start = end;
        }
        groups
    }
    fn location(&self, locus: &LocusId) -> Result<(usize, usize, &LocusDefinition), Error> {
        self.location_named(&locus.0)
    }
    fn location_named(&self, locus: &str) -> Result<(usize, usize, &LocusDefinition), Error> {
        let mut chromosome = 0;
        let mut position = 0;
        let mut previous_chromosome = None;
        for definition in &self.definitions {
            if let Some(previous) = previous_chromosome
                && previous != definition.chromosome.as_str()
            {
                chromosome += 1;
                position = 0;
            }
            if definition.id.0 == locus {
                return Ok((chromosome, position, definition));
            }
            previous_chromosome = Some(definition.chromosome.as_str());
            position += 1;
        }
        Err(Error::Unavailable)
    }
    pub fn seed_person(&self, store: &mut Store, seed: u64, person: PersonId) -> Result<(), Error> {
        if person_genome(store, person).is_some() {
            return Err(configuration("duplicate genetic person"));
        }
        insert_person_genome(store, person, self.seed_genome(seed, person))?;
        Ok(())
    }

    fn seed_genome(&self, seed: u64, person: PersonId) -> PersonGenome {
        let chromosomes = self
            .groups()
            .into_iter()
            .map(|definitions| ChromosomePair {
                homologs: std::array::from_fn(|side| Chromosome {
                    variants: definitions
                        .iter()
                        .map(|definition| {
                            let count = definition.initial_alleles.len() as u64;
                            // Two independent draws bias the index toward its center without
                            // assuming any semantic meaning for the allele values.
                            let first = key(
                                seed,
                                person.0,
                                side as u64,
                                &definition.chromosome,
                                &definition.id,
                                1,
                            ) % count.div_ceil(2);
                            let second = key(
                                seed,
                                person.0,
                                side as u64,
                                &definition.chromosome,
                                &definition.id,
                                2,
                            ) % (count / 2 + 1);
                            let index = first + second;
                            standing_variant(
                                &definition.id,
                                definition.initial_alleles[index as usize],
                            )
                        })
                        .collect(),
                }),
            })
            .collect();
        PersonGenome {
            genome: Genome { chromosomes },
        }
    }

    /// Prepares a founder without changing runtime metadata or host rows.
    pub fn prepare_founder(
        &self,
        seed: u64,
        person: PersonId,
        assignments: &[(LocusId, [i32; 2])],
    ) -> Result<PreparedFounder, Error> {
        if person.0 == 0 {
            return Err(configuration("invalid genetic founder identity"));
        }
        let mut prepared = PreparedFounder {
            genome: self.seed_genome(seed, person),
            variants: BTreeMap::new(),
        };
        for (locus, values) in assignments {
            self.assign_alleles(&mut prepared.genome, locus, *values, &mut prepared.variants)?;
        }
        prepared.genome.validate()?;
        Ok(prepared)
    }

    /// Folds a prepared delta into an unpublished runtime. Every admission
    /// guard runs before any registry write, including guards for forged data.
    pub fn apply_founder_variants(&mut self, prepared: &PreparedFounder) -> Result<(), Error> {
        prepared.genome.validate()?;
        for (id, variant) in &prepared.variants {
            let (_, _, definition) = self.location(&variant.locus)?;
            if variant.value < definition.min
                || variant.value > definition.max
                || *id != standing_variant(&variant.locus, variant.value)
            {
                return Err(configuration("invalid standing founder variant"));
            }
            if self
                .variants
                .get(id)
                .is_some_and(|existing| existing != variant)
            {
                return Err(configuration("genetic variant identity collision"));
            }
        }
        if !self.genome_valid_with(&prepared.genome.genome, Some(&prepared.variants)) {
            return Err(configuration("malformed prepared founder genome"));
        }
        self.variants.extend(prepared.variants.clone());
        Ok(())
    }
    /// Sets explicit founder substrate before that genome has participated in
    /// reproduction. Acquired characteristics never update these values.
    pub fn set_alleles(
        &mut self,
        store: &mut Store,
        person: PersonId,
        locus: &LocusId,
        values: [i32; 2],
    ) -> Result<(), Error> {
        if all_inheritances(store).any(|(_, inheritance)| {
            inheritance.parents.contains(&person) || inheritance.born == Some(person)
        }) {
            return Err(Error::Unavailable);
        }
        let (_, _, definition) = self.location(locus)?;
        if values
            .iter()
            .any(|value| *value < definition.min || *value > definition.max)
        {
            return Err(configuration("founder allele outside locus bounds"));
        }
        let mut record = person_genome(store, person)
            .ok_or(Error::Unavailable)?
            .clone();
        let mut additions = BTreeMap::new();
        self.assign_alleles(&mut record, locus, values, &mut additions)?;
        insert_person_genome(store, person, record)?;
        self.variants.extend(additions);
        Ok(())
    }

    fn assign_alleles(
        &self,
        record: &mut PersonGenome,
        locus: &LocusId,
        values: [i32; 2],
        additions: &mut BTreeMap<VariantId, Variant>,
    ) -> Result<(), Error> {
        let (chromosome, position, definition) = self.location(locus)?;
        if values
            .iter()
            .any(|value| *value < definition.min || *value > definition.max)
        {
            return Err(configuration("founder allele outside locus bounds"));
        }
        self.genome_allele_ids_named(record, &locus.0)?;
        let ids = values.map(|value| standing_variant(locus, value));
        for (id, value) in ids.into_iter().zip(values) {
            let variant = Variant {
                locus: locus.clone(),
                value,
            };
            if self
                .variants
                .get(&id)
                .is_some_and(|existing| existing != &variant)
            {
                return Err(configuration("genetic variant identity collision"));
            }
            insert_variant(additions, id, variant)?;
        }
        for (side, id) in ids.into_iter().enumerate() {
            record.genome.chromosomes[chromosome].homologs[side].variants[position] = id;
        }
        Ok(())
    }
    pub fn allele_ids(
        &self,
        store: &Store,
        person: PersonId,
        locus: &LocusId,
    ) -> Result<[VariantId; 2], Error> {
        self.allele_ids_named(store, person, &locus.0)
    }
    /// Reads an owner's borrowed locus name without allocating an identifier.
    pub fn allele_ids_named(
        &self,
        store: &Store,
        person: PersonId,
        locus: &str,
    ) -> Result<[VariantId; 2], Error> {
        let (chromosome, position, _) = self.location_named(locus)?;
        Self::genome_ids_at(
            person_genome(store, person).ok_or(Error::Unavailable)?,
            chromosome,
            position,
        )
    }
    fn genome_allele_ids_named(
        &self,
        record: &PersonGenome,
        locus: &str,
    ) -> Result<[VariantId; 2], Error> {
        let (chromosome, position, _) = self.location_named(locus)?;
        Self::genome_ids_at(record, chromosome, position)
    }
    fn genome_ids_at(
        record: &PersonGenome,
        chromosome: usize,
        position: usize,
    ) -> Result<[VariantId; 2], Error> {
        let pair = record
            .genome
            .chromosomes
            .get(chromosome)
            .ok_or_else(|| configuration("malformed person genome"))?;
        let mut ids = [VariantId(0); 2];
        for (side, id) in ids.iter_mut().enumerate() {
            *id = *pair.homologs[side]
                .variants
                .get(position)
                .ok_or_else(|| configuration("malformed person genome"))?;
        }
        Ok(ids)
    }
    pub fn alleles(
        &self,
        store: &Store,
        person: PersonId,
        locus: &LocusId,
    ) -> Result<[i32; 2], Error> {
        self.alleles_named(store, person, &locus.0)
    }
    /// Returns opaque allele values for an owner's borrowed locus name.
    pub fn alleles_named(
        &self,
        store: &Store,
        person: PersonId,
        locus: &str,
    ) -> Result<[i32; 2], Error> {
        self.allele_values(self.allele_ids_named(store, person, locus)?, locus)
    }
    fn genome_alleles_named(&self, record: &PersonGenome, locus: &str) -> Result<[i32; 2], Error> {
        let ids = self.genome_allele_ids_named(record, locus)?;
        self.allele_values(ids, locus)
    }
    fn allele_values(&self, ids: [VariantId; 2], locus: &str) -> Result<[i32; 2], Error> {
        let mut values = [0; 2];
        for (id, value) in ids.into_iter().zip(&mut values) {
            let variant = self
                .variants
                .get(&id)
                .filter(|variant| variant.locus.0 == locus)
                .ok_or_else(|| configuration("unknown or mismatched genetic variant"))?;
            *value = variant.value;
        }
        Ok(values)
    }
    fn gamete(
        &self,
        store: &Store,
        seed: u64,
        pregnancy: PregnancyId,
        side: usize,
        person: PersonId,
    ) -> Result<Gamete, Error> {
        let parent = &person_genome(store, person)
            .ok_or(Error::Unavailable)?
            .genome;
        if !self.genome_valid(parent) {
            return Err(configuration("malformed parental genome"));
        }
        let mut result = Gamete {
            chromosomes: Vec::new(),
            transmissions: Vec::new(),
            mutations: BTreeMap::new(),
        };
        for (chromosome, definitions) in self.groups().into_iter().enumerate() {
            let first = &definitions[0];
            let mut homolog = (key(
                seed,
                pregnancy.0,
                side as u64,
                &first.chromosome,
                &first.id,
                3,
            ) % 2) as usize;
            let mut transmitted = Vec::new();
            for (position, definition) in definitions.iter().enumerate() {
                let draw = |purpose| {
                    key(
                        seed,
                        pregnancy.0,
                        side as u64,
                        &definition.chromosome,
                        &definition.id,
                        purpose,
                    )
                };
                let crossed =
                    position > 0 && draw(4) % 1_000 < u64::from(self.rules.crossover_chance);
                if crossed {
                    homolog ^= 1;
                }
                let source = parent.chromosomes[chromosome].homologs[homolog].variants[position];
                let variant = &self.variants[&source];
                let mutated =
                    definition.mutable && draw(5) % 10_000 < u64::from(self.rules.mutation_chance);
                let target = if mutated {
                    let delta = (draw(6) % (u64::from(definition.mutation_step) * 2 + 1)) as i64
                        - i64::from(definition.mutation_step);
                    let id = VariantId((draw(7) ^ mix(source.0)) | 1 << 63);
                    if id == source {
                        return Err(configuration("genetic variant identity collision"));
                    }
                    let value = Variant {
                        locus: definition.id.clone(),
                        value: (i64::from(variant.value) + delta)
                            .clamp(i64::from(definition.min), i64::from(definition.max))
                            as i32,
                    };
                    if self
                        .variants
                        .get(&id)
                        .is_some_and(|existing| existing != &value)
                    {
                        return Err(configuration("genetic variant identity collision"));
                    }
                    insert_variant(&mut result.mutations, id, value)?;
                    id
                } else {
                    source
                };
                transmitted.push(target);
                result.transmissions.push(Transmission {
                    locus: definition.id.clone(),
                    source_homolog: homolog as u8,
                    source_variant: source,
                    transmitted_variant: target,
                    crossed_before: crossed,
                    mutated,
                });
            }
            result.chromosomes.push(Chromosome {
                variants: transmitted,
            });
        }
        Ok(result)
    }
    pub fn conceive(
        &mut self,
        store: &mut Store,
        seed: u64,
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
        now: SimTime,
    ) -> Result<(), Error> {
        self.prepare_conception(store, seed, pregnancy, parents, now)?
            .commit(self, store)
    }
    pub fn born(
        &mut self,
        store: &mut Store,
        pregnancy: PregnancyId,
        child: PersonId,
        now: SimTime,
    ) -> Result<(), Error> {
        self.prepare_birth(store, pregnancy, child, now)?
            .commit(self, store)
    }
    pub fn end_pregnancy(&self, store: &mut Store, pregnancy: PregnancyId, now: SimTime) {
        self.prepare_end_pregnancy(store, pregnancy, now)
            .commit(store)
            .expect("immediately prepared genetic interruption");
    }
    fn genome_valid(&self, genome: &Genome) -> bool {
        self.genome_valid_with(genome, None)
    }
    fn genome_valid_with(
        &self,
        genome: &Genome,
        additions: Option<&BTreeMap<VariantId, Variant>>,
    ) -> bool {
        let groups = self.groups();
        genome.chromosomes.len() == groups.len()
            && genome
                .chromosomes
                .iter()
                .zip(groups)
                .all(|(pair, definitions)| {
                    pair.homologs.iter().all(|homolog| {
                        homolog.variants.len() == definitions.len()
                            && homolog
                                .variants
                                .iter()
                                .zip(definitions)
                                .all(|(id, definition)| {
                                    additions
                                        .and_then(|variants| variants.get(id))
                                        .or_else(|| self.variants.get(id))
                                        .is_some_and(|variant| {
                                            variant.locus == definition.id
                                                && variant.value >= definition.min
                                                && variant.value <= definition.max
                                        })
                                })
                    })
                })
    }
}

pub fn validate(world: &Runtime, store: &Store, now: SimTime) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent genetic state".into());
    if !world.rules.valid()
        || canonicalize(world.definitions.clone()).map_err(|_| fail())? != world.definitions
    {
        return Err(fail());
    }
    let definitions: BTreeMap<_, _> = world
        .definitions
        .iter()
        .map(|definition| (&definition.id, definition))
        .collect();
    for (id, variant) in &world.variants {
        let definition = definitions.get(&variant.locus).ok_or_else(fail)?;
        if id.0 == 0 || variant.value < definition.min || variant.value > definition.max {
            return Err(fail());
        }
    }
    for definition in &world.definitions {
        for value in &definition.initial_alleles {
            if world
                .variants
                .get(&standing_variant(&definition.id, *value))
                != Some(&Variant {
                    locus: definition.id.clone(),
                    value: *value,
                })
            {
                return Err(fail());
            }
        }
    }
    if all_person_genomes(store).any(|(_, record)| !world.genome_valid(&record.genome)) {
        return Err(fail());
    }
    let mut children = BTreeSet::new();
    for (pregnancy, inheritance) in all_inheritances(store) {
        if inheritance.born.is_some_and(|child| child.0 == pregnancy.0)
            || inheritance.parents[0] == inheritance.parents[1]
            || inheritance.conceived_at > now
            || inheritance.ended_at.is_some_and(|ended| {
                ended < inheritance.conceived_at || ended > now || inheritance.born.is_some()
            })
            || inheritance
                .parents
                .iter()
                .any(|parent| person_genome(store, *parent).is_none())
        {
            return Err(fail());
        }
        let genome = match (&inheritance.genome, inheritance.born) {
            (Some(genome), None) => genome,
            (None, Some(child))
                if !inheritance.parents.contains(&child) && children.insert(child) =>
            {
                &person_genome(store, child).ok_or_else(fail)?.genome
            }
            _ => return Err(fail()),
        };
        if !world.genome_valid(genome) {
            return Err(fail());
        }
        for (side, traces) in inheritance.transmissions.iter().enumerate() {
            if traces.len() != world.definitions.len() {
                return Err(fail());
            }
            let parent = &person_genome(store, inheritance.parents[side])
                .ok_or_else(fail)?
                .genome;
            let mut index = 0;
            for (chromosome, group) in world.groups().into_iter().enumerate() {
                let mut previous = None;
                for (position, definition) in group.iter().enumerate() {
                    let trace = &traces[index];
                    index += 1;
                    let source = world.variants.get(&trace.source_variant).ok_or_else(fail)?;
                    let target = world
                        .variants
                        .get(&trace.transmitted_variant)
                        .ok_or_else(fail)?;
                    if trace.locus != definition.id
                        || trace.source_homolog > 1
                        || source.locus != trace.locus
                        || target.locus != trace.locus
                        || position == 0 && trace.crossed_before
                        || position > 0
                            && previous
                                != Some(trace.source_homolog ^ u8::from(trace.crossed_before))
                        || trace.crossed_before && world.rules.crossover_chance == 0
                        || position > 0
                            && !trace.crossed_before
                            && world.rules.crossover_chance == 1_000
                        || parent.chromosomes[chromosome].homologs
                            [usize::from(trace.source_homolog)]
                        .variants[position]
                            != trace.source_variant
                        || genome.chromosomes[chromosome].homologs[side].variants[position]
                            != trace.transmitted_variant
                        || !trace.mutated && trace.source_variant != trace.transmitted_variant
                        || trace.mutated
                            && (!definition.mutable
                                || world.rules.mutation_chance == 0
                                || trace.source_variant == trace.transmitted_variant
                                || trace.transmitted_variant.0 & (1 << 63) == 0
                                || i64::from(source.value).abs_diff(i64::from(target.value))
                                    > u64::from(definition.mutation_step))
                        || definition.mutable
                            && world.rules.mutation_chance == 10_000
                            && !trace.mutated
                    {
                        return Err(fail());
                    }
                    previous = Some(trace.source_homolog);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

mod ending;
pub use ending::PreparedPregnancyEnd;

//! Trait-independent diploid inheritance, linked crossover, and mutation.
//! Trait owners supply locus definitions and interpret raw allele values.
use game_core::{Error, PersonId, PregnancyId, SimTime};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
pub struct Inheritance {
    pub parents: [PersonId; 2],
    pub conceived_at: SimTime,
    pub genome: Option<Genome>,
    pub transmissions: [Vec<Transmission>; 2],
    pub born: Option<PersonId>,
    pub ended_at: Option<SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub definitions: Vec<LocusDefinition>,
    pub variants: BTreeMap<VariantId, Variant>,
    pub people: BTreeMap<PersonId, PersonGenome>,
    pub pregnancies: BTreeMap<PregnancyId, Inheritance>,
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
impl World {
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
            people: BTreeMap::new(),
            pregnancies: BTreeMap::new(),
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
    pub fn seed_person(&mut self, seed: u64, person: PersonId) -> Result<(), Error> {
        if self.people.contains_key(&person) {
            return Err(configuration("duplicate genetic person"));
        }
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
        self.people.insert(
            person,
            PersonGenome {
                genome: Genome { chromosomes },
            },
        );
        Ok(())
    }
    /// Sets explicit founder substrate before that genome has participated in
    /// reproduction. Acquired characteristics never update these values.
    pub fn set_alleles(
        &mut self,
        person: PersonId,
        locus: &LocusId,
        values: [i32; 2],
    ) -> Result<(), Error> {
        if self.pregnancies.values().any(|inheritance| {
            inheritance.parents.contains(&person) || inheritance.born == Some(person)
        }) {
            return Err(Error::Unavailable);
        }
        let (chromosome, position, definition) = self.location(locus)?;
        if values
            .iter()
            .any(|value| *value < definition.min || *value > definition.max)
        {
            return Err(configuration("founder allele outside locus bounds"));
        }
        self.allele_ids(person, locus)?;
        let ids = values.map(|value| standing_variant(locus, value));
        let mut additions = BTreeMap::new();
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
            insert_variant(&mut additions, id, variant)?;
        }
        let record = self.people.get_mut(&person).unwrap();
        for (side, id) in ids.into_iter().enumerate() {
            record.genome.chromosomes[chromosome].homologs[side].variants[position] = id;
        }
        self.variants.extend(additions);
        Ok(())
    }
    pub fn allele_ids(&self, person: PersonId, locus: &LocusId) -> Result<[VariantId; 2], Error> {
        self.allele_ids_named(person, &locus.0)
    }
    /// Reads an owner's borrowed locus name without allocating an identifier.
    pub fn allele_ids_named(&self, person: PersonId, locus: &str) -> Result<[VariantId; 2], Error> {
        let (chromosome, position, _) = self.location_named(locus)?;
        let pair = self
            .people
            .get(&person)
            .ok_or(Error::Unavailable)?
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
    pub fn alleles(&self, person: PersonId, locus: &LocusId) -> Result<[i32; 2], Error> {
        self.alleles_named(person, &locus.0)
    }
    /// Returns opaque allele values for an owner's borrowed locus name.
    pub fn alleles_named(&self, person: PersonId, locus: &str) -> Result<[i32; 2], Error> {
        let ids = self.allele_ids_named(person, locus)?;
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
        seed: u64,
        pregnancy: PregnancyId,
        side: usize,
        parent: &Genome,
    ) -> Result<Gamete, Error> {
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
        seed: u64,
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
        now: SimTime,
    ) -> Result<(), Error> {
        if let Some(existing) = self.pregnancies.get(&pregnancy) {
            return if existing.parents == parents && existing.conceived_at == now {
                Ok(())
            } else {
                Err(Error::SequenceConflict)
            };
        }
        if parents[0] == parents[1] {
            return Err(Error::Unavailable);
        }
        let first = &self
            .people
            .get(&parents[0])
            .ok_or(Error::Unavailable)?
            .genome;
        let second = &self
            .people
            .get(&parents[1])
            .ok_or(Error::Unavailable)?
            .genome;
        let a = self.gamete(seed, pregnancy, 0, first)?;
        let b = self.gamete(seed, pregnancy, 1, second)?;
        if a.mutations
            .iter()
            .any(|(id, variant)| b.mutations.get(id).is_some_and(|other| other != variant))
        {
            return Err(configuration("genetic variant identity collision"));
        }
        let genome = Genome {
            chromosomes: a
                .chromosomes
                .into_iter()
                .zip(b.chromosomes)
                .map(|(a, b)| ChromosomePair { homologs: [a, b] })
                .collect(),
        };
        self.variants.extend(a.mutations);
        self.variants.extend(b.mutations);
        self.pregnancies.insert(
            pregnancy,
            Inheritance {
                parents,
                conceived_at: now,
                genome: Some(genome),
                transmissions: [a.transmissions, b.transmissions],
                born: None,
                ended_at: None,
            },
        );
        Ok(())
    }
    pub fn born(
        &mut self,
        pregnancy: PregnancyId,
        child: PersonId,
        now: SimTime,
    ) -> Result<(), Error> {
        let inheritance = self.pregnancies.get(&pregnancy).ok_or(Error::Unavailable)?;
        if inheritance.born == Some(child) {
            return Ok(());
        }
        if inheritance.born.is_some()
            || inheritance.ended_at.is_some()
            || self.people.contains_key(&child)
            || inheritance.parents.contains(&child)
            || now < inheritance.conceived_at
        {
            return Err(Error::Unavailable);
        }
        let genome = inheritance.genome.clone().ok_or(Error::Unavailable)?;
        if !self.genome_valid(&genome) {
            return Err(configuration("malformed inherited genome"));
        }
        self.people.insert(child, PersonGenome { genome });
        let inheritance = self.pregnancies.get_mut(&pregnancy).unwrap();
        inheritance.genome = None;
        inheritance.born = Some(child);
        Ok(())
    }
    pub fn end_pregnancy(&mut self, pregnancy: PregnancyId, now: SimTime) {
        if let Some(record) = self.pregnancies.get_mut(&pregnancy)
            && record.born.is_none()
            && record.ended_at.is_none()
            && now >= record.conceived_at
        {
            record.ended_at = Some(now);
        }
    }
    fn genome_valid(&self, genome: &Genome) -> bool {
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
                                    self.variants.get(id).is_some_and(|variant| {
                                        variant.locus == definition.id
                                            && variant.value >= definition.min
                                            && variant.value <= definition.max
                                    })
                                })
                    })
                })
    }
}

pub fn validate(world: &World, now: SimTime) -> Result<(), Error> {
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
    if world
        .people
        .values()
        .any(|record| !world.genome_valid(&record.genome))
    {
        return Err(fail());
    }
    let mut children = BTreeSet::new();
    for inheritance in world.pregnancies.values() {
        if inheritance.parents[0] == inheritance.parents[1]
            || inheritance.conceived_at > now
            || inheritance.ended_at.is_some_and(|ended| {
                ended < inheritance.conceived_at || ended > now || inheritance.born.is_some()
            })
            || inheritance
                .parents
                .iter()
                .any(|parent| !world.people.contains_key(parent))
        {
            return Err(fail());
        }
        let genome = match (&inheritance.genome, inheritance.born) {
            (Some(genome), None) => genome,
            (None, Some(child))
                if !inheritance.parents.contains(&child) && children.insert(child) =>
            {
                &world.people.get(&child).ok_or_else(fail)?.genome
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
            let parent = &world.people[&inheritance.parents[side]].genome;
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
mod tests {
    use super::*;
    fn definitions() -> Vec<LocusDefinition> {
        [
            ("opaque.alpha", "a", 2),
            ("opaque.beta", "a", 7),
            ("foreign.choice", "b", 0),
        ]
        .into_iter()
        .map(|(id, chromosome, position)| LocusDefinition {
            id: LocusId(id.into()),
            chromosome: chromosome.into(),
            position,
            min: -500,
            max: 500,
            initial_alleles: vec![-500, -250, 0, 250, 500],
            mutation_step: 100,
            mutable: true,
        })
        .collect()
    }
    fn parents(rules: Rules) -> World {
        let mut world = World::new(rules, definitions()).unwrap();
        world.seed_person(7, PersonId(1)).unwrap();
        world.seed_person(7, PersonId(2)).unwrap();
        for definition in definitions() {
            world
                .set_alleles(PersonId(1), &definition.id, [-500, 500])
                .unwrap();
            world
                .set_alleles(PersonId(2), &definition.id, [-250, 250])
                .unwrap();
        }
        world
    }
    #[test]
    fn schema_and_rng_are_independent_of_definition_and_operation_order() {
        let mut reversed = definitions();
        reversed.reverse();
        reversed[0].initial_alleles.reverse();
        let mut a = World::new(Rules::default(), definitions()).unwrap();
        let mut b = World::new(Rules::default(), reversed).unwrap();
        for person in [PersonId(1), PersonId(2)] {
            a.seed_person(7, person).unwrap();
        }
        for person in [PersonId(2), PersonId(1)] {
            b.seed_person(7, person).unwrap();
        }
        for pregnancy in [PregnancyId(3), PregnancyId(4)] {
            a.conceive(9, pregnancy, [PersonId(1), PersonId(2)], SimTime(1))
                .unwrap();
        }
        for pregnancy in [PregnancyId(4), PregnancyId(3)] {
            b.conceive(9, pregnancy, [PersonId(1), PersonId(2)], SimTime(1))
                .unwrap();
        }
        assert_eq!(a, b);
        validate(&a, SimTime(1)).unwrap();
    }
    #[test]
    fn linked_crossover_traces_identify_actual_parental_homologs() {
        let mut world = parents(Rules {
            mutation_chance: 0,
            crossover_chance: 1_000,
        });
        world
            .conceive(4, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        let inheritance = &world.pregnancies[&PregnancyId(3)];
        for (side, traces) in inheritance.transmissions.iter().enumerate() {
            assert_eq!(traces.len(), 3);
            assert!(!traces[0].crossed_before);
            assert!(traces[1].crossed_before);
            assert_ne!(traces[0].source_homolog, traces[1].source_homolog);
            assert!(!traces[2].crossed_before);
            for trace in traces {
                assert!(!trace.mutated);
                assert_eq!(trace.source_variant, trace.transmitted_variant);
                assert_eq!(
                    world
                        .allele_ids(PersonId(side as u64 + 1), &trace.locus)
                        .unwrap()[usize::from(trace.source_homolog)],
                    trace.source_variant
                );
            }
        }
        validate(&world, SimTime(1)).unwrap();
    }
    #[test]
    fn mutation_respects_owner_bounds_and_immutable_loci_without_rewriting_parents() {
        let mut definitions = definitions();
        definitions[2].mutable = false;
        definitions[2].mutation_step = 0;
        let mut world = World::new(
            Rules {
                mutation_chance: 10_000,
                crossover_chance: 0,
            },
            definitions,
        )
        .unwrap();
        world.seed_person(3, PersonId(1)).unwrap();
        world.seed_person(3, PersonId(2)).unwrap();
        let before = world.people.clone();
        world
            .conceive(4, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        assert_eq!(world.people, before);
        for trace in world.pregnancies[&PregnancyId(3)]
            .transmissions
            .iter()
            .flatten()
        {
            if trace.locus.0 == "foreign.choice" {
                assert!(!trace.mutated);
                assert_eq!(trace.source_variant, trace.transmitted_variant);
            } else {
                assert!(trace.mutated);
                assert_ne!(trace.source_variant, trace.transmitted_variant);
                let old = &world.variants[&trace.source_variant];
                let new = &world.variants[&trace.transmitted_variant];
                assert!(old.value.abs_diff(new.value) <= 100);
                assert!((-500..=500).contains(&new.value));
            }
        }
        validate(&world, SimTime(1)).unwrap();
    }
    #[test]
    fn frozen_conception_birth_retry_and_save_roundtrip() {
        let mut world = parents(Rules::default());
        world
            .conceive(8, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        let frozen = world.pregnancies[&PregnancyId(3)].genome.clone().unwrap();
        assert_eq!(
            world.set_alleles(PersonId(1), &LocusId("opaque.alpha".into()), [0, 0]),
            Err(Error::Unavailable)
        );
        let before_retry = world.clone();
        world
            .conceive(999, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        assert_eq!(world, before_retry);
        assert_eq!(
            world.conceive(8, PregnancyId(3), [PersonId(2), PersonId(1)], SimTime(1)),
            Err(Error::SequenceConflict)
        );
        world.born(PregnancyId(3), PersonId(4), SimTime(2)).unwrap();
        assert_eq!(world.people[&PersonId(4)].genome, frozen);
        let before_retry = world.clone();
        world.born(PregnancyId(3), PersonId(4), SimTime(3)).unwrap();
        assert_eq!(world, before_retry);
        assert_eq!(
            world.born(PregnancyId(3), PersonId(5), SimTime(3)),
            Err(Error::Unavailable)
        );
        let restored: World = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(world, restored);
        validate(&restored, SimTime(3)).unwrap();
    }
    #[test]
    fn pregnancy_ending_retains_frozen_trace_and_prevents_birth() {
        let mut world = parents(Rules::default());
        world
            .conceive(8, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(2))
            .unwrap();
        world.end_pregnancy(PregnancyId(3), SimTime(1));
        assert_eq!(world.pregnancies[&PregnancyId(3)].ended_at, None);
        world.end_pregnancy(PregnancyId(3), SimTime(3));
        world.end_pregnancy(PregnancyId(3), SimTime(4));
        assert_eq!(
            world.pregnancies[&PregnancyId(3)].ended_at,
            Some(SimTime(3))
        );
        assert_eq!(
            world.born(PregnancyId(3), PersonId(4), SimTime(4)),
            Err(Error::Unavailable)
        );
        validate(&world, SimTime(4)).unwrap();
    }
    #[test]
    fn arbitrary_schema_sizes_and_empty_schema_need_no_trait_enumeration() {
        for size in [0, 1, 7, 19] {
            let definitions = (0..size)
                .map(|index| LocusDefinition {
                    id: LocusId(format!("external.{index}")),
                    chromosome: format!("group.{}", index / 3),
                    position: index,
                    min: i32::MIN,
                    max: i32::MAX,
                    initial_alleles: vec![i32::MIN, 0, i32::MAX],
                    mutation_step: u16::MAX,
                    mutable: true,
                })
                .collect();
            let mut world = World::new(
                Rules {
                    mutation_chance: 10_000,
                    crossover_chance: 1_000,
                },
                definitions,
            )
            .unwrap();
            world.seed_person(3, PersonId(1)).unwrap();
            world.seed_person(3, PersonId(2)).unwrap();
            world
                .conceive(3, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
                .unwrap();
            world.born(PregnancyId(3), PersonId(4), SimTime(2)).unwrap();
            assert_eq!(
                world.pregnancies[&PregnancyId(3)].transmissions[0].len(),
                usize::from(size)
            );
            validate(&world, SimTime(2)).unwrap();
        }
    }
    #[test]
    fn founder_assignment_is_atomic_and_raw_values_are_available_to_owners() {
        let mut world = parents(Rules::default());
        let locus = LocusId("opaque.alpha".into());
        let before = world.clone();
        assert!(world.set_alleles(PersonId(1), &locus, [20, 501]).is_err());
        assert_eq!(world, before);
        world.set_alleles(PersonId(1), &locus, [20, 21]).unwrap();
        assert_eq!(world.alleles(PersonId(1), &locus).unwrap(), [20, 21]);
        validate(&world, SimTime(0)).unwrap();
    }
    #[test]
    fn borrowed_and_typed_allele_lookups_agree_across_chromosome_boundaries() {
        let world = parents(Rules::default());
        for definition in &world.definitions {
            assert_eq!(
                world.alleles(PersonId(1), &definition.id),
                world.alleles_named(PersonId(1), &definition.id.0)
            );
            assert_eq!(
                world.allele_ids(PersonId(2), &definition.id),
                world.allele_ids_named(PersonId(2), &definition.id.0)
            );
        }
        assert_eq!(
            world.alleles_named(PersonId(1), "missing"),
            Err(Error::Unavailable)
        );
        assert_eq!(
            world.alleles_named(PersonId(99), "opaque.alpha"),
            Err(Error::Unavailable)
        );
    }
    #[test]
    fn borrowed_lookup_preserves_malformed_genome_and_variant_errors() {
        let original = parents(Rules::default());
        let locus = LocusId("foreign.choice".into());
        let mutations: [fn(&mut World); 3] = [
            |world| {
                world
                    .people
                    .get_mut(&PersonId(1))
                    .unwrap()
                    .genome
                    .chromosomes
                    .pop();
            },
            |world| {
                world
                    .people
                    .get_mut(&PersonId(1))
                    .unwrap()
                    .genome
                    .chromosomes[1]
                    .homologs[1]
                    .variants
                    .clear();
            },
            |world| {
                let id = world
                    .allele_ids_named(PersonId(1), "foreign.choice")
                    .unwrap()[0];
                world.variants.get_mut(&id).unwrap().locus = LocusId("opaque.alpha".into());
            },
        ];
        for mutate in mutations {
            let mut world = original.clone();
            mutate(&mut world);
            let typed = world.alleles(PersonId(1), &locus);
            assert!(matches!(typed, Err(Error::InvalidConfiguration(_))));
            assert_eq!(typed, world.alleles_named(PersonId(1), &locus.0));
        }
    }
    #[test]
    fn invalid_definitions_are_rejected() {
        let original = definitions();
        let mut duplicate = original.clone();
        duplicate.push(duplicate[0].clone());
        assert!(World::new(Rules::default(), duplicate).is_err());
        let mut duplicate_position = original.clone();
        duplicate_position[1].position = duplicate_position[0].position;
        assert!(World::new(Rules::default(), duplicate_position).is_err());
        let mutations: [fn(&mut LocusDefinition); 6] = [
            |d: &mut LocusDefinition| d.min = d.max + 1,
            |d: &mut LocusDefinition| d.initial_alleles.clear(),
            |d: &mut LocusDefinition| d.initial_alleles.push(999),
            |d: &mut LocusDefinition| d.initial_alleles.push(0),
            |d: &mut LocusDefinition| d.mutation_step = 0,
            |d: &mut LocusDefinition| d.id.0.clear(),
        ];
        for mutate in mutations {
            let mut definitions = original.clone();
            mutate(&mut definitions[0]);
            assert!(World::new(Rules::default(), definitions).is_err());
        }
        assert!(
            World::new(
                Rules {
                    mutation_chance: 10_001,
                    crossover_chance: 0
                },
                original
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_saves_reject_schema_genome_and_transmission_inconsistencies() {
        let mut original = parents(Rules {
            mutation_chance: 10_000,
            crossover_chance: 1_000,
        });
        original
            .conceive(8, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        let mutations: [fn(&mut World); 7] = [
            |w: &mut World| w.definitions.reverse(),
            |w: &mut World| {
                w.people
                    .get_mut(&PersonId(1))
                    .unwrap()
                    .genome
                    .chromosomes
                    .pop();
            },
            |w: &mut World| {
                w.pregnancies
                    .get_mut(&PregnancyId(3))
                    .unwrap()
                    .transmissions[0][1]
                    .crossed_before = false
            },
            |w: &mut World| {
                w.pregnancies
                    .get_mut(&PregnancyId(3))
                    .unwrap()
                    .transmissions[0][0]
                    .mutated = false
            },
            |w: &mut World| {
                w.pregnancies
                    .get_mut(&PregnancyId(3))
                    .unwrap()
                    .transmissions[0][0]
                    .source_homolog = 2
            },
            |w: &mut World| {
                w.pregnancies
                    .get_mut(&PregnancyId(3))
                    .unwrap()
                    .transmissions[0]
                    .pop();
            },
            |w: &mut World| {
                let id = w.pregnancies[&PregnancyId(3)].transmissions[0][0].transmitted_variant;
                w.variants.get_mut(&id).unwrap().value = 900;
            },
        ];
        for mutate in mutations {
            let mut world = original.clone();
            mutate(&mut world);
            assert!(matches!(
                validate(&world, SimTime(1)),
                Err(Error::InvalidSave(_))
            ));
        }
    }
    #[test]
    fn missing_or_same_parent_conception_has_no_effect() {
        let mut world = parents(Rules::default());
        let before = world.clone();
        assert_eq!(
            world.conceive(1, PregnancyId(3), [PersonId(1), PersonId(1)], SimTime(1)),
            Err(Error::Unavailable)
        );
        assert_eq!(
            world.conceive(1, PregnancyId(3), [PersonId(1), PersonId(9)], SimTime(1)),
            Err(Error::Unavailable)
        );
        assert_eq!(world, before);
    }
}

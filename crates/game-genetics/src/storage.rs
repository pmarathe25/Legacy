//! Genetics owns only its columns in the host's shared entity store.
use super::*;
use game_ecs::{Schema, StableId};

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<PersonGenome>("genetics/genome", PersonGenome::validate)?;
    schema.register::<Inheritance>("genetics/inheritance", Inheritance::validate)
}

fn genome_shape_valid(genome: &Genome) -> bool {
    genome.chromosomes.iter().all(|pair| {
        pair.homologs[0].variants.len() == pair.homologs[1].variants.len()
            && !pair.homologs[0].variants.is_empty()
            && pair
                .homologs
                .iter()
                .all(|homolog| homolog.variants.iter().all(|variant| variant.0 != 0))
    })
}

impl PersonGenome {
    pub fn validate(&self) -> Result<(), Error> {
        if !genome_shape_valid(&self.genome) {
            return Err(configuration("malformed person genome"));
        }
        Ok(())
    }
}

impl Inheritance {
    pub fn validate(&self) -> Result<(), Error> {
        if self.parents.iter().any(|parent| parent.0 == 0)
            || self.parents[0] == self.parents[1]
            || self
                .born
                .is_some_and(|child| child.0 == 0 || self.parents.contains(&child))
            || self.genome.is_some() == self.born.is_some()
            || self
                .genome
                .as_ref()
                .is_some_and(|genome| !genome_shape_valid(genome))
            || self
                .ended_at
                .is_some_and(|ended| ended < self.conceived_at || self.born.is_some())
            || self.transmissions[0].len() != self.transmissions[1].len()
            || self.transmissions.iter().flatten().any(|trace| {
                trace.locus.0.trim().is_empty()
                    || trace.locus.0.trim() != trace.locus.0
                    || trace.source_homolog > 1
                    || trace.source_variant.0 == 0
                    || trace.transmitted_variant.0 == 0
            })
        {
            return Err(configuration("malformed genetic inheritance"));
        }
        Ok(())
    }
}

pub fn person_genome(store: &Store, person: PersonId) -> Option<&PersonGenome> {
    store.get(StableId(person.0))
}
pub fn person_genome_mut(store: &mut Store, person: PersonId) -> Option<&mut PersonGenome> {
    store.get_mut(StableId(person.0))
}
pub fn all_person_genomes(store: &Store) -> impl Iterator<Item = (PersonId, &PersonGenome)> {
    store
        .iter::<PersonGenome>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_person_genome(
    store: &mut Store,
    person: PersonId,
    genome: PersonGenome,
) -> Result<(), Error> {
    genome.validate()?;
    store.insert_or_spawn(StableId(person.0), genome)
}
pub fn remove_person_genome(
    store: &mut Store,
    person: PersonId,
) -> Result<Option<PersonGenome>, Error> {
    if !store.contains(StableId(person.0)) {
        return Ok(None);
    }
    store.remove(StableId(person.0))
}

pub fn inheritance(store: &Store, pregnancy: PregnancyId) -> Option<&Inheritance> {
    store.get(StableId(pregnancy.0))
}
pub fn inheritance_mut(store: &mut Store, pregnancy: PregnancyId) -> Option<&mut Inheritance> {
    store.get_mut(StableId(pregnancy.0))
}
pub fn all_inheritances(store: &Store) -> impl Iterator<Item = (PregnancyId, &Inheritance)> {
    store
        .iter::<Inheritance>()
        .map(|(id, row)| (PregnancyId(id.0), row))
}
pub fn insert_inheritance(
    store: &mut Store,
    pregnancy: PregnancyId,
    inheritance: Inheritance,
) -> Result<(), Error> {
    inheritance.validate()?;
    store.insert_or_spawn(StableId(pregnancy.0), inheritance)
}
pub fn remove_inheritance(
    store: &mut Store,
    pregnancy: PregnancyId,
) -> Result<Option<Inheritance>, Error> {
    if !store.contains(StableId(pregnancy.0)) {
        return Ok(None);
    }
    store.remove(StableId(pregnancy.0))
}

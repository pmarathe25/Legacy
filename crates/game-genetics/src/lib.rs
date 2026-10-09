//! A bounded diploid inheritance model. Linked variants recombine into gametes;
//! immutable genomes supply predispositions, while expression has separate
//! developmental and experiential inputs. No skills, memories or social roles
//! are part of the genome.
use game_core::{Error, PersonId, PregnancyId, SimTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const LOCI: usize = 8;
const CHROMOSOMES: usize = 2;
const PER_CHROMOSOME: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Locus {
    Strength,
    Constitution,
    Metabolism,
    ColdAdaptation,
    Discipline,
    Sociability,
    Empathy,
    RiskTolerance,
}
impl Locus {
    pub const ALL: [Self; LOCI] = [
        Self::Strength,
        Self::Constitution,
        Self::Metabolism,
        Self::ColdAdaptation,
        Self::Discipline,
        Self::Sociability,
        Self::Empathy,
        Self::RiskTolerance,
    ];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|locus| *locus == self).unwrap()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VariantId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub locus: Locus,
    pub effect: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chromosome {
    pub variants: [VariantId; PER_CHROMOSOME],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChromosomePair {
    pub homologs: [Chromosome; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SexChromosome {
    X,
    Y,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Female,
    Male,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genome {
    pub autosomes: [ChromosomePair; CHROMOSOMES],
    pub sex: [SexChromosome; 2],
}
impl Genome {
    pub fn sex(&self) -> Result<Sex, Error> {
        match self.sex {
            [SexChromosome::X, SexChromosome::X] => Ok(Sex::Female),
            [SexChromosome::X, SexChromosome::Y] | [SexChromosome::Y, SexChromosome::X] => {
                Ok(Sex::Male)
            }
            _ => Err(Error::InvalidConfiguration(
                "unsupported sex chromosome pair".into(),
            )),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    /// Per transmitted allele, on a 0..10,000 scale. These are coarse game loci.
    pub mutation_chance: u16,
    pub mutation_step: u16,
    /// Per boundary between linked loci, on a 0..1,000 scale.
    pub crossover_chance: u16,
    pub temperament_influence: u16,
    pub learned_temperament_limit: u16,
    pub strength_spread: u16,
    pub constitution_spread: u16,
    pub metabolism_spread: u16,
    pub cold_span_centi: u16,
    pub expression_period_ms: u64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            mutation_chance: 10,
            mutation_step: 100,
            crossover_chance: 350,
            temperament_influence: 400,
            learned_temperament_limit: 150,
            strength_spread: 200,
            constitution_spread: 250,
            metabolism_spread: 150,
            cold_span_centi: 400,
            expression_period_ms: 86_400_000,
        }
    }
}
impl Rules {
    pub fn valid(&self) -> bool {
        self.mutation_chance <= 10_000
            && (1..=1_000).contains(&self.mutation_step)
            && self.crossover_chance <= 1_000
            && self.temperament_influence <= 1_000
            && self.learned_temperament_limit <= 500
            && [
                self.strength_spread,
                self.constitution_spread,
                self.metabolism_spread,
            ]
            .into_iter()
            .all(|spread| spread <= 500)
            && self.cold_span_centi <= 2_000
            && self.expression_period_ms > 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    fn valid(self) -> bool {
        [
            self.discipline,
            self.sociability,
            self.empathy,
            self.risk_tolerance,
        ]
        .into_iter()
        .all(|value| value <= 1_000)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expression {
    pub maturity_milli: u16,
    pub physical_training_milli: u16,
    pub learned_temperament: [i16; 4],
}
impl Default for Expression {
    fn default() -> Self {
        Self {
            maturity_milli: 1_000,
            physical_training_milli: 0,
            learned_temperament: [0; 4],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonGenome {
    pub genome: Genome,
    pub baseline: Temperament,
    pub expression: Expression,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transmission {
    pub locus: Locus,
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
    pub transmissions: [[Transmission; LOCI]; 2],
    pub born: Option<PersonId>,
    pub ended_at: Option<SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub variants: BTreeMap<VariantId, Variant>,
    pub people: BTreeMap<PersonId, PersonGenome>,
    pub pregnancies: BTreeMap<PregnancyId, Inheritance>,
    pub next_expression: SimTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phenotype {
    pub sex: Sex,
    pub strength_milli: u16,
    pub max_health_milli: u16,
    pub metabolism_milli: u16,
    pub cold_tolerance_centi: i16,
    pub temperament: Temperament,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Learning {
    GaveCare,
    ReceivedCare,
    Company,
    Practice,
    Guidance,
    ExperiencedHarm,
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
fn key(seed: u64, identity: u64, side: u64, chromosome: u64, locus: u64, purpose: u64) -> u64 {
    mix(seed
        ^ mix(identity)
        ^ mix(side.wrapping_add(0x9e3779b97f4a7c15))
        ^ mix(chromosome.wrapping_add(0x632be59bd9b4e019))
        ^ mix(locus.wrapping_add(0x94d049bb133111eb))
        ^ mix(purpose))
}
fn initial_id(locus: usize, index: u64) -> VariantId {
    VariantId((locus as u64) * 21 + index + 1)
}

struct Gamete {
    chromosomes: [Chromosome; CHROMOSOMES],
    sex: SexChromosome,
    transmissions: [Transmission; LOCI],
    mutations: BTreeMap<VariantId, Variant>,
}
impl World {
    pub fn new(rules: Rules, now: SimTime) -> Result<Self, Error> {
        if !rules.valid() {
            return Err(Error::InvalidConfiguration("invalid genetic rules".into()));
        }
        let next_expression = SimTime(
            now.0
                .checked_add(rules.expression_period_ms)
                .ok_or(Error::Overflow)?,
        );
        let mut world = Self {
            rules,
            next_expression,
            ..Self::default()
        };
        for (locus_index, locus) in Locus::ALL.into_iter().enumerate() {
            for index in 0..=20 {
                world.variants.insert(
                    initial_id(locus_index, index),
                    Variant {
                        locus,
                        effect: (index * 50) as u16,
                    },
                );
            }
        }
        Ok(world)
    }
    pub fn seed_person(
        &mut self,
        seed: u64,
        person: PersonId,
        sex: Sex,
        baseline: Temperament,
        expression: Expression,
    ) -> Result<(), Error> {
        if self.people.contains_key(&person)
            || !baseline.valid()
            || expression.maturity_milli > 1_000
            || expression.physical_training_milli > 200
            || expression
                .learned_temperament
                .iter()
                .any(|offset| offset.unsigned_abs() > self.rules.learned_temperament_limit)
        {
            return Err(Error::InvalidConfiguration(
                "duplicate genetic person or invalid baseline".into(),
            ));
        }
        let autosomes = std::array::from_fn(|chromosome| ChromosomePair {
            homologs: std::array::from_fn(|side| Chromosome {
                variants: std::array::from_fn(|locus| {
                    let index = key(
                        seed,
                        person.0,
                        side as u64,
                        chromosome as u64,
                        locus as u64,
                        1,
                    ) % 11
                        + key(
                            seed,
                            person.0,
                            side as u64,
                            chromosome as u64,
                            locus as u64,
                            2,
                        ) % 11;
                    initial_id(chromosome * PER_CHROMOSOME + locus, index)
                }),
            }),
        });
        let genome = Genome {
            autosomes,
            sex: match sex {
                Sex::Female => [SexChromosome::X, SexChromosome::X],
                Sex::Male => [SexChromosome::X, SexChromosome::Y],
            },
        };
        self.people.insert(
            person,
            PersonGenome {
                genome,
                baseline,
                expression,
            },
        );
        Ok(())
    }
    fn gamete(
        &self,
        seed: u64,
        pregnancy: PregnancyId,
        side: usize,
        parent: &Genome,
    ) -> Result<Gamete, Error> {
        let mut mutations = BTreeMap::new();
        let mut traces = std::array::from_fn(|index| Transmission {
            locus: Locus::ALL[index],
            source_homolog: 0,
            source_variant: VariantId(0),
            transmitted_variant: VariantId(0),
            crossed_before: false,
            mutated: false,
        });
        let mut chromosomes = Vec::new();
        for chromosome in 0..CHROMOSOMES {
            let mut homolog =
                (key(seed, pregnancy.0, side as u64, chromosome as u64, 0, 3) % 2) as usize;
            let mut transmitted = [VariantId(0); PER_CHROMOSOME];
            for (locus, slot) in transmitted.iter_mut().enumerate() {
                let crossed = locus > 0
                    && key(
                        seed,
                        pregnancy.0,
                        side as u64,
                        chromosome as u64,
                        locus as u64,
                        4,
                    ) % 1_000
                        < u64::from(self.rules.crossover_chance);
                if crossed {
                    homolog ^= 1;
                }
                let source = parent.autosomes[chromosome].homologs[homolog].variants[locus];
                let variant = *self.variants.get(&source).ok_or_else(|| {
                    Error::InvalidConfiguration("unknown inherited variant".into())
                })?;
                if variant.locus != Locus::ALL[chromosome * PER_CHROMOSOME + locus] {
                    return Err(Error::InvalidConfiguration(
                        "variant belongs to a different locus".into(),
                    ));
                }
                let mutated = key(
                    seed,
                    pregnancy.0,
                    side as u64,
                    chromosome as u64,
                    locus as u64,
                    5,
                ) % 10_000
                    < u64::from(self.rules.mutation_chance);
                let target = if mutated {
                    let delta = (key(
                        seed,
                        pregnancy.0,
                        side as u64,
                        chromosome as u64,
                        locus as u64,
                        6,
                    ) % (u64::from(self.rules.mutation_step) * 2 + 1))
                        as i32
                        - i32::from(self.rules.mutation_step);
                    let id = VariantId(
                        key(
                            seed,
                            pregnancy.0,
                            side as u64,
                            chromosome as u64,
                            locus as u64,
                            7,
                        ) ^ mix(source.0)
                            | 1 << 63,
                    );
                    let value = Variant {
                        locus: variant.locus,
                        effect: (i32::from(variant.effect) + delta).clamp(0, 1_000) as u16,
                    };
                    if self
                        .variants
                        .get(&id)
                        .is_some_and(|existing| *existing != value)
                    {
                        return Err(Error::InvalidConfiguration(
                            "genetic variant identity collision".into(),
                        ));
                    }
                    mutations.insert(id, value);
                    id
                } else {
                    source
                };
                *slot = target;
                traces[chromosome * PER_CHROMOSOME + locus] = Transmission {
                    locus: variant.locus,
                    source_homolog: homolog as u8,
                    source_variant: source,
                    transmitted_variant: target,
                    crossed_before: crossed,
                    mutated,
                };
            }
            chromosomes.push(Chromosome {
                variants: transmitted,
            });
        }
        let sex = parent.sex[(key(seed, pregnancy.0, side as u64, 2, 0, 8) % 2) as usize];
        Ok(Gamete {
            chromosomes: chromosomes.try_into().unwrap(),
            sex,
            transmissions: traces,
            mutations,
        })
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
        if first.sex() != Ok(Sex::Female) || second.sex() != Ok(Sex::Male) {
            return Err(Error::Unavailable);
        }
        let a = self.gamete(seed, pregnancy, 0, first)?;
        let b = self.gamete(seed, pregnancy, 1, second)?;
        if a.mutations
            .iter()
            .any(|(id, variant)| b.mutations.get(id).is_some_and(|other| other != variant))
        {
            return Err(Error::InvalidConfiguration(
                "genetic variant identity collision".into(),
            ));
        }
        let genome = Genome {
            autosomes: std::array::from_fn(|index| ChromosomePair {
                homologs: [a.chromosomes[index].clone(), b.chromosomes[index].clone()],
            }),
            sex: [a.sex, b.sex],
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
    ) -> Result<Phenotype, Error> {
        let inheritance = self.pregnancies.get(&pregnancy).ok_or(Error::Unavailable)?;
        if inheritance.born == Some(child) {
            return self.phenotype(child);
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
        let record = PersonGenome {
            genome,
            baseline: Temperament::default(),
            expression: Expression {
                maturity_milli: 0,
                ..Expression::default()
            },
        };
        let phenotype = self.express(&record)?;
        self.people.insert(child, record);
        let inheritance = self.pregnancies.get_mut(&pregnancy).unwrap();
        inheritance.genome = None;
        inheritance.born = Some(child);
        Ok(phenotype)
    }
    pub fn end_pregnancy(&mut self, pregnancy: PregnancyId, now: SimTime) {
        if let Some(record) = self.pregnancies.get_mut(&pregnancy)
            && record.born.is_none()
            && now >= record.conceived_at
        {
            record.ended_at = Some(now);
        }
    }
    pub fn phenotype(&self, person: PersonId) -> Result<Phenotype, Error> {
        let record = self.people.get(&person).ok_or(Error::Unavailable)?;
        self.express(record)
    }
    fn express(&self, record: &PersonGenome) -> Result<Phenotype, Error> {
        let mut scores = [0_i32; LOCI];
        for (index, score) in scores.iter_mut().enumerate() {
            let pair = &record.genome.autosomes[index / PER_CHROMOSOME];
            let first = self
                .variants
                .get(&pair.homologs[0].variants[index % PER_CHROMOSOME])
                .ok_or(Error::Unavailable)?;
            let second = self
                .variants
                .get(&pair.homologs[1].variants[index % PER_CHROMOSOME])
                .ok_or(Error::Unavailable)?;
            *score = (i32::from(first.effect) + i32::from(second.effect)) / 2;
        }
        let factor = |score: i32, spread: u16| 1_000 + (score - 500) * i32::from(spread) / 500;
        let growth = 500 + i32::from(record.expression.maturity_milli) / 2;
        let strength = (factor(scores[0], self.rules.strength_spread) * growth / 1_000
            + i32::from(record.expression.physical_training_milli))
        .clamp(250, 2_000) as u16;
        let health_growth = 800 + i32::from(record.expression.maturity_milli) / 5;
        let baseline = [
            record.baseline.discipline,
            record.baseline.sociability,
            record.baseline.empathy,
            record.baseline.risk_tolerance,
        ];
        let temperament: [u16; 4] = std::array::from_fn(|index| {
            ((i32::from(baseline[index]) * (1_000 - i32::from(self.rules.temperament_influence))
                + scores[index + 4] * i32::from(self.rules.temperament_influence))
                / 1_000
                + i32::from(record.expression.learned_temperament[index]))
            .clamp(0, 1_000) as u16
        });
        Ok(Phenotype {
            sex: record.genome.sex()?,
            strength_milli: strength,
            max_health_milli: (factor(scores[1], self.rules.constitution_spread) * health_growth
                / 1_000) as u16,
            metabolism_milli: factor(scores[2], self.rules.metabolism_spread) as u16,
            cold_tolerance_centi: ((scores[3] - 500) * i32::from(self.rules.cold_span_centi) / 500)
                as i16,
            temperament: Temperament {
                discipline: temperament[0],
                sociability: temperament[1],
                empathy: temperament[2],
                risk_tolerance: temperament[3],
            },
        })
    }
    pub fn develop(&mut self, person: PersonId, maturity: u16, training: u16) -> Result<(), Error> {
        if maturity > 1_000 || training > 200 {
            return Err(Error::InvalidConfiguration(
                "invalid developmental genetic expression".into(),
            ));
        }
        let record = self.people.get_mut(&person).ok_or(Error::Unavailable)?;
        record.expression.maturity_milli = maturity;
        record.expression.physical_training_milli = training;
        Ok(())
    }
    pub fn learn(&mut self, person: PersonId, event: Learning) {
        let Some(record) = self.people.get_mut(&person) else {
            return;
        };
        let delta = match event {
            Learning::GaveCare => [1, 0, 1, 0],
            Learning::ReceivedCare => [0, 1, 1, 0],
            Learning::Company => [0, 1, 0, 0],
            Learning::Practice => [1, 0, 0, 0],
            Learning::Guidance => [1, 1, 0, 0],
            Learning::ExperiencedHarm => [0, 0, 0, -1],
        };
        let bound = self.rules.learned_temperament_limit as i16;
        for (offset, delta) in record.expression.learned_temperament.iter_mut().zip(delta) {
            *offset = (*offset + delta).clamp(-bound, bound);
        }
    }
}

pub fn validate(world: &World, now: SimTime) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent genetic state".into());
    if !world.rules.valid()
        || world.next_expression <= now
        || world
            .variants
            .iter()
            .any(|(id, variant)| id.0 == 0 || variant.effect > 1_000)
    {
        return Err(fail());
    }
    let genome_valid = |genome: &Genome| -> bool {
        genome.sex().is_ok()
            && genome
                .autosomes
                .iter()
                .enumerate()
                .all(|(chromosome, pair)| {
                    pair.homologs.iter().all(|homolog| {
                        homolog.variants.iter().enumerate().all(|(locus, id)| {
                            world.variants.get(id).is_some_and(|variant| {
                                variant.locus == Locus::ALL[chromosome * PER_CHROMOSOME + locus]
                            })
                        })
                    })
                })
    };
    for record in world.people.values() {
        if !genome_valid(&record.genome)
            || !record.baseline.valid()
            || record.expression.maturity_milli > 1_000
            || record.expression.physical_training_milli > 200
            || record
                .expression
                .learned_temperament
                .iter()
                .any(|offset| offset.unsigned_abs() > world.rules.learned_temperament_limit)
        {
            return Err(fail());
        }
    }
    for inheritance in world.pregnancies.values() {
        if inheritance.parents[0] == inheritance.parents[1]
            || inheritance.conceived_at > now
            || inheritance.ended_at.is_some_and(|ended| {
                ended < inheritance.conceived_at || ended > now || inheritance.born.is_some()
            })
            || !world.people.contains_key(&inheritance.parents[0])
            || !world.people.contains_key(&inheritance.parents[1])
        {
            return Err(fail());
        }
        let genome = match (&inheritance.genome, inheritance.born) {
            (Some(genome), None) => genome,
            (None, Some(child)) if !inheritance.parents.contains(&child) => {
                &world.people.get(&child).ok_or_else(fail)?.genome
            }
            _ => return Err(fail()),
        };
        if !genome_valid(genome) {
            return Err(fail());
        }
        for (side, traces) in inheritance.transmissions.iter().enumerate() {
            let parent = &world.people[&inheritance.parents[side]].genome;
            for (index, trace) in traces.iter().enumerate() {
                if trace.locus != Locus::ALL[index]
                    || trace.source_homolog > 1
                    || index % PER_CHROMOSOME == 0 && trace.crossed_before
                    || world
                        .variants
                        .get(&trace.source_variant)
                        .is_none_or(|variant| variant.locus != trace.locus)
                    || world
                        .variants
                        .get(&trace.transmitted_variant)
                        .is_none_or(|variant| variant.locus != trace.locus)
                    || parent.autosomes[index / PER_CHROMOSOME].homologs
                        [usize::from(trace.source_homolog)]
                    .variants[index % PER_CHROMOSOME]
                        != trace.source_variant
                    || genome.autosomes[index / PER_CHROMOSOME].homologs[side].variants
                        [index % PER_CHROMOSOME]
                        != trace.transmitted_variant
                    || !trace.mutated && trace.source_variant != trace.transmitted_variant
                    || trace.mutated && trace.source_variant == trace.transmitted_variant
                {
                    return Err(fail());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parents(rules: Rules) -> World {
        let mut world = World::new(rules, SimTime(0)).unwrap();
        world
            .seed_person(
                7,
                PersonId(1),
                Sex::Female,
                Temperament::default(),
                Expression::default(),
            )
            .unwrap();
        world
            .seed_person(
                7,
                PersonId(2),
                Sex::Male,
                Temperament::default(),
                Expression::default(),
            )
            .unwrap();
        world
    }
    #[test]
    fn each_transmitted_variant_has_an_actual_parental_homolog_and_linked_crossover() {
        let rules = Rules {
            mutation_chance: 0,
            crossover_chance: 1_000,
            ..Rules::default()
        };
        let mut world = parents(rules);
        world
            .conceive(4, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        for (side, traces) in world.pregnancies[&PregnancyId(3)]
            .transmissions
            .iter()
            .enumerate()
        {
            let mut previous = None;
            for trace in traces {
                assert!(!trace.mutated);
                assert_eq!(trace.source_variant, trace.transmitted_variant);
                let index = trace.locus.index();
                assert_eq!(
                    world.people[&PersonId(side as u64 + 1)].genome.autosomes[index / 4].homologs
                        [usize::from(trace.source_homolog)]
                    .variants[index % 4],
                    trace.source_variant
                );
                if index % 4 > 0 {
                    assert!(trace.crossed_before);
                    assert_ne!(previous, Some(trace.source_homolog));
                }
                previous = Some(trace.source_homolog);
            }
        }
        validate(&world, SimTime(1)).unwrap();
    }
    #[test]
    fn mutation_creates_explicit_new_variants_without_rewriting_parent_genomes() {
        let mut world = parents(Rules {
            mutation_chance: 10_000,
            ..Rules::default()
        });
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
            assert!(trace.mutated);
            assert_ne!(trace.source_variant, trace.transmitted_variant);
            let old = world.variants[&trace.source_variant];
            let new = world.variants[&trace.transmitted_variant];
            assert_eq!(old.locus, new.locus);
            assert!(old.effect.abs_diff(new.effect) <= 100);
        }
        validate(&world, SimTime(1)).unwrap();
    }
    #[test]
    fn keyed_gametes_are_replay_order_and_save_independent() {
        let mut first = parents(Rules::default());
        let mut second = first.clone();
        for id in [3, 4, 5] {
            first
                .conceive(42, PregnancyId(id), [PersonId(1), PersonId(2)], SimTime(1))
                .unwrap();
        }
        for id in [5, 3, 4] {
            second
                .conceive(42, PregnancyId(id), [PersonId(1), PersonId(2)], SimTime(1))
                .unwrap();
        }
        assert_eq!(first, second);
        let saved = serde_json::to_vec(&first).unwrap();
        let mut restored: World = serde_json::from_slice(&saved).unwrap();
        restored
            .conceive(42, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        assert_eq!(first, restored);
        assert_eq!(
            first.born(PregnancyId(3), PersonId(6), SimTime(2)).unwrap(),
            restored
                .born(PregnancyId(3), PersonId(6), SimTime(2))
                .unwrap()
        );
        assert_eq!(first, restored);
    }
    #[test]
    fn conception_is_frozen_when_parents_develop_and_learn_later() {
        let mut world = parents(Rules::default());
        world
            .conceive(9, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        let frozen = world.pregnancies[&PregnancyId(3)].genome.clone().unwrap();
        for _ in 0..100 {
            world.learn(PersonId(1), Learning::GaveCare);
            world.learn(PersonId(2), Learning::ExperiencedHarm);
        }
        world.develop(PersonId(1), 1_000, 200).unwrap();
        world.born(PregnancyId(3), PersonId(4), SimTime(2)).unwrap();
        assert_eq!(world.people[&PersonId(4)].genome, frozen);
        assert_eq!(
            world.people[&PersonId(4)].expression.learned_temperament,
            [0; 4]
        );
        assert_eq!(
            world.people[&PersonId(4)]
                .expression
                .physical_training_milli,
            0
        );
        assert!(world.pregnancies[&PregnancyId(3)].genome.is_none());
        validate(&world, SimTime(2)).unwrap();
    }
    #[test]
    fn parental_similarity_is_inherited_while_maturity_training_and_care_change_expression() {
        let mut world = parents(Rules {
            mutation_chance: 0,
            temperament_influence: 1_000,
            ..Rules::default()
        });
        // Both biological parents carry the same high variants at every locus.
        for record in world.people.values_mut() {
            for (chromosome, pair) in record.genome.autosomes.iter_mut().enumerate() {
                for homolog in &mut pair.homologs {
                    for (locus, slot) in homolog.variants.iter_mut().enumerate() {
                        *slot = initial_id(chromosome * 4 + locus, 18);
                    }
                }
            }
        }
        world
            .conceive(1, PregnancyId(3), [PersonId(1), PersonId(2)], SimTime(1))
            .unwrap();
        let newborn = world.born(PregnancyId(3), PersonId(4), SimTime(2)).unwrap();
        assert_eq!(newborn.temperament.empathy, 900);
        assert!(newborn.strength_milli < world.phenotype(PersonId(1)).unwrap().strength_milli);
        let genome = world.people[&PersonId(4)].genome.clone();
        world.develop(PersonId(4), 1_000, 100).unwrap();
        for _ in 0..30 {
            world.learn(PersonId(4), Learning::ReceivedCare);
        }
        let grown = world.phenotype(PersonId(4)).unwrap();
        assert!(grown.strength_milli > newborn.strength_milli);
        assert!(grown.max_health_milli > newborn.max_health_milli);
        assert!(grown.temperament.empathy > newborn.temperament.empathy);
        assert_eq!(world.people[&PersonId(4)].genome, genome);
    }
    #[test]
    fn genomes_contain_neither_learned_mastery_nor_goals_titles_or_caregivers() {
        let world = parents(Rules::default());
        let value = serde_json::to_value(&world.people[&PersonId(1)].genome).unwrap();
        let keys = value.as_object().unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("autosomes") && keys.contains_key("sex"));
        let mut invalid = world;
        invalid
            .people
            .get_mut(&PersonId(1))
            .unwrap()
            .genome
            .autosomes[0]
            .homologs[0]
            .variants[0] = VariantId(0);
        assert!(validate(&invalid, SimTime(0)).is_err());
    }
}

//! Temperament predisposition and experience. Genes are optional raw evidence;
//! acquired offsets and the interpretation of that evidence belong to characters.
use game_core::{Error, PersonId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub definitions: Definitions,
    pub people: BTreeMap<PersonId, PersonTemperament>,
}
impl World {
    pub fn new(definitions: Definitions) -> Result<Self, Error> {
        definitions.validate()?;
        Ok(Self {
            definitions,
            people: BTreeMap::new(),
        })
    }
    pub fn register(&mut self, person: PersonId, baseline: Temperament) -> Result<(), Error> {
        if !baseline.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid temperament baseline".into(),
            ));
        }
        if self.people.contains_key(&person) {
            return Err(Error::Unavailable);
        }
        self.people.insert(
            person,
            PersonTemperament {
                baseline,
                learned_offsets: [0; 4],
            },
        );
        Ok(())
    }
    pub fn register_newborn(
        &mut self,
        person: PersonId,
    ) -> Result<crate::decisions::Personality, Error> {
        let personality = self.definitions.newborn_personality.clone();
        self.register(person, Temperament::from(&personality))?;
        Ok(personality)
    }
    pub fn express(
        &self,
        person: PersonId,
        scores: &BTreeMap<String, i32>,
    ) -> Result<Temperament, Error> {
        self.express_with(person, |locus| {
            scores.get(locus).copied().ok_or(Error::Unavailable)
        })
    }
    fn express_with(
        &self,
        person: PersonId,
        mut score: impl FnMut(&str) -> Result<i32, Error>,
    ) -> Result<Temperament, Error> {
        let record = self.people.get(&person).ok_or(Error::Unavailable)?;
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
        person: PersonId,
        genes: &game_genetics::World,
    ) -> Result<Temperament, Error> {
        self.express_with(person, |locus| {
            let alleles = genes.alleles_named(person, locus)?;
            Ok(((i64::from(alleles[0]) + i64::from(alleles[1])) / 2) as i32)
        })
    }
    pub fn learn(&mut self, person: PersonId, experience: &str) -> Result<(), Error> {
        let deltas = self
            .definitions
            .learning
            .iter()
            .find(|l| l.id == experience)
            .ok_or(Error::Unavailable)?
            .deltas;
        let record = self.people.get_mut(&person).ok_or(Error::Unavailable)?;
        let bound = i32::from(self.definitions.learned_limit);
        for (offset, delta) in record.learned_offsets.iter_mut().zip(deltas) {
            *offset = (i32::from(*offset) + i32::from(delta)).clamp(-bound, bound) as i16;
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.definitions.validate().is_err()
            || self.people.values().any(|p| {
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
    fn definitions() -> Definitions {
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
        let mut world = World::new(definitions()).unwrap();
        world.register(PersonId(1), Temperament::default()).unwrap();
        let scores = BTreeMap::from([("temperament.test".into(), 1_000)]);
        assert_eq!(world.express(PersonId(1), &scores).unwrap().empathy, 700);
        for _ in 0..10 {
            world.learn(PersonId(1), "care").unwrap();
        }
        assert_eq!(world.express(PersonId(1), &scores).unwrap().empathy, 702);
        assert_eq!(world.people[&PersonId(1)].learned_offsets, [2, 0, 2, 0]);
        assert_eq!(scores["temperament.test"], 1_000);
        world.validate().unwrap();
    }
    #[test]
    fn noninherited_dispositions_remain_with_the_character() {
        let mut world = World::new(Definitions {
            traits: vec![],
            ..definitions()
        })
        .unwrap();
        world
            .register(
                PersonId(1),
                Temperament {
                    empathy: 900,
                    ..Temperament::default()
                },
            )
            .unwrap();
        assert_eq!(
            world
                .express(PersonId(1), &BTreeMap::new())
                .unwrap()
                .empathy,
            900
        );
        let mut personality = crate::decisions::Personality::industrious();
        let ambition = personality.ambition;
        world
            .express(PersonId(1), &BTreeMap::new())
            .unwrap()
            .apply_to(&mut personality);
        assert_eq!(personality.empathy, 900);
        assert_eq!(personality.ambition, ambition);
    }
    #[test]
    fn newborn_baseline_comes_from_character_content() {
        let mut definitions = definitions();
        definitions.newborn_personality.ambition = 123;
        definitions.newborn_personality.empathy = 456;
        let mut world = World::new(definitions).unwrap();
        let personality = world.register_newborn(PersonId(7)).unwrap();
        assert_eq!(personality.ambition, 123);
        assert_eq!(world.people[&PersonId(7)].baseline.empathy, 456);
        assert_eq!(world.people[&PersonId(7)].learned_offsets, [0; 4]);
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn optional_genetics_and_experience_have_separate_state() {
        use game_genetics::HeritableCharacteristic;
        let definitions = definitions();
        let mut genes =
            game_genetics::World::new(game_genetics::Rules::default(), definitions.loci()).unwrap();
        genes.seed_person(7, PersonId(1)).unwrap();
        genes
            .set_alleles(
                PersonId(1),
                &game_genetics::LocusId("temperament.test".into()),
                [1_000, 1_000],
            )
            .unwrap();
        let before = genes.clone();
        let mut mind = World::new(definitions).unwrap();
        mind.register(PersonId(1), Temperament::default()).unwrap();
        assert_eq!(
            mind.express_from_genes(PersonId(1), &genes)
                .unwrap()
                .empathy,
            700
        );
        mind.learn(PersonId(1), "care").unwrap();
        assert_eq!(
            mind.express_from_genes(PersonId(1), &genes)
                .unwrap()
                .empathy,
            701
        );
        assert_eq!(genes, before);
    }
    #[test]
    fn unknown_experience_is_inert_and_save_round_trips() {
        let mut world = World::new(definitions()).unwrap();
        world.register(PersonId(1), Temperament::default()).unwrap();
        let before = world.clone();
        assert!(world.learn(PersonId(1), "missing").is_err());
        assert_eq!(world, before);
        let bytes = serde_json::to_vec(&world).unwrap();
        assert_eq!(serde_json::from_slice::<World>(&bytes).unwrap(), world);
    }
}

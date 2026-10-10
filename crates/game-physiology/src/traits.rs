//! Physical characteristic definitions and acquired development. Inheritance
//! supplies raw alleles; this module alone interprets them as body properties.
use game_core::{Error, PersonId, SimTime};
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
pub enum PhysicalTrait {
    Strength,
    Constitution,
    Metabolism,
    ColdAdaptation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitDefinition {
    pub kind: PhysicalTrait,
    pub inheritance: GeneBinding,
    /// Strength, constitution and metabolism use a milli-factor spread; cold
    /// adaptation uses a centigrade span around its neutral score.
    pub spread: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub expression_period_ms: u64,
    pub strength_growth_base: u16,
    pub strength_maturity_divisor: u16,
    pub health_growth_base: u16,
    pub health_maturity_divisor: u16,
    pub min_strength: u16,
    pub max_strength: u16,
    pub max_training: u16,
    pub training_per_level: u16,
    pub traits: Vec<TraitDefinition>,
}
impl Definitions {
    pub fn validate(&self) -> Result<(), Error> {
        let mut kinds = BTreeSet::new();
        let mut loci = BTreeSet::new();
        if self.expression_period_ms == 0
            || self.strength_maturity_divisor == 0
            || self.health_maturity_divisor == 0
            || self.min_strength == 0
            || self.min_strength > self.max_strength
            || self.strength_growth_base > 1_000
            || self.health_growth_base < 2
            || self.health_growth_base > 1_000
            || self.max_strength > 10_000
            || self.max_training > 10_000
            || self.traits.iter().any(|t| {
                !t.inheritance.valid()
                    || !kinds.insert(t.kind)
                    || !loci.insert(&t.inheritance.locus)
                    || t.spread
                        > if t.kind == PhysicalTrait::ColdAdaptation {
                            30_000
                        } else {
                            500
                        }
            })
        {
            return Err(Error::InvalidConfiguration(
                "invalid physical trait definitions".into(),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Development {
    pub maturity_milli: u16,
    pub physical_training_milli: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phenotype {
    pub strength_milli: u16,
    pub max_health_milli: u16,
    pub metabolism_milli: u16,
    pub cold_tolerance_centi: i16,
}

/// Preserve the proportion of existing injury, including its fixed-point
/// remainder, when the body's maximum health changes. Death remains death.
pub fn rescale_health(
    health: u64,
    remainder: u16,
    alive: bool,
    old_factor: u16,
    new_factor: u16,
) -> (u64, u16) {
    if !alive {
        return (0, 0);
    }
    if old_factor == 0 {
        return (health, remainder);
    }
    let scaled = (u128::from(health) * 1_000 + u128::from(remainder)) * u128::from(new_factor)
        / u128::from(old_factor);
    (
        (scaled / 1_000).min(u128::from(u64::MAX)) as u64,
        (scaled % 1_000) as u16,
    )
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub definitions: Definitions,
    pub people: BTreeMap<PersonId, Development>,
    pub next_expression: SimTime,
}
impl World {
    pub fn reset_epoch(&mut self, now: SimTime) -> Result<(), Error> {
        let next = SimTime(
            now.0
                .checked_add(self.definitions.expression_period_ms)
                .ok_or(Error::Overflow)?,
        );
        self.next_expression = next;
        Ok(())
    }
    pub fn new(definitions: Definitions, now: SimTime) -> Result<Self, Error> {
        definitions.validate()?;
        let next_expression = SimTime(
            now.0
                .checked_add(definitions.expression_period_ms)
                .ok_or(Error::Overflow)?,
        );
        Ok(Self {
            definitions,
            people: BTreeMap::new(),
            next_expression,
        })
    }
    pub fn register(&mut self, person: PersonId, development: Development) -> Result<(), Error> {
        if self.people.contains_key(&person) {
            return Err(Error::Unavailable);
        }
        self.check_development(development)?;
        self.people.insert(person, development);
        Ok(())
    }
    fn check_development(&self, development: Development) -> Result<(), Error> {
        if development.maturity_milli > 1_000
            || development.physical_training_milli > self.definitions.max_training
        {
            return Err(Error::InvalidConfiguration(
                "invalid physical development".into(),
            ));
        }
        Ok(())
    }
    pub fn develop(&mut self, person: PersonId, maturity: u16, training: u16) -> Result<(), Error> {
        let development = Development {
            maturity_milli: maturity,
            physical_training_milli: training,
        };
        self.check_development(development)?;
        *self.people.get_mut(&person).ok_or(Error::Unavailable)? = development;
        Ok(())
    }
    pub fn phenotype(
        &self,
        person: PersonId,
        scores: &BTreeMap<String, i32>,
    ) -> Result<Phenotype, Error> {
        self.phenotype_with(person, |locus| {
            scores.get(locus).copied().ok_or(Error::Unavailable)
        })
    }
    fn phenotype_with(
        &self,
        person: PersonId,
        mut score: impl FnMut(&str) -> Result<i32, Error>,
    ) -> Result<Phenotype, Error> {
        let d = self.people.get(&person).ok_or(Error::Unavailable)?;
        let mut result = Phenotype {
            strength_milli: 1_000,
            max_health_milli: 1_000,
            metabolism_milli: 1_000,
            cold_tolerance_centi: 0,
        };
        for t in &self.definitions.traits {
            let score = score(&t.inheritance.locus)?;
            if !(0..=1_000).contains(&score) {
                return Err(Error::Unavailable);
            }
            let factor = 1_000 + (score - 500) * i32::from(t.spread) / 500;
            match t.kind {
                PhysicalTrait::Strength => {
                    let growth = i32::from(self.definitions.strength_growth_base)
                        + i32::from(d.maturity_milli)
                            / i32::from(self.definitions.strength_maturity_divisor);
                    result.strength_milli =
                        (factor * growth / 1_000 + i32::from(d.physical_training_milli)).clamp(
                            i32::from(self.definitions.min_strength),
                            i32::from(self.definitions.max_strength),
                        ) as u16;
                }
                PhysicalTrait::Constitution => {
                    let growth = i32::from(self.definitions.health_growth_base)
                        + i32::from(d.maturity_milli)
                            / i32::from(self.definitions.health_maturity_divisor);
                    result.max_health_milli = (factor * growth / 1_000) as u16;
                }
                PhysicalTrait::Metabolism => result.metabolism_milli = factor as u16,
                PhysicalTrait::ColdAdaptation => {
                    result.cold_tolerance_centi = ((score - 500) * i32::from(t.spread) / 500) as i16
                }
            }
        }
        Ok(result)
    }
    #[cfg(feature = "genetics")]
    pub fn phenotype_from_genes(
        &self,
        person: PersonId,
        genes: &game_genetics::World,
    ) -> Result<Phenotype, Error> {
        self.phenotype_with(person, |locus| {
            let alleles = genes.alleles_named(person, locus)?;
            Ok(((i64::from(alleles[0]) + i64::from(alleles[1])) / 2) as i32)
        })
    }
    /// Callers select the relevant acquired skill levels. No skill names or
    /// dependence on the skill system enter physical development.
    pub fn training_from_levels(&self, levels: impl IntoIterator<Item = u16>) -> u16 {
        let total: u128 = levels.into_iter().map(u128::from).sum();
        (total.saturating_mul(u128::from(self.definitions.training_per_level)))
            .min(u128::from(self.definitions.max_training)) as u16
    }
    pub fn next_boundary(&self, now: SimTime, limit: SimTime) -> Option<SimTime> {
        (self.next_expression > now && self.next_expression <= limit)
            .then_some(self.next_expression)
    }
    pub fn prepare_refresh_time(&self, now: SimTime) -> Result<SimTime, Error> {
        if now < self.next_expression {
            return Err(Error::Unavailable);
        }
        let period = self.definitions.expression_period_ms;
        let steps = (now.0 - self.next_expression.0) / period + 1;
        Ok(SimTime(
            self.next_expression
                .0
                .checked_add(steps.checked_mul(period).ok_or(Error::Overflow)?)
                .ok_or(Error::Overflow)?,
        ))
    }
    pub fn refresh_time(&mut self, now: SimTime) -> Result<SimTime, Error> {
        let next = self.prepare_refresh_time(now)?;
        self.next_expression = next;
        Ok(next)
    }
    pub fn validate(&self, now: SimTime) -> Result<(), Error> {
        if self.definitions.validate().is_err()
            || self.next_expression <= now
            || self
                .people
                .values()
                .any(|d| self.check_development(*d).is_err())
        {
            return Err(Error::InvalidSave(
                "inconsistent physical trait state".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn body_growth_preserves_injury_fraction_and_cannot_revive_a_dead_body() {
        assert_eq!(rescale_health(50, 500, true, 1_000, 1_200), (60, 600));
        assert_eq!(rescale_health(60, 600, true, 1_200, 1_000), (50, 500));
        assert_eq!(rescale_health(50, 500, false, 1_000, 1_200), (0, 0));
    }
    #[test]
    fn delayed_refresh_keeps_its_original_cadence_and_overflow_preserves_the_epoch() {
        let mut world = World::new(definitions(), SimTime(0)).unwrap();
        assert_eq!(world.refresh_time(SimTime(25)).unwrap(), SimTime(30));
        let before = world.clone();
        assert_eq!(world.reset_epoch(SimTime(u64::MAX)), Err(Error::Overflow));
        assert_eq!(world, before);
    }
    fn definitions() -> Definitions {
        Definitions {
            expression_period_ms: 10,
            strength_growth_base: 500,
            strength_maturity_divisor: 2,
            health_growth_base: 800,
            health_maturity_divisor: 5,
            min_strength: 250,
            max_strength: 2_000,
            max_training: 200,
            training_per_level: 2,
            traits: vec![TraitDefinition {
                kind: PhysicalTrait::Strength,
                spread: 200,
                inheritance: GeneBinding {
                    locus: "body.test".into(),
                    chromosome: "body".into(),
                    position: 0,
                    min: 0,
                    max: 1_000,
                    initial_alleles: vec![0, 500, 1_000],
                    mutation_step: 100,
                    mutable: true,
                },
            }],
        }
    }
    #[test]
    fn development_and_training_change_expression_without_changing_alleles() {
        let mut world = World::new(definitions(), SimTime(0)).unwrap();
        world
            .register(
                PersonId(1),
                Development {
                    maturity_milli: 0,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
        let scores = BTreeMap::from([("body.test".into(), 1_000)]);
        assert_eq!(
            world
                .phenotype(PersonId(1), &scores)
                .unwrap()
                .strength_milli,
            600
        );
        world.develop(PersonId(1), 1_000, 100).unwrap();
        assert_eq!(
            world
                .phenotype(PersonId(1), &scores)
                .unwrap()
                .strength_milli,
            1_300
        );
        assert_eq!(scores["body.test"], 1_000);
        assert_eq!(
            world
                .phenotype(PersonId(1), &scores)
                .unwrap()
                .max_health_milli,
            1_000
        );
    }
    #[test]
    fn missing_evidence_and_invalid_development_are_inert() {
        let mut world = World::new(definitions(), SimTime(0)).unwrap();
        world
            .register(
                PersonId(1),
                Development {
                    maturity_milli: 1_000,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
        let before = world.clone();
        assert!(world.develop(PersonId(1), 1_001, 0).is_err());
        assert!(world.phenotype(PersonId(1), &BTreeMap::new()).is_err());
        assert_eq!(world, before);
    }
    #[test]
    fn refresh_deadline_and_training_are_owned_and_overflow_is_inert() {
        let mut world = World::new(definitions(), SimTime(0)).unwrap();
        assert_eq!(
            world.next_boundary(SimTime(0), SimTime(20)),
            Some(SimTime(10))
        );
        assert_eq!(world.training_from_levels([30, 40, 60]), 200);
        let before = world.clone();
        assert!(world.refresh_time(SimTime(9)).is_err());
        assert!(world.refresh_time(SimTime(u64::MAX)).is_err());
        assert_eq!(world, before);
        assert_eq!(world.refresh_time(SimTime(10)).unwrap(), SimTime(20));
        world.validate(SimTime(10)).unwrap();
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn optional_genetics_supplies_only_alleles_and_development_stays_local() {
        use game_genetics::HeritableCharacteristic;
        let definitions = definitions();
        let mut genes =
            game_genetics::World::new(game_genetics::Rules::default(), definitions.loci()).unwrap();
        genes.seed_person(7, PersonId(1)).unwrap();
        genes
            .set_alleles(
                PersonId(1),
                &game_genetics::LocusId("body.test".into()),
                [1_000, 1_000],
            )
            .unwrap();
        let before = genes.clone();
        let mut body = World::new(definitions, SimTime(0)).unwrap();
        body.register(
            PersonId(1),
            Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )
        .unwrap();
        assert_eq!(
            body.phenotype_from_genes(PersonId(1), &genes)
                .unwrap()
                .strength_milli,
            600
        );
        body.develop(PersonId(1), 1_000, 100).unwrap();
        assert_eq!(
            body.phenotype_from_genes(PersonId(1), &genes)
                .unwrap()
                .strength_milli,
            1_300
        );
        assert_eq!(genes, before);
    }
    #[test]
    fn state_round_trips_and_rejects_unknown_definition_fields() {
        let world = World::new(definitions(), SimTime(0)).unwrap();
        let bytes = serde_json::to_vec(&world).unwrap();
        assert_eq!(serde_json::from_slice::<World>(&bytes).unwrap(), world);
        let mut value = serde_json::to_value(definitions()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("misspelled_growth".into(), 1.into());
        assert!(serde_json::from_value::<Definitions>(value).is_err());
    }
}

//! Physical characteristic definitions and acquired development. Inheritance
//! supplies raw alleles; this module alone interprets them as body properties.
use game_core::{Error, PersonId, SimTime};
use game_ecs::{Schema, StableId, Store};
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
impl Development {
    pub fn validate(&self) -> Result<(), Error> {
        if self.maturity_milli > 1_000 || self.physical_training_milli > 10_000 {
            return Err(Error::InvalidConfiguration(
                "invalid physical development".into(),
            ));
        }
        Ok(())
    }
}

pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Development>("physiology/development", Development::validate)
}
pub fn development(store: &Store, person: PersonId) -> Option<&Development> {
    store.get(StableId(person.0))
}
pub fn development_mut(store: &mut Store, person: PersonId) -> Option<&mut Development> {
    store.get_mut(StableId(person.0))
}
pub fn all_developments(store: &Store) -> impl Iterator<Item = (PersonId, &Development)> {
    store
        .iter::<Development>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_development(
    store: &mut Store,
    person: PersonId,
    development: Development,
) -> Result<(), Error> {
    development.validate()?;
    store.insert_or_spawn(StableId(person.0), development)
}
pub fn remove_development(
    store: &mut Store,
    person: PersonId,
) -> Result<Option<Development>, Error> {
    if !store.contains(StableId(person.0)) {
        return Ok(None);
    }
    store.remove(StableId(person.0))
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
pub struct Runtime {
    pub definitions: Definitions,
    pub next_expression: SimTime,
}
impl Runtime {
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
            next_expression,
        })
    }
    pub fn register(
        &self,
        store: &mut Store,
        person: PersonId,
        development: Development,
    ) -> Result<(), Error> {
        if store.get::<Development>(StableId(person.0)).is_some() {
            return Err(Error::Unavailable);
        }
        self.check_development(development)?;
        insert_development(store, person, development)?;
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
    pub fn develop(
        &self,
        store: &mut Store,
        person: PersonId,
        maturity: u16,
        training: u16,
    ) -> Result<(), Error> {
        let development = Development {
            maturity_milli: maturity,
            physical_training_milli: training,
        };
        self.check_development(development)?;
        store
            .get::<Development>(StableId(person.0))
            .ok_or(Error::Unavailable)?;
        insert_development(store, person, development)?;
        Ok(())
    }
    pub fn phenotype(
        &self,
        store: &Store,
        person: PersonId,
        scores: &BTreeMap<String, i32>,
    ) -> Result<Phenotype, Error> {
        self.phenotype_with(store, person, |locus| {
            scores.get(locus).copied().ok_or(Error::Unavailable)
        })
    }
    fn phenotype_with(
        &self,
        store: &Store,
        person: PersonId,
        mut score: impl FnMut(&str) -> Result<i32, Error>,
    ) -> Result<Phenotype, Error> {
        let d = development(store, person).ok_or(Error::Unavailable)?;
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
        store: &Store,
        person: PersonId,
        genes: &impl game_genetics::AlleleSource,
    ) -> Result<Phenotype, Error> {
        self.phenotype_with(store, person, |locus| {
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
    pub fn validate(&self, store: &Store, now: SimTime) -> Result<(), Error> {
        if self.definitions.validate().is_err()
            || self.next_expression <= now
            || all_developments(store).any(|(_, d)| self.check_development(*d).is_err())
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
    fn store() -> Store {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        #[cfg(feature = "genetics")]
        game_genetics::register_components(&mut schema).unwrap();
        Store::with_schema(schema)
    }
    #[test]
    fn body_growth_preserves_injury_fraction_and_cannot_revive_a_dead_body() {
        assert_eq!(rescale_health(50, 500, true, 1_000, 1_200), (60, 600));
        assert_eq!(rescale_health(60, 600, true, 1_200, 1_000), (50, 500));
        assert_eq!(rescale_health(50, 500, false, 1_000, 1_200), (0, 0));
    }
    #[test]
    fn delayed_refresh_keeps_its_original_cadence_and_overflow_preserves_the_epoch() {
        let mut world = Runtime::new(definitions(), SimTime(0)).unwrap();
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
        let mut store = store();
        let world = Runtime::new(definitions(), SimTime(0)).unwrap();
        world
            .register(
                &mut store,
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
                .phenotype(&store, PersonId(1), &scores)
                .unwrap()
                .strength_milli,
            600
        );
        world.develop(&mut store, PersonId(1), 1_000, 100).unwrap();
        assert_eq!(
            world
                .phenotype(&store, PersonId(1), &scores)
                .unwrap()
                .strength_milli,
            1_300
        );
        assert_eq!(scores["body.test"], 1_000);
        assert_eq!(
            world
                .phenotype(&store, PersonId(1), &scores)
                .unwrap()
                .max_health_milli,
            1_000
        );
    }
    #[test]
    fn missing_evidence_and_invalid_development_are_inert() {
        let mut store = store();
        let world = Runtime::new(definitions(), SimTime(0)).unwrap();
        world
            .register(
                &mut store,
                PersonId(1),
                Development {
                    maturity_milli: 1_000,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
        let before = (world.clone(), store.clone());
        assert!(world.develop(&mut store, PersonId(1), 1_001, 0).is_err());
        assert!(
            world
                .phenotype(&store, PersonId(1), &BTreeMap::new())
                .is_err()
        );
        assert_eq!((world.clone(), store.clone()), before);
    }
    #[test]
    fn refresh_deadline_and_training_are_owned_and_overflow_is_inert() {
        let store = store();
        let mut world = Runtime::new(definitions(), SimTime(0)).unwrap();
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
        world.validate(&store, SimTime(10)).unwrap();
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn optional_genetics_supplies_only_alleles_and_development_stays_local() {
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
                &game_genetics::LocusId("body.test".into()),
                [1_000, 1_000],
            )
            .unwrap();
        let before = genes.clone();
        let genome_before = game_genetics::person_genome(&store, PersonId(1))
            .unwrap()
            .clone();
        let body = Runtime::new(definitions, SimTime(0)).unwrap();
        body.register(
            &mut store,
            PersonId(1),
            Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )
        .unwrap();
        assert_eq!(
            body.phenotype_from_genes(&store, PersonId(1), &genes.allele_source(&store))
                .unwrap()
                .strength_milli,
            600
        );
        body.develop(&mut store, PersonId(1), 1_000, 100).unwrap();
        assert_eq!(
            body.phenotype_from_genes(&store, PersonId(1), &genes.allele_source(&store))
                .unwrap()
                .strength_milli,
            1_300
        );
        assert_eq!(genes, before);
        assert_eq!(
            game_genetics::person_genome(&store, PersonId(1)),
            Some(&genome_before)
        );
        let acquired_before = *development(&store, PersonId(1)).unwrap();
        genes
            .set_alleles(
                &mut store,
                PersonId(1),
                &game_genetics::LocusId("body.test".into()),
                [0, 0],
            )
            .unwrap();
        assert_eq!(development(&store, PersonId(1)), Some(&acquired_before));
    }
    #[test]
    fn state_round_trips_and_rejects_unknown_definition_fields() {
        let world = Runtime::new(definitions(), SimTime(0)).unwrap();
        let bytes = serde_json::to_vec(&world).unwrap();
        assert_eq!(serde_json::from_slice::<Runtime>(&bytes).unwrap(), world);
        let mut value = serde_json::to_value(definitions()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("misspelled_growth".into(), 1.into());
        assert!(serde_json::from_value::<Definitions>(value).is_err());
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Foreign(u64);

    #[test]
    fn development_schema_roundtrip_and_removal_preserve_other_owner_columns() {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        assert_eq!(
            schema.keys().collect::<Vec<_>>(),
            vec!["physiology/development"]
        );
        assert!(register_components(&mut schema).is_err());
        schema
            .register::<Foreign>("other/column", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema.clone());
        store.insert_or_spawn(StableId(1), Foreign(77)).unwrap();
        let runtime = Runtime::new(definitions(), SimTime(0)).unwrap();
        runtime
            .register(
                &mut store,
                PersonId(1),
                Development {
                    maturity_milli: 0,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
        runtime
            .develop(&mut store, PersonId(1), 1_000, 100)
            .unwrap();
        assert_eq!(all_developments(&store).count(), 1);
        let restored_runtime: Runtime =
            serde_json::from_slice(&serde_json::to_vec(&runtime).unwrap()).unwrap();
        let restored = Store::restore(
            schema,
            serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(restored, store);
        restored_runtime.validate(&restored, SimTime(0)).unwrap();
        assert!(
            remove_development(&mut store, PersonId(1))
                .unwrap()
                .is_some()
        );
        assert!(store.contains(StableId(1)));
        assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(77)));
        assert!(
            remove_development(&mut store, PersonId(99))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn intrinsic_and_contextual_development_validation_remain_separate() {
        let runtime = Runtime::new(definitions(), SimTime(0)).unwrap();
        let mut store = store();
        let before = store.clone();
        assert!(
            insert_development(
                &mut store,
                PersonId(1),
                Development {
                    maturity_milli: 1_001,
                    physical_training_milli: 0
                }
            )
            .is_err()
        );
        assert!(
            runtime
                .register(
                    &mut store,
                    PersonId(0),
                    Development {
                        maturity_milli: 0,
                        physical_training_milli: 0
                    }
                )
                .is_err()
        );
        assert_eq!(store, before);
        insert_development(
            &mut store,
            PersonId(1),
            Development {
                maturity_milli: 1_000,
                physical_training_milli: 201,
            },
        )
        .unwrap();
        let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
        assert!(matches!(
            runtime.validate(&restored, SimTime(0)),
            Err(Error::InvalidSave(_))
        ));
        development_mut(&mut store, PersonId(1))
            .unwrap()
            .maturity_milli = 1_001;
        assert!(Store::restore(store.schema().clone(), store.snapshot().unwrap()).is_err());
        let mut json = serde_json::to_value(runtime).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert("people".into(), serde_json::json!({}));
        assert!(serde_json::from_value::<Runtime>(json).is_err());
    }

    #[test]
    fn schema_rejection_of_development_is_atomic() {
        let mut schema = Schema::new();
        schema
            .register::<Development>("physiology/development", |row| {
                row.validate()?;
                if row.physical_training_milli > 0 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        let runtime = Runtime::new(definitions(), SimTime(0)).unwrap();
        runtime
            .register(
                &mut store,
                PersonId(1),
                Development {
                    maturity_milli: 0,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            runtime.develop(&mut store, PersonId(1), 1_000, 100),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

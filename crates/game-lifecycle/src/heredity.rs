//! Reproductive-role interpretation. Genetics stores and transmits alleles but
//! does not decide what they mean for conception or biological development.
use crate::Reproduction;
use game_core::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
            && self.min <= self.max
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinition {
    pub alleles: [i32; 2],
    pub reproduction: Reproduction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub inheritance: GeneBinding,
    pub roles: Vec<RoleDefinition>,
}
impl Definition {
    pub fn validate(&self) -> Result<(), Error> {
        let mut combinations = BTreeSet::new();
        let valid = self.inheritance.valid()
            && !self.inheritance.mutable
            && !self.roles.is_empty()
            && self.roles.iter().all(|role| {
                role.alleles
                    .iter()
                    .all(|n| (self.inheritance.min..=self.inheritance.max).contains(n))
                    && combinations.insert(ordered(role.alleles))
            });
        if !valid {
            return Err(Error::InvalidConfiguration(
                "invalid reproductive heredity definitions".into(),
            ));
        }
        // Every reachable offspring combination must have a biological meaning
        // before conception can freeze a genome. This categorical mechanism does
        // not mutate alleles; numerical predispositions may still mutate elsewhere.
        for gestator in self
            .roles
            .iter()
            .filter(|r| r.reproduction == Reproduction::Gestates)
        {
            for supplier in self
                .roles
                .iter()
                .filter(|r| r.reproduction == Reproduction::SuppliesSeed)
            {
                for first in gestator.alleles {
                    for second in supplier.alleles {
                        if !combinations.contains(&ordered([first, second])) {
                            return Err(Error::InvalidConfiguration(
                                "reproductive heredity omits a reachable offspring role".into(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// The ordering of parental homologs has no effect on expression.
    pub fn interpret(&self, alleles: [i32; 2]) -> Result<Reproduction, Error> {
        self.roles
            .iter()
            .find(|role| ordered(role.alleles) == ordered(alleles))
            .map(|role| role.reproduction)
            .ok_or(Error::Unavailable)
    }
    /// Supplies a configured founding combination, without teaching genetics
    /// anything about reproductive roles.
    pub fn founder_alleles(&self, role: Reproduction) -> Result<[i32; 2], Error> {
        self.roles
            .iter()
            .find(|r| r.reproduction == role)
            .or_else(|| {
                (role == Reproduction::Neither)
                    .then(|| self.roles.first())
                    .flatten()
            })
            .map(|r| r.alleles)
            .ok_or(Error::Unavailable)
    }
    #[cfg(feature = "genetics")]
    pub fn interpret_genes(
        &self,
        person: game_core::PersonId,
        genes: &impl game_genetics::AlleleSource,
    ) -> Result<Reproduction, Error> {
        self.interpret(genes.alleles_named(person, &self.inheritance.locus)?)
    }
}
fn ordered(mut alleles: [i32; 2]) -> [i32; 2] {
    alleles.sort();
    alleles
}
#[cfg(feature = "genetics")]
impl game_genetics::HeritableCharacteristic for Definition {
    fn loci(&self) -> Vec<game_genetics::LocusDefinition> {
        game_genetics::HeritableCharacteristic::loci(&self.inheritance)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> Definition {
        Definition {
            inheritance: GeneBinding {
                locus: "reproduction.test".into(),
                chromosome: "pair".into(),
                position: 0,
                min: 0,
                max: 1,
                initial_alleles: vec![0, 1],
                mutation_step: 0,
                mutable: false,
            },
            roles: vec![
                RoleDefinition {
                    alleles: [0, 0],
                    reproduction: Reproduction::Gestates,
                },
                RoleDefinition {
                    alleles: [0, 1],
                    reproduction: Reproduction::SuppliesSeed,
                },
            ],
        }
    }
    #[test]
    fn configured_pairs_determine_roles_without_parental_order() {
        let d = definition();
        d.validate().unwrap();
        assert_eq!(d.interpret([0, 0]).unwrap(), Reproduction::Gestates);
        assert_eq!(d.interpret([0, 1]).unwrap(), Reproduction::SuppliesSeed);
        assert_eq!(d.interpret([1, 0]).unwrap(), Reproduction::SuppliesSeed);
        assert!(d.interpret([1, 1]).is_err());
        assert_eq!(d.founder_alleles(Reproduction::Gestates).unwrap(), [0, 0]);
    }
    #[test]
    fn changed_content_changes_interpretation_and_ambiguous_rules_are_rejected() {
        let mut d = definition();
        d.roles[0].reproduction = Reproduction::Neither;
        assert_eq!(d.interpret([0, 0]).unwrap(), Reproduction::Neither);
        d.roles.push(RoleDefinition {
            alleles: [1, 0],
            reproduction: Reproduction::Gestates,
        });
        assert!(d.validate().is_err());
    }
    #[test]
    fn mutable_or_empty_categorical_role_definitions_are_rejected() {
        let mut d = definition();
        d.inheritance.mutable = true;
        assert!(d.validate().is_err());
        d.inheritance.mutable = false;
        d.roles.clear();
        assert!(d.validate().is_err());
    }
    #[test]
    fn every_possible_offspring_of_reproductive_roles_requires_an_authored_role() {
        let mut d = definition();
        d.inheritance.max = 2;
        d.inheritance.initial_alleles.push(2);
        d.roles[0].alleles = [0, 1];
        d.roles[1].alleles = [0, 2];
        // These parents can transmit [0,0], [0,2], [1,0], or [1,2].
        assert!(d.validate().is_err());
        d.roles.push(RoleDefinition {
            alleles: [0, 0],
            reproduction: Reproduction::Neither,
        });
        assert!(d.validate().is_err());
        d.roles.push(RoleDefinition {
            alleles: [1, 2],
            reproduction: Reproduction::Neither,
        });
        d.validate().unwrap();
        assert_eq!(d.interpret([2, 1]).unwrap(), Reproduction::Neither);
    }
    #[test]
    fn allele_labels_are_content_and_do_not_encode_human_chromosome_constants() {
        let mut d = definition();
        d.inheritance.min = 10;
        d.inheritance.max = 20;
        d.inheritance.initial_alleles = vec![10, 20];
        d.roles[0].alleles = [10, 10];
        d.roles[1].alleles = [10, 20];
        d.validate().unwrap();
        assert_eq!(d.founder_alleles(Reproduction::Gestates).unwrap(), [10, 10]);
        assert_eq!(d.interpret([20, 10]).unwrap(), Reproduction::SuppliesSeed);
        assert!(d.interpret([20, 20]).is_err());
    }
    #[cfg(feature = "genetics")]
    #[test]
    fn optional_inheritance_remains_ignorant_of_reproductive_roles() {
        let mut schema = game_ecs::Schema::new();
        game_genetics::register_components(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        use game_genetics::HeritableCharacteristic;
        let definition = definition();
        let mut genes =
            game_genetics::Runtime::new(game_genetics::Rules::default(), definition.loci())
                .unwrap();
        genes
            .seed_person(&mut store, 9, game_core::PersonId(1))
            .unwrap();
        genes
            .set_alleles(
                &mut store,
                game_core::PersonId(1),
                &game_genetics::LocusId("reproduction.test".into()),
                [1, 0],
            )
            .unwrap();
        assert_eq!(
            definition
                .interpret_genes(game_core::PersonId(1), &genes.allele_source(&store))
                .unwrap(),
            Reproduction::SuppliesSeed
        );
        genes
            .set_alleles(
                &mut store,
                game_core::PersonId(1),
                &game_genetics::LocusId("reproduction.test".into()),
                [1, 1],
            )
            .unwrap();
        assert!(
            definition
                .interpret_genes(game_core::PersonId(1), &genes.allele_source(&store))
                .is_err()
        );
    }
    #[test]
    fn unknown_fields_are_rejected_and_definitions_round_trip() {
        let d = definition();
        let bytes = serde_json::to_vec(&d).unwrap();
        assert_eq!(serde_json::from_slice::<Definition>(&bytes).unwrap(), d);
        let mut value = serde_json::to_value(d).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), 1.into());
        assert!(serde_json::from_value::<Definition>(value).is_err());
    }
}

#[cfg(feature = "genetics")]
pub struct PreparedReproduction {
    definition: Definition,
    actor: game_core::PersonId,
    alleles: [i32; 2],
    before: crate::Life,
    after: crate::Life,
}
#[cfg(feature = "genetics")]
impl Definition {
    /// Receive inherited alleles through the lifecycle owner rather than editing Life externally.
    pub fn prepare_reproduction(
        &self,
        store: &game_ecs::Store,
        actor: game_core::PersonId,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<PreparedReproduction, Error> {
        self.validate()?;
        let before = crate::life(store, actor).ok_or(Error::Unavailable)?.clone();
        let alleles = source.alleles_named(actor, &self.inheritance.locus)?;
        let mut after = before.clone();
        after.reproduction = self.interpret(alleles)?;
        after.validate()?;
        Ok(PreparedReproduction {
            definition: self.clone(),
            actor,
            alleles,
            before,
            after,
        })
    }
    pub fn validate_lives(
        &self,
        store: &game_ecs::Store,
        source: &impl game_genetics::AlleleSource,
    ) -> Result<(), Error> {
        self.validate()
            .map_err(|_| Error::InvalidSave("invalid reproductive heredity".into()))?;
        for (actor, life) in crate::all_lives(store) {
            let interpreted = self
                .interpret_genes(actor, source)
                .map_err(|_| Error::InvalidSave("invalid reproductive alleles".into()))?;
            if life.reproduction != Reproduction::Neither && interpreted != life.reproduction {
                return Err(Error::InvalidSave(
                    "reproductive role differs from inheritance".into(),
                ));
            }
        }
        Ok(())
    }
}
#[cfg(feature = "genetics")]
impl PreparedReproduction {
    pub fn stage_into(
        self,
        definition: &Definition,
        store: &game_ecs::Store,
        source: &impl game_genetics::AlleleSource,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        if definition != &self.definition
            || source.alleles_named(self.actor, &definition.inheritance.locus)? != self.alleles
        {
            return Err(Error::Unavailable);
        }
        let mut reads = game_ecs::ReadSet::new();
        reads.expect_row(game_ecs::StableId(self.actor.0), Some(self.before));
        reads.validate(store)?;
        batch.insert(game_ecs::StableId(self.actor.0), self.after);
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<crate::Life>(game_ecs::StableId(self.actor.0));
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(
        self,
        definition: &Definition,
        store: &mut game_ecs::Store,
        read_alleles: impl Fn(&game_ecs::Store, game_core::PersonId, &str) -> Result<[i32; 2], Error>,
    ) -> Result<(), Error> {
        struct Source<'a, F> {
            store: &'a game_ecs::Store,
            read: &'a F,
        }
        impl<F: Fn(&game_ecs::Store, game_core::PersonId, &str) -> Result<[i32; 2], Error>>
            game_genetics::AlleleSource for Source<'_, F>
        {
            fn alleles_named(
                &self,
                actor: game_core::PersonId,
                locus: &str,
            ) -> Result<[i32; 2], Error> {
                (self.read)(self.store, actor, locus)
            }
        }
        let mut batch = game_ecs::Batch::new();
        let guard = self.stage_into(
            definition,
            store,
            &Source {
                store,
                read: &read_alleles,
            },
            &mut batch,
        )?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

#[cfg(all(test, feature = "genetics"))]
mod receiving_tests {
    use super::*;
    use game_core::{LifeStage, PersonId, PregnancyId, SimDuration, SimTime};
    use game_genetics::HeritableCharacteristic;
    fn definition() -> Definition {
        Definition {
            inheritance: GeneBinding {
                locus: "role".into(),
                chromosome: "pair".into(),
                position: 0,
                min: 0,
                max: 1,
                initial_alleles: vec![0, 1],
                mutation_step: 0,
                mutable: false,
            },
            roles: vec![
                RoleDefinition {
                    alleles: [0, 0],
                    reproduction: Reproduction::Gestates,
                },
                RoleDefinition {
                    alleles: [0, 1],
                    reproduction: Reproduction::SuppliesSeed,
                },
                RoleDefinition {
                    alleles: [1, 1],
                    reproduction: Reproduction::SuppliesSeed,
                },
            ],
        }
    }
    fn life(id: PersonId) -> crate::Life {
        crate::Life {
            id,
            born: SimTime(0),
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction: Reproduction::Gestates,
            parents: vec![],
            caregivers: BTreeSet::new(),
            preference: None,
        }
    }
    #[test]
    fn unpublished_birth_genome_and_receiving_reproductive_role_commit_together_without_assembly() {
        let definition = definition();
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        game_genetics::register_components(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        let mut genes = game_genetics::Runtime::new(
            game_genetics::Rules {
                mutation_chance: 0,
                crossover_chance: 0,
            },
            definition.loci(),
        )
        .unwrap();
        for (actor, value) in [(PersonId(1), 0), (PersonId(2), 1)] {
            genes.seed_person(&mut store, 7, actor).unwrap();
            genes
                .set_alleles(
                    &mut store,
                    actor,
                    &game_genetics::LocusId("role".into()),
                    [value; 2],
                )
                .unwrap();
        }
        genes
            .conceive(
                &mut store,
                7,
                PregnancyId(3),
                [PersonId(1), PersonId(2)],
                SimTime(1),
            )
            .unwrap();
        let child = PersonId(4);
        crate::insert_life(&mut store, life(child)).unwrap();
        let birth = genes
            .prepare_birth(&store, PregnancyId(3), child, SimTime(2))
            .unwrap();
        let source = genes.genome_source(child, birth.genome().unwrap());
        let received = definition
            .prepare_reproduction(&store, child, &source)
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        let mut batch = game_ecs::Batch::new();
        let role_guard = received
            .stage_into(&definition, &store, &source, &mut batch)
            .unwrap();
        let genes_guard = birth.stage_into(&genes, &store, &mut batch).unwrap();
        role_guard.validate(&store, &batch).unwrap();
        genes_guard.validate(&genes, &store, &batch).unwrap();
        assert_eq!(store.snapshot().unwrap(), snapshot);
        store.apply_batch(batch).unwrap();
        assert_eq!(
            crate::life(&store, child).unwrap().reproduction,
            Reproduction::SuppliesSeed
        );
        assert_eq!(
            game_genetics::inheritance(&store, PregnancyId(3))
                .unwrap()
                .born,
            Some(child)
        );
        definition
            .validate_lives(&store, &genes.allele_source(&store))
            .unwrap();
        let stale = definition
            .prepare_reproduction(&store, child, &genes.allele_source(&store))
            .unwrap();
        crate::life_mut(&mut store, child).unwrap().preference =
            Some((game_core::ChildhoodInterest::Movement, SimTime(2)));
        let before = store.snapshot().unwrap();
        assert_eq!(
            stale.commit(&definition, &mut store, |store, actor, locus| genes
                .alleles_named(store, actor, locus)),
            Err(Error::Unavailable)
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
}

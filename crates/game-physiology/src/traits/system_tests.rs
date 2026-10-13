use super::*;
use game_core::Vitals;
use game_genetics::{AlleleSource, HeritableCharacteristic};

struct Source(i32);
impl AlleleSource for Source {
    fn alleles_named(&self, _: PersonId, _: &str) -> Result<[i32; 2], Error> {
        Ok([self.0; 2])
    }
}
struct Unread;
impl AlleleSource for Unread {
    fn alleles_named(&self, _: PersonId, _: &str) -> Result<[i32; 2], Error> {
        panic!("no expression should be read")
    }
}
fn definitions() -> Definitions {
    let mut definitions = super::tests::definitions();
    let mut health = definitions.traits[0].clone();
    health.kind = PhysicalTrait::Constitution;
    health.inheritance.locus = "body.health".into();
    health.inheritance.position = 1;
    definitions.traits.push(health);
    definitions
}
fn schema() -> Schema {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    crate::register_components(&mut schema).unwrap();
    game_genetics::register_components(&mut schema).unwrap();
    schema
}
fn body(alive: bool) -> crate::Body {
    crate::Body {
        vitals: Vitals {
            health: if alive { 50 } else { 0 },
            hunger: 123,
            fatigue: 777,
            alive,
        },
        fatigue_remainder: 17,
        health_remainder: 500,
    }
}
fn fixture() -> (Runtime, Store) {
    let runtime = Runtime::new(definitions(), SimTime(0)).unwrap();
    let mut store = Store::with_schema(schema());
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
    crate::insert_body(&mut store, PersonId(1), body(true)).unwrap();
    (runtime, store)
}
fn after() -> Development {
    Development {
        maturity_milli: 1_000,
        physical_training_milli: 100,
    }
}

#[test]
fn inherited_genome_and_physical_development_compose_without_the_game_assembly() {
    let mut physical = Runtime::new(definitions(), SimTime(0)).unwrap();
    let mut genes = game_genetics::Runtime::new(
        game_genetics::Rules {
            mutation_chance: 0,
            crossover_chance: 1_000,
        },
        physical.definitions.loci(),
    )
    .unwrap();
    let mut store = Store::with_schema(schema());
    for (parent, value) in [(PersonId(1), 0), (PersonId(2), 1_000)] {
        genes.seed_person(&mut store, 7, parent).unwrap();
        for definition in &physical.definitions.traits {
            genes
                .set_alleles(
                    &mut store,
                    parent,
                    &game_genetics::LocusId(definition.inheritance.locus.clone()),
                    [value; 2],
                )
                .unwrap();
        }
        physical
            .register(
                &mut store,
                parent,
                Development {
                    maturity_milli: 1_000,
                    physical_training_milli: 0,
                },
            )
            .unwrap();
    }
    let pregnancy = game_core::PregnancyId(3);
    let child = PersonId(4);
    genes
        .conceive(
            &mut store,
            7,
            pregnancy,
            [PersonId(1), PersonId(2)],
            SimTime(10),
        )
        .unwrap();
    genes
        .born(&mut store, pregnancy, child, SimTime(20))
        .unwrap();
    physical
        .register(
            &mut store,
            child,
            Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )
        .unwrap();
    for definition in &physical.definitions.traits {
        assert_eq!(
            genes
                .alleles_named(&store, child, &definition.inheritance.locus)
                .unwrap(),
            [0, 1_000]
        );
    }
    let newborn = physical
        .phenotype_from_genes(&store, child, &genes.allele_source(&store))
        .unwrap();
    assert_eq!(
        (newborn.strength_milli, newborn.max_health_milli),
        (500, 800)
    );
    let genome = game_genetics::person_genome(&store, child).unwrap().clone();
    physical.develop(&mut store, child, 1_000, 0).unwrap();
    let grown = physical
        .phenotype_from_genes(&store, child, &genes.allele_source(&store))
        .unwrap();
    assert_eq!(
        (grown.strength_milli, grown.max_health_milli),
        (1_000, 1_000)
    );
    for parent in [PersonId(1), PersonId(2)] {
        let phenotype = physical
            .phenotype_from_genes(&store, parent, &genes.allele_source(&store))
            .unwrap();
        assert_ne!(phenotype.strength_milli, grown.strength_milli);
        assert_eq!(phenotype.strength_milli.abs_diff(grown.strength_milli), 200);
    }
    physical.develop(&mut store, child, 1_000, 100).unwrap();
    let trained = physical
        .phenotype_from_genes(&store, child, &genes.allele_source(&store))
        .unwrap();
    assert_eq!(trained.strength_milli, 1_100);
    assert_eq!(game_genetics::person_genome(&store, child), Some(&genome));

    physical.reset_epoch(SimTime(20)).unwrap();
    let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    game_genetics::validate(&genes, &restored, SimTime(20)).unwrap();
    physical.validate(&restored, SimTime(20)).unwrap();
    assert_eq!(
        physical
            .phenotype_from_genes(&restored, child, &genes.allele_source(&restored))
            .unwrap(),
        trained
    );
    assert_eq!(
        game_genetics::inheritance(&restored, pregnancy)
            .unwrap()
            .born,
        Some(child)
    );
}

#[test]
fn development_selects_existing_physical_skills_and_rate_scaling_preserves_exact_fractions() {
    let (runtime, _) = fixture();
    assert_eq!(
        runtime
            .development_from_facts(
                700,
                &[
                    (Skill::Craft, 10),
                    (Skill::Farming, 20),
                    (Skill::Combat, 30),
                    (Skill::Medicine, 500)
                ]
            )
            .unwrap(),
        Development {
            maturity_milli: 700,
            physical_training_milli: 120
        }
    );
    assert_eq!(
        runtime
            .development_from_facts(1_000, &[(Skill::Craft, u16::MAX)])
            .unwrap()
            .physical_training_milli,
        200
    );
    assert!(runtime.development_from_facts(1_001, &[]).is_err());
    assert_eq!(
        scale_work_rate(
            SkillRate {
                numerator: 1_003,
                denominator: 777
            },
            1_234
        )
        .unwrap(),
        SkillRate {
            numerator: 1_237_702,
            denominator: 777_000
        }
    );
    assert_eq!(
        scale_work_rate(
            SkillRate {
                numerator: u64::MAX,
                denominator: 1
            },
            2
        ),
        Err(Error::Overflow)
    );
    assert_eq!(
        scale_work_rate(
            SkillRate {
                numerator: 1,
                denominator: u64::MAX
            },
            1_000
        ),
        Err(Error::Overflow)
    );
}

#[test]
fn refresh_with_actual_gene_rows_preserves_injury_fraction_death_and_other_body_fields() {
    let (mut runtime, mut store) = fixture();
    runtime
        .register(
            &mut store,
            PersonId(2),
            Development {
                maturity_milli: 0,
                physical_training_milli: 0,
            },
        )
        .unwrap();
    crate::insert_body(&mut store, PersonId(2), body(false)).unwrap();
    let mut genes =
        game_genetics::Runtime::new(game_genetics::Rules::default(), runtime.definitions.loci())
            .unwrap();
    for actor in [PersonId(1), PersonId(2)] {
        genes.seed_person(&mut store, 7, actor).unwrap();
        for definition in &runtime.definitions.traits {
            genes
                .set_alleles(
                    &mut store,
                    actor,
                    &game_genetics::LocusId(definition.inheritance.locus.clone()),
                    [1_000; 2],
                )
                .unwrap();
        }
    }
    let genome_before = game_genetics::person_genome(&store, PersonId(1))
        .unwrap()
        .clone();
    let prepared = runtime
        .prepare_refresh(
            &store,
            SimTime(25),
            &[(PersonId(2), after()), (PersonId(1), after())],
            &genes.allele_source(&store),
        )
        .unwrap()
        .unwrap();
    assert_eq!(prepared.next_deadline(), SimTime(30));
    let (_, old, new) = prepared.phenotypes().next().unwrap();
    assert_eq!((old.strength_milli, new.strength_milli), (600, 1_300));
    assert_eq!((old.max_health_milli, new.max_health_milli), (960, 1_200));
    prepared
        .commit(&mut runtime, &mut store, |store, actor, locus| {
            genes.alleles_named(store, actor, locus)
        })
        .unwrap();
    let alive = crate::body(&store, PersonId(1)).unwrap();
    assert_eq!((alive.vitals.health, alive.health_remainder), (63, 125));
    assert_eq!(
        (
            alive.vitals.hunger,
            alive.vitals.fatigue,
            alive.fatigue_remainder
        ),
        (123, 777, 17)
    );
    let dead = crate::body(&store, PersonId(2)).unwrap();
    assert_eq!(
        (dead.vitals.health, dead.health_remainder, dead.vitals.alive),
        (0, 0, false)
    );
    assert_eq!(runtime.next_expression, SimTime(30));
    assert_eq!(
        game_genetics::person_genome(&store, PersonId(1)),
        Some(&genome_before)
    );
}

#[test]
fn stale_header_body_development_or_used_alleles_reject_before_any_refresh_write() {
    for mutation in 0..4 {
        let (mut runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_refresh(
                &store,
                SimTime(10),
                &[(PersonId(1), after())],
                &Source(1_000),
            )
            .unwrap()
            .unwrap();
        let mut value = 1_000;
        match mutation {
            0 => runtime.definitions.max_training += 1,
            1 => {
                development_mut(&mut store, PersonId(1))
                    .unwrap()
                    .maturity_milli += 1
            }
            2 => {
                crate::body_mut(&mut store, PersonId(1))
                    .unwrap()
                    .vitals
                    .fatigue += 1
            }
            3 => value = 0,
            _ => unreachable!(),
        }
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            prepared.commit(&mut runtime, &mut store, |_, _, _| Ok([value; 2])),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn rejected_body_or_development_admission_keeps_the_other_rows_and_deadline_unchanged() {
    for reject_body in [false, true] {
        let mut schema = Schema::new();
        game_genetics::register_components(&mut schema).unwrap();
        if reject_body {
            register_components(&mut schema).unwrap();
            schema
                .register::<crate::Body>("physiology/body", |body| {
                    body.validate()?;
                    if body.vitals.health > 60 {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        } else {
            crate::register_components(&mut schema).unwrap();
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
        }
        let mut runtime = Runtime::new(definitions(), SimTime(0)).unwrap();
        let mut store = Store::with_schema(schema);
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
        crate::insert_body(&mut store, PersonId(1), body(true)).unwrap();
        let prepared = runtime
            .prepare_refresh(
                &store,
                SimTime(10),
                &[(PersonId(1), after())],
                &Source(1_000),
            )
            .unwrap()
            .unwrap();
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            prepared.commit(&mut runtime, &mut store, |_, _, _| Ok([1_000; 2])),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn refresh_collisions_membership_and_overflow_cannot_skip_or_partially_update_people() {
    let (mut runtime, mut store) = fixture();
    assert!(
        runtime
            .prepare_refresh(&store, SimTime(0), &[], &Unread)
            .unwrap()
            .is_none()
    );
    assert!(
        runtime
            .prepare_refresh(
                &store,
                SimTime(10),
                &[(PersonId(1), after()), (PersonId(1), after())],
                &Unread
            )
            .is_err()
    );
    assert!(
        runtime
            .prepare_refresh(&store, SimTime(10), &[], &Unread)
            .is_err()
    );
    let before = (runtime.clone(), store.clone());
    assert!(matches!(
        runtime.prepare_refresh(
            &store,
            SimTime(u64::MAX),
            &[(PersonId(1), after())],
            &Unread
        ),
        Err(Error::Overflow)
    ));
    assert_eq!((runtime.clone(), store.clone()), before);
    let prepared = runtime
        .prepare_refresh(
            &store,
            SimTime(10),
            &[(PersonId(1), after())],
            &Source(1_000),
        )
        .unwrap()
        .unwrap();
    runtime.register(&mut store, PersonId(2), after()).unwrap();
    let before = (runtime.clone(), store.clone());
    assert_eq!(
        prepared.commit(&mut runtime, &mut store, |_, _, _| Ok([1_000; 2])),
        Err(Error::Unavailable)
    );
    assert_eq!((runtime, store), before);
}

#[test]
fn refresh_entry_point_selects_its_population_and_interprets_raw_training_facts() {
    let (mut runtime, mut store) = fixture();
    assert!(
        runtime
            .prepare_refresh_from_facts(&store, SimTime(0), &Unread, |_| panic!("not due"))
            .unwrap()
            .is_none()
    );
    let prepared = runtime
        .prepare_refresh_from_facts(&store, SimTime(10), &Source(1_000), |actor| {
            assert_eq!(actor, PersonId(1));
            Ok((1_000, vec![(Skill::Craft, 20), (Skill::Medicine, 1_000)]))
        })
        .unwrap()
        .unwrap();
    prepared
        .commit(&mut runtime, &mut store, |_, _, _| Ok([1_000; 2]))
        .unwrap();
    assert_eq!(
        development(&store, PersonId(1))
            .unwrap()
            .physical_training_milli,
        40
    );
    runtime
        .validate_references(&store, &BTreeSet::from([PersonId(1)]), &Source(1_000))
        .unwrap();
    assert!(
        runtime
            .validate_references(&store, &BTreeSet::new(), &Source(1_000))
            .is_err()
    );
    runtime.register_newborn(&mut store, PersonId(2)).unwrap();
    assert_eq!(
        development(&store, PersonId(2)).unwrap(),
        &Development {
            maturity_milli: 0,
            physical_training_milli: 0
        }
    );
}

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

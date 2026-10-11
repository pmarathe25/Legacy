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
fn store() -> Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    Store::with_schema(schema)
}
fn parents(rules: Rules) -> (Runtime, Store) {
    let mut store = store();
    let mut world = Runtime::new(rules, definitions()).unwrap();
    world.seed_person(&mut store, 7, PersonId(1)).unwrap();
    world.seed_person(&mut store, 7, PersonId(2)).unwrap();
    for definition in definitions() {
        world
            .set_alleles(&mut store, PersonId(1), &definition.id, [-500, 500])
            .unwrap();
        world
            .set_alleles(&mut store, PersonId(2), &definition.id, [-250, 250])
            .unwrap();
    }
    (world, store)
}
#[test]
fn schema_and_rng_are_independent_of_definition_and_operation_order() {
    let mut reversed = definitions();
    reversed.reverse();
    reversed[0].initial_alleles.reverse();
    let mut store_a = store();
    let mut store_b = store();
    let mut a = Runtime::new(Rules::default(), definitions()).unwrap();
    let mut b = Runtime::new(Rules::default(), reversed).unwrap();
    for person in [PersonId(1), PersonId(2)] {
        a.seed_person(&mut store_a, 7, person).unwrap();
    }
    for person in [PersonId(2), PersonId(1)] {
        b.seed_person(&mut store_b, 7, person).unwrap();
    }
    for pregnancy in [PregnancyId(3), PregnancyId(4)] {
        a.conceive(
            &mut store_a,
            9,
            pregnancy,
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    }
    for pregnancy in [PregnancyId(4), PregnancyId(3)] {
        b.conceive(
            &mut store_b,
            9,
            pregnancy,
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    }
    assert_eq!(a, b);
    assert_eq!(store_a, store_b);
    validate(&a, &store_a, SimTime(1)).unwrap();
}
#[test]
fn linked_crossover_traces_identify_actual_parental_homologs() {
    let (mut world, mut store) = parents(Rules {
        mutation_chance: 0,
        crossover_chance: 1_000,
    });
    world
        .conceive(
            &mut store,
            4,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    let inheritance = inheritance(&store, PregnancyId(3)).unwrap();
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
                    .allele_ids(&store, PersonId(side as u64 + 1), &trace.locus)
                    .unwrap()[usize::from(trace.source_homolog)],
                trace.source_variant
            );
        }
    }
    validate(&world, &store, SimTime(1)).unwrap();
}
#[test]
fn mutation_respects_owner_bounds_and_immutable_loci_without_rewriting_parents() {
    let mut definitions = definitions();
    definitions[2].mutable = false;
    definitions[2].mutation_step = 0;
    let mut store = store();
    let mut world = Runtime::new(
        Rules {
            mutation_chance: 10_000,
            crossover_chance: 0,
        },
        definitions,
    )
    .unwrap();
    world.seed_person(&mut store, 3, PersonId(1)).unwrap();
    world.seed_person(&mut store, 3, PersonId(2)).unwrap();
    let before: Vec<_> = all_person_genomes(&store)
        .map(|(id, row)| (id, row.clone()))
        .collect();
    world
        .conceive(
            &mut store,
            4,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    assert_eq!(
        all_person_genomes(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<Vec<_>>(),
        before
    );
    for trace in inheritance(&store, PregnancyId(3))
        .unwrap()
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
    validate(&world, &store, SimTime(1)).unwrap();
}
#[test]
fn frozen_conception_birth_retry_and_save_roundtrip() {
    let (mut world, mut store) = parents(Rules::default());
    world
        .conceive(
            &mut store,
            8,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    let frozen = inheritance(&store, PregnancyId(3))
        .unwrap()
        .genome
        .clone()
        .unwrap();
    assert_eq!(
        world.set_alleles(
            &mut store,
            PersonId(1),
            &LocusId("opaque.alpha".into()),
            [0, 0]
        ),
        Err(Error::Unavailable)
    );
    let before_retry = (world.clone(), store.clone());
    world
        .conceive(
            &mut store,
            999,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    assert_eq!((world.clone(), store.clone()), before_retry);
    assert_eq!(
        world.conceive(
            &mut store,
            8,
            PregnancyId(3),
            [PersonId(2), PersonId(1)],
            SimTime(1)
        ),
        Err(Error::SequenceConflict)
    );
    world
        .born(&mut store, PregnancyId(3), PersonId(4), SimTime(2))
        .unwrap();
    assert_eq!(person_genome(&store, PersonId(4)).unwrap().genome, frozen);
    let before_retry = (world.clone(), store.clone());
    world
        .born(&mut store, PregnancyId(3), PersonId(4), SimTime(3))
        .unwrap();
    assert_eq!((world.clone(), store.clone()), before_retry);
    assert_eq!(
        world.born(&mut store, PregnancyId(3), PersonId(5), SimTime(3)),
        Err(Error::Unavailable)
    );
    let restored: Runtime = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    let restored_store = Store::restore(
        store.schema().clone(),
        serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(world, restored);
    assert_eq!(store, restored_store);
    validate(&restored, &restored_store, SimTime(3)).unwrap();
}
#[test]
fn pregnancy_ending_retains_frozen_trace_and_prevents_birth() {
    let (mut world, mut store) = parents(Rules::default());
    world
        .conceive(
            &mut store,
            8,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(2),
        )
        .unwrap();
    world.end_pregnancy(&mut store, PregnancyId(3), SimTime(1));
    assert_eq!(inheritance(&store, PregnancyId(3)).unwrap().ended_at, None);
    world.end_pregnancy(&mut store, PregnancyId(3), SimTime(3));
    world.end_pregnancy(&mut store, PregnancyId(3), SimTime(4));
    assert_eq!(
        inheritance(&store, PregnancyId(3)).unwrap().ended_at,
        Some(SimTime(3))
    );
    assert_eq!(
        world.born(&mut store, PregnancyId(3), PersonId(4), SimTime(4)),
        Err(Error::Unavailable)
    );
    validate(&world, &store, SimTime(4)).unwrap();
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
        let mut store = store();
        let mut world = Runtime::new(
            Rules {
                mutation_chance: 10_000,
                crossover_chance: 1_000,
            },
            definitions,
        )
        .unwrap();
        world.seed_person(&mut store, 3, PersonId(1)).unwrap();
        world.seed_person(&mut store, 3, PersonId(2)).unwrap();
        world
            .conceive(
                &mut store,
                3,
                PregnancyId(3),
                [PersonId(1), PersonId(2)],
                SimTime(1),
            )
            .unwrap();
        world
            .born(&mut store, PregnancyId(3), PersonId(4), SimTime(2))
            .unwrap();
        assert_eq!(
            inheritance(&store, PregnancyId(3)).unwrap().transmissions[0].len(),
            usize::from(size)
        );
        validate(&world, &store, SimTime(2)).unwrap();
    }
}
#[test]
fn founder_assignment_is_atomic_and_raw_values_are_available_to_owners() {
    let (mut world, mut store) = parents(Rules::default());
    let locus = LocusId("opaque.alpha".into());
    let before = (world.clone(), store.clone());
    assert!(
        world
            .set_alleles(&mut store, PersonId(1), &locus, [20, 501])
            .is_err()
    );
    assert_eq!((world.clone(), store.clone()), before);
    world
        .set_alleles(&mut store, PersonId(1), &locus, [20, 21])
        .unwrap();
    assert_eq!(
        world.alleles(&store, PersonId(1), &locus).unwrap(),
        [20, 21]
    );
    validate(&world, &store, SimTime(0)).unwrap();
}
#[test]
fn borrowed_and_typed_allele_lookups_agree_across_chromosome_boundaries() {
    let (world, store) = parents(Rules::default());
    let source = world.allele_source(&store);
    for definition in &world.definitions {
        assert_eq!(
            world.alleles(&store, PersonId(1), &definition.id),
            world.alleles_named(&store, PersonId(1), &definition.id.0)
        );
        assert_eq!(
            world.allele_ids(&store, PersonId(2), &definition.id),
            world.allele_ids_named(&store, PersonId(2), &definition.id.0)
        );
        assert_eq!(
            source.alleles_named(PersonId(1), &definition.id.0),
            world.alleles(&store, PersonId(1), &definition.id)
        );
    }
    assert_eq!(
        world.alleles_named(&store, PersonId(1), "missing"),
        Err(Error::Unavailable)
    );
    assert_eq!(
        world.alleles_named(&store, PersonId(99), "opaque.alpha"),
        Err(Error::Unavailable)
    );
    assert_eq!(
        source.alleles_named(PersonId(99), "opaque.alpha"),
        Err(Error::Unavailable)
    );
}
#[test]
fn borrowed_lookup_preserves_malformed_genome_and_variant_errors() {
    let original = parents(Rules::default());
    let locus = LocusId("foreign.choice".into());
    let mutations: [fn(&mut (Runtime, Store)); 3] = [
        |state| {
            let (_, store) = state;
            person_genome_mut(store, PersonId(1))
                .unwrap()
                .genome
                .chromosomes
                .pop();
        },
        |state| {
            let (_, store) = state;
            person_genome_mut(store, PersonId(1))
                .unwrap()
                .genome
                .chromosomes[1]
                .homologs[1]
                .variants
                .clear();
        },
        |state| {
            let (world, store) = state;
            let id = world
                .allele_ids_named(store, PersonId(1), "foreign.choice")
                .unwrap()[0];
            world.variants.get_mut(&id).unwrap().locus = LocusId("opaque.alpha".into());
        },
    ];
    for mutate in mutations {
        let mut state = original.clone();
        mutate(&mut state);
        let (world, store) = state;
        let typed = world.alleles(&store, PersonId(1), &locus);
        assert!(matches!(typed, Err(Error::InvalidConfiguration(_))));
        assert_eq!(typed, world.alleles_named(&store, PersonId(1), &locus.0));
    }
}
#[test]
fn invalid_definitions_are_rejected() {
    let original = definitions();
    let mut duplicate = original.clone();
    duplicate.push(duplicate[0].clone());
    assert!(Runtime::new(Rules::default(), duplicate).is_err());
    let mut duplicate_position = original.clone();
    duplicate_position[1].position = duplicate_position[0].position;
    assert!(Runtime::new(Rules::default(), duplicate_position).is_err());
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
        assert!(Runtime::new(Rules::default(), definitions).is_err());
    }
    assert!(
        Runtime::new(
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
    let (mut original_runtime, mut original_store) = parents(Rules {
        mutation_chance: 10_000,
        crossover_chance: 1_000,
    });
    original_runtime
        .conceive(
            &mut original_store,
            8,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    let original = (original_runtime, original_store);
    let mutations: [fn(&mut (Runtime, Store)); 7] = [
        |state| state.0.definitions.reverse(),
        |state| {
            let (_, store) = state;
            person_genome_mut(store, PersonId(1))
                .unwrap()
                .genome
                .chromosomes
                .pop();
        },
        |state| {
            let (_, store) = state;
            inheritance_mut(store, PregnancyId(3))
                .unwrap()
                .transmissions[0][1]
                .crossed_before = false
        },
        |state| {
            let (_, store) = state;
            inheritance_mut(store, PregnancyId(3))
                .unwrap()
                .transmissions[0][0]
                .mutated = false
        },
        |state| {
            let (_, store) = state;
            inheritance_mut(store, PregnancyId(3))
                .unwrap()
                .transmissions[0][0]
                .source_homolog = 2
        },
        |state| {
            let (_, store) = state;
            inheritance_mut(store, PregnancyId(3))
                .unwrap()
                .transmissions[0]
                .pop();
        },
        |state| {
            let (w, store) = state;
            let id =
                inheritance(store, PregnancyId(3)).unwrap().transmissions[0][0].transmitted_variant;
            w.variants.get_mut(&id).unwrap().value = 900;
        },
    ];
    for mutate in mutations {
        let mut state = original.clone();
        mutate(&mut state);
        let (world, store) = state;
        assert!(matches!(
            validate(&world, &store, SimTime(1)),
            Err(Error::InvalidSave(_))
        ));
    }
}
#[test]
fn missing_or_same_parent_conception_has_no_effect() {
    let (mut world, mut store) = parents(Rules::default());
    let before = (world.clone(), store.clone());
    assert_eq!(
        world.conceive(
            &mut store,
            1,
            PregnancyId(3),
            [PersonId(1), PersonId(1)],
            SimTime(1)
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(
        world.conceive(
            &mut store,
            1,
            PregnancyId(3),
            [PersonId(1), PersonId(9)],
            SimTime(1)
        ),
        Err(Error::Unavailable)
    );
    assert_eq!((world.clone(), store.clone()), before);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct AcquiredTrait {
    experience: u64,
}

#[test]
fn registration_and_owner_removal_preserve_foreign_columns_and_entities() {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    assert_eq!(
        schema.keys().collect::<Vec<_>>(),
        vec!["genetics/genome", "genetics/inheritance"]
    );
    assert!(register_components(&mut schema).is_err());
    schema
        .register::<AcquiredTrait>("other/acquired", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema.clone());
    let mut runtime = Runtime::new(Rules::default(), definitions()).unwrap();
    for person in [PersonId(1), PersonId(2)] {
        store
            .insert_or_spawn(
                StableId(person.0),
                AcquiredTrait {
                    experience: 80 + person.0,
                },
            )
            .unwrap();
        runtime.seed_person(&mut store, 3, person).unwrap();
    }
    runtime
        .conceive(
            &mut store,
            4,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    runtime
        .born(&mut store, PregnancyId(3), PersonId(4), SimTime(2))
        .unwrap();
    assert_eq!(
        store.get::<AcquiredTrait>(StableId(1)),
        Some(&AcquiredTrait { experience: 81 })
    );
    assert_eq!(
        store.get::<AcquiredTrait>(StableId(2)),
        Some(&AcquiredTrait { experience: 82 })
    );
    assert_eq!(store.get::<AcquiredTrait>(StableId(4)), None);
    validate(&runtime, &store, SimTime(2)).unwrap();
    let restored = Store::restore(
        schema,
        serde_json::from_slice(&serde_json::to_vec(&store.snapshot().unwrap()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(restored, store);
    assert!(
        remove_person_genome(&mut store, PersonId(1))
            .unwrap()
            .is_some()
    );
    assert!(store.contains(StableId(1)));
    assert_eq!(
        store.get::<AcquiredTrait>(StableId(1)),
        Some(&AcquiredTrait { experience: 81 })
    );
    assert!(
        remove_person_genome(&mut store, PersonId(99))
            .unwrap()
            .is_none()
    );
    assert!(
        remove_inheritance(&mut store, PregnancyId(3))
            .unwrap()
            .is_some()
    );
    assert!(store.contains(StableId(3)));
    assert!(
        remove_inheritance(&mut store, PregnancyId(99))
            .unwrap()
            .is_none()
    );
}

#[test]
fn invalid_component_insertions_and_unknown_old_maps_are_rejected() {
    let mut store = store();
    let before = store.clone();
    let malformed = PersonGenome {
        genome: Genome {
            chromosomes: vec![ChromosomePair {
                homologs: [
                    Chromosome {
                        variants: vec![VariantId(0)],
                    },
                    Chromosome {
                        variants: vec![VariantId(1)],
                    },
                ],
            }],
        },
    };
    assert!(insert_person_genome(&mut store, PersonId(1), malformed).is_err());
    let malformed = Inheritance {
        parents: [PersonId(1), PersonId(1)],
        conceived_at: SimTime(0),
        genome: Some(Genome {
            chromosomes: vec![],
        }),
        transmissions: [vec![], vec![]],
        born: None,
        ended_at: None,
    };
    assert!(insert_inheritance(&mut store, PregnancyId(2), malformed).is_err());
    assert_eq!(store, before);
    let runtime = Runtime::new(Rules::default(), vec![]).unwrap();
    let mut json = serde_json::to_value(runtime).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("people".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<Runtime>(json).is_err());
}

#[test]
fn conception_schema_failure_does_not_publish_pending_mutations() {
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<PersonGenome>("genetics/genome", PersonGenome::validate)
        .unwrap();
    let mut store = Store::with_schema(schema);
    let mut runtime = Runtime::new(
        Rules {
            mutation_chance: 10_000,
            crossover_chance: 1_000,
        },
        definitions(),
    )
    .unwrap();
    runtime.seed_person(&mut store, 3, PersonId(1)).unwrap();
    runtime.seed_person(&mut store, 3, PersonId(2)).unwrap();
    let before = (runtime.clone(), store.clone());
    assert!(
        runtime
            .conceive(
                &mut store,
                8,
                PregnancyId(3),
                [PersonId(1), PersonId(2)],
                SimTime(1)
            )
            .is_err()
    );
    assert_eq!((runtime, store), before);
}

#[test]
fn founder_schema_failure_does_not_publish_variants() {
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<PersonGenome>("genetics/genome", |record| {
            record.validate()?;
            let forbidden = standing_variant(&LocusId("opaque.alpha".into()), 20);
            if record
                .genome
                .chromosomes
                .iter()
                .flat_map(|pair| &pair.homologs)
                .any(|homolog| homolog.variants.contains(&forbidden))
            {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema
        .register::<Inheritance>("genetics/inheritance", Inheritance::validate)
        .unwrap();
    let mut store = Store::with_schema(schema);
    let mut runtime = Runtime::new(Rules::default(), definitions()).unwrap();
    runtime.seed_person(&mut store, 3, PersonId(1)).unwrap();
    let before = (runtime.clone(), store.clone());
    assert_eq!(
        runtime.set_alleles(
            &mut store,
            PersonId(1),
            &LocusId("opaque.alpha".into()),
            [20, 21]
        ),
        Err(Error::Unavailable)
    );
    assert_eq!((runtime, store), before);
}

#[test]
fn birth_schema_failures_never_attach_a_child_or_mark_the_pregnancy_born() {
    for reject_child in [false, true] {
        let mut schema = game_ecs::Schema::new();
        if reject_child {
            schema
                .register::<PersonGenome>("genetics/genome", |record| {
                    record.validate()?;
                    if record
                        .genome
                        .chromosomes
                        .iter()
                        .flat_map(|pair| &pair.homologs)
                        .flat_map(|homolog| &homolog.variants)
                        .any(|id| id.0 & (1 << 63) != 0)
                    {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
            schema
                .register::<Inheritance>("genetics/inheritance", Inheritance::validate)
                .unwrap();
        } else {
            schema
                .register::<PersonGenome>("genetics/genome", PersonGenome::validate)
                .unwrap();
            schema
                .register::<Inheritance>("genetics/inheritance", |record| {
                    record.validate()?;
                    if record.born.is_some() {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        }
        let mut store = Store::with_schema(schema);
        let mut runtime = Runtime::new(
            Rules {
                mutation_chance: 10_000,
                crossover_chance: 1_000,
            },
            definitions(),
        )
        .unwrap();
        runtime.seed_person(&mut store, 3, PersonId(1)).unwrap();
        runtime.seed_person(&mut store, 3, PersonId(2)).unwrap();
        runtime
            .conceive(
                &mut store,
                8,
                PregnancyId(3),
                [PersonId(1), PersonId(2)],
                SimTime(1),
            )
            .unwrap();
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            runtime.born(&mut store, PregnancyId(3), PersonId(4), SimTime(2)),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn store_restore_accepts_intrinsic_trace_shape_but_context_rejects_frozen_trace_change() {
    let (mut runtime, mut store) = parents(Rules {
        mutation_chance: 0,
        crossover_chance: 1_000,
    });
    runtime
        .conceive(
            &mut store,
            8,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    let mut changed = inheritance(&store, PregnancyId(3)).unwrap().clone();
    changed.transmissions[0][1].crossed_before = false;
    insert_inheritance(&mut store, PregnancyId(3), changed).unwrap();
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert!(matches!(
        validate(&runtime, &restored, SimTime(1)),
        Err(Error::InvalidSave(_))
    ));
}

#[test]
fn pure_founder_rows_and_readers_preserve_keyed_seed_and_assignment_results() {
    let mut runtime = Runtime::new(Rules::default(), definitions()).unwrap();
    let original = runtime.clone();
    let assignments = [
        (LocusId("opaque.alpha".into()), [20, 21]),
        (LocusId("foreign.choice".into()), [-13, 14]),
    ];
    let prepared = runtime
        .prepare_founder(7, PersonId(1), &assignments)
        .unwrap();
    assert_eq!(runtime, original);
    let mut expected_runtime = original;
    let mut expected_store = store();
    expected_runtime
        .seed_person(&mut expected_store, 7, PersonId(1))
        .unwrap();
    for (locus, values) in &assignments {
        expected_runtime
            .set_alleles(&mut expected_store, PersonId(1), locus, *values)
            .unwrap();
    }
    assert_eq!(
        person_genome(&expected_store, PersonId(1)),
        Some(&prepared.genome)
    );
    runtime.apply_founder_variants(&prepared).unwrap();
    assert_eq!(runtime, expected_runtime);
    let source = runtime.genome_source(PersonId(1), &prepared.genome);
    for definition in definitions() {
        assert_eq!(
            source.alleles_named(PersonId(1), &definition.id.0),
            runtime.alleles_named(&expected_store, PersonId(1), &definition.id.0)
        );
    }
    assert_eq!(
        source.alleles_named(PersonId(99), "opaque.alpha"),
        Err(Error::Unavailable)
    );
    assert_eq!(
        source.alleles_named(PersonId(1), "missing"),
        Err(Error::Unavailable)
    );
    let mut host = store();
    assert!(all_person_genomes(&host).next().is_none());
    let mut batch = Batch::new();
    batch.insert(StableId(1), prepared.genome);
    host.apply_batch(batch).unwrap();
    assert_eq!(host, expected_store);
    validate(&runtime, &host, SimTime(0)).unwrap();
}

#[test]
fn a_late_invalid_founder_assignment_preserves_runtime_and_host_rows() {
    let (runtime, host) = parents(Rules::default());
    let before = (runtime.clone(), host.clone());
    assert!(
        runtime
            .prepare_founder(
                7,
                PersonId(4),
                &[
                    (LocusId("opaque.alpha".into()), [20, 21]),
                    (LocusId("foreign.choice".into()), [14, 501]),
                ]
            )
            .is_err()
    );
    assert_eq!((runtime.clone(), host.clone()), before);
    assert!(runtime.prepare_founder(7, PersonId(0), &[]).is_err());
    let prepared = runtime.prepare_founder(7, PersonId(4), &[]).unwrap();
    let mut malformed = prepared.genome;
    malformed.genome.chromosomes[1].homologs[0].variants.clear();
    assert!(matches!(
        runtime
            .genome_source(PersonId(4), &malformed)
            .alleles_named(PersonId(4), "foreign.choice"),
        Err(Error::InvalidConfiguration(_))
    ));
    assert_eq!((runtime, host), before);
}

#[test]
fn forged_founder_deltas_are_rejected_before_any_registry_write() {
    let runtime = Runtime::new(Rules::default(), definitions()).unwrap();
    let prepared = runtime
        .prepare_founder(
            7,
            PersonId(1),
            &[
                (LocusId("opaque.alpha".into()), [20, 21]),
                (LocusId("foreign.choice".into()), [-13, 14]),
            ],
        )
        .unwrap();
    let mutations: [fn(&mut PreparedFounder); 5] = [
        |value| value.variants.values_mut().next_back().unwrap().locus = LocusId("missing".into()),
        |value| value.variants.values_mut().next_back().unwrap().value = 501,
        |value| {
            let (id, variant) = value.variants.pop_last().unwrap();
            value.variants.insert(VariantId(id.0 | (1 << 63)), variant);
        },
        |value| value.genome.genome.chromosomes[0].homologs[0].variants[0] = VariantId(0),
        |value| value.genome.genome.chromosomes[0].homologs[0].variants[0] = VariantId(u64::MAX),
    ];
    for mutate in mutations {
        let mut runtime = runtime.clone();
        let before = runtime.clone();
        let mut forged = prepared.clone();
        mutate(&mut forged);
        assert!(runtime.apply_founder_variants(&forged).is_err());
        assert_eq!(runtime, before);
    }
    let mut conflicting = runtime;
    let (id, variant) = prepared.variants.last_key_value().unwrap();
    let mut variant = variant.clone();
    variant.value += 1;
    conflicting.variants.insert(*id, variant);
    let before = conflicting.clone();
    assert!(conflicting.apply_founder_variants(&prepared).is_err());
    assert_eq!(conflicting, before);
}

#[test]
fn birth_requires_distinct_child_and_pregnancy_entities_before_any_write() {
    let (mut runtime, mut store) = parents(Rules::default());
    runtime
        .conceive(
            &mut store,
            8,
            PregnancyId(3),
            [PersonId(1), PersonId(2)],
            SimTime(1),
        )
        .unwrap();
    let before = (runtime.clone(), store.clone());
    assert_eq!(
        runtime.born(&mut store, PregnancyId(3), PersonId(3), SimTime(2)),
        Err(Error::Unavailable)
    );
    assert_eq!((runtime.clone(), store.clone()), before);

    let genome = inheritance(&store, PregnancyId(3))
        .unwrap()
        .genome
        .clone()
        .unwrap();
    insert_person_genome(&mut store, PersonId(3), PersonGenome { genome }).unwrap();
    let record = inheritance_mut(&mut store, PregnancyId(3)).unwrap();
    record.genome = None;
    record.born = Some(PersonId(3));
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert!(matches!(
        validate(&runtime, &restored, SimTime(2)),
        Err(Error::InvalidSave(_))
    ));
}

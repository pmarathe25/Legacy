use super::*;

fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_land::register_components(&mut schema).unwrap();
    schema
}
fn runtime() -> Runtime {
    Runtime::new(EcologyRules::default(), 0).unwrap()
}
fn deposit(id: u64, area: u64) -> Resource {
    Resource {
        id: ResourceId(id),
        area: AreaId(area),
        kind: ResourceKind::Berries,
        remaining: Quantity(5_000),
        capacity: Quantity(20_000),
        daily_growth: Quantity(250),
        growth_remainder: 0,
        decay_permille: 0,
        soil_water: 75,
        water_capacity: 120,
        optimal_water: 50,
        minimum_temperature: 0,
        optimum_temperature: 20,
        contamination: 10,
        runoff_contamination: 0,
        labor_per_unit: MINUTE,
        workers: 2,
        access: BTreeSet::from([PersonId(1)]),
        custodian: Some(PersonId(1)),
    }
}
fn setup() -> (Runtime, Store, Vec<ObserverArea>) {
    let runtime = runtime();
    let mut store = Store::with_schema(schema());
    for id in 1..=3 {
        store.ensure(StableId(id)).unwrap();
    }
    for (id, area) in [(100, 1), (101, 2), (102, 3)] {
        runtime
            .insert_resource(&mut store, deposit(id, area))
            .unwrap();
        runtime
            .insert_plot_soil(
                &mut store,
                Plot {
                    id: SiteId(id + 100),
                    area: AreaId(area),
                    access: BTreeSet::from([PersonId(2)]),
                    custodian: Some(PersonId(2)),
                },
                Soil {
                    fertility: 800,
                    water: 75,
                    capacity: 120,
                },
            )
            .unwrap();
    }
    let facts = vec![
        ObserverArea {
            actor: PersonId(1),
            area: AreaId(1),
        },
        ObserverArea {
            actor: PersonId(2),
            area: AreaId(1),
        },
        ObserverArea {
            actor: PersonId(3),
            area: AreaId(2),
        },
    ];
    (runtime, store, facts)
}
// Independent copy of the old assembly loop, including its unusual behavior:
// a newer area timestamp is retained, but local resource/plot views still refresh.
fn oracle(
    runtime: &Runtime,
    store: &Store,
    now: SimTime,
    facts: &[ObserverArea],
) -> BTreeMap<PersonId, Knowledge> {
    let mut out: BTreeMap<_, _> = runtime
        .all_knowledge(store)
        .map(|(id, row)| (id, row.clone()))
        .collect();
    for facts in facts {
        let row = out.entry(facts.actor).or_default();
        let future = row
            .observed_areas
            .get(&facts.area)
            .is_some_and(|at| *at > now);
        if !future {
            if row.observed_areas.len() == 16 && !row.observed_areas.contains_key(&facts.area) {
                let oldest = row
                    .observed_areas
                    .iter()
                    .min_by_key(|(id, at)| (**at, **id))
                    .map(|(id, _)| *id)
                    .unwrap();
                row.observed_areas.remove(&oldest);
            }
            row.observed_areas.insert(facts.area, now);
        }
        for (_, source) in runtime
            .resources(store)
            .filter(|(_, source)| source.area == facts.area)
        {
            row.known_resources.insert(
                source.id,
                ResourceView {
                    id: source.id,
                    area: facts.area,
                    kind: source.kind,
                    available: source.remaining,
                    accessible: source.access.contains(&facts.actor),
                    custodian: source.custodian,
                    contamination: source.contamination,
                    observed_at: now,
                },
            );
        }
        for (_, plot) in runtime
            .plots(store)
            .filter(|(_, plot)| plot.area == facts.area)
        {
            let soil = runtime.soil(store, plot.id).unwrap();
            row.known_plots.insert(
                plot.id,
                PlotView {
                    id: plot.id,
                    area: facts.area,
                    soil_fertility: soil.fertility,
                    soil_water: soil.water,
                    accessible: plot.access.contains(&facts.actor),
                    custodian: plot.custodian,
                    observed_at: now,
                },
            );
        }
    }
    out
}
#[test]
fn local_observations_match_old_loop_under_reordering_and_save_restore() {
    let (runtime, mut store, mut facts) = setup();
    let expected = oracle(&runtime, &store, SimTime(100), &facts);
    let opening = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_local_observations(&store, SimTime(100), &facts)
        .unwrap();
    assert_eq!(prepared.observers(), 3);
    assert_eq!(store.snapshot().unwrap(), opening);
    let json = serde_json::to_string(&opening).unwrap();
    let mut restored = Store::restore(schema(), serde_json::from_str(&json).unwrap()).unwrap();
    facts.reverse();
    let second = runtime
        .prepare_local_observations(&restored, SimTime(100), &facts)
        .unwrap();
    prepared.commit(&runtime, &mut store).unwrap();
    second.commit(&runtime, &mut restored).unwrap();
    assert_eq!(store.snapshot().unwrap(), restored.snapshot().unwrap());
    assert_eq!(
        runtime
            .all_knowledge(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    let first = runtime.knowledge(&store, PersonId(1)).unwrap();
    let second = runtime.knowledge(&store, PersonId(2)).unwrap();
    assert!(first.known_resources[&ResourceId(100)].accessible);
    assert!(!second.known_resources[&ResourceId(100)].accessible);
    assert!(!first.known_plots[&SiteId(200)].accessible);
    assert!(second.known_plots[&SiteId(200)].accessible);
    assert!(!first.known_resources.contains_key(&ResourceId(101)));
    assert!(
        runtime
            .knowledge(&store, PersonId(3))
            .unwrap()
            .known_resources
            .contains_key(&ResourceId(101))
    );
}
#[test]
fn observation_retains_missing_remote_memories_and_exact_clock_eviction_policy() {
    let (runtime, mut store, mut facts) = setup();
    let mut remembered = oracle(
        &runtime,
        &store,
        SimTime(50),
        &[ObserverArea {
            actor: PersonId(1),
            area: AreaId(3),
        }],
    )
    .remove(&PersonId(1))
    .unwrap();
    remembered.observed_areas.insert(AreaId(1), SimTime(500));
    runtime
        .insert_knowledge(&mut store, PersonId(1), remembered)
        .unwrap();
    runtime
        .remove_resource(&mut store, ResourceId(102))
        .unwrap();
    runtime.remove_plot(&mut store, SiteId(202)).unwrap();
    let mut capped = Knowledge::default();
    for id in 3..=18 {
        capped
            .observed_areas
            .insert(AreaId(id), SimTime(if id <= 4 { 0 } else { id }));
    }
    runtime
        .insert_knowledge(&mut store, PersonId(3), capped)
        .unwrap();
    let expected = oracle(&runtime, &store, SimTime(100), &facts);
    runtime
        .prepare_local_observations(&store, SimTime(100), &facts)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime
            .all_knowledge(&store)
            .map(|(id, row)| (id, row.clone()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    let row = runtime.knowledge(&store, PersonId(1)).unwrap();
    assert_eq!(row.observed_areas[&AreaId(1)], SimTime(500));
    assert_eq!(
        row.known_resources[&ResourceId(100)].observed_at,
        SimTime(100)
    );
    assert_eq!(
        row.known_resources[&ResourceId(102)].observed_at,
        SimTime(50)
    );
    assert!(row.known_plots.contains_key(&SiteId(202)));
    let row = runtime.knowledge(&store, PersonId(3)).unwrap();
    assert_eq!(row.observed_areas.len(), 16);
    assert!(!row.observed_areas.contains_key(&AreaId(3)));
    assert!(row.observed_areas.contains_key(&AreaId(4)));
    // Existing timestamps refresh without eviction, including observations in an empty area.
    facts[2].area = AreaId(18);
    runtime
        .prepare_local_observations(&store, SimTime(101), &facts)
        .unwrap()
        .commit(&runtime, &mut store)
        .unwrap();
    assert_eq!(
        runtime
            .knowledge(&store, PersonId(3))
            .unwrap()
            .observed_areas[&AreaId(18)],
        SimTime(101)
    );
}
#[test]
fn changed_relevant_rows_and_membership_reject_all_observers_before_publication() {
    for change in 0..7 {
        let (runtime, mut store, facts) = setup();
        let prepared = runtime
            .prepare_local_observations(&store, SimTime(100), &facts)
            .unwrap();
        match change {
            0 => {
                runtime
                    .resource_mut(&mut store, ResourceId(100))
                    .unwrap()
                    .remaining = Quantity(1_000)
            }
            1 => {
                runtime
                    .resource_mut(&mut store, ResourceId(100))
                    .unwrap()
                    .access
                    .insert(PersonId(2));
            }
            2 => {
                runtime.remove_plot(&mut store, SiteId(200)).unwrap();
            }
            3 => runtime.soil_mut(&mut store, SiteId(200)).unwrap().water = 1,
            4 => {
                runtime
                    .insert_resource(&mut store, deposit(103, 1))
                    .unwrap();
            }
            5 => {
                runtime
                    .resource_mut(&mut store, ResourceId(102))
                    .unwrap()
                    .area = AreaId(1)
            }
            _ => {
                runtime
                    .insert_knowledge(
                        &mut store,
                        PersonId(2),
                        Knowledge {
                            observed_areas: BTreeMap::from([(AreaId(9), SimTime(99))]),
                            ..Knowledge::default()
                        },
                    )
                    .unwrap();
            }
        }
        let before = store.snapshot().unwrap();
        assert!(prepared.commit(&runtime, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert!(runtime.knowledge(&store, PersonId(1)).is_none());
    }
    let (runtime, mut store, facts) = setup();
    let prepared = runtime
        .prepare_local_observations(&store, SimTime(100), &facts)
        .unwrap();
    runtime
        .resource_mut(&mut store, ResourceId(102))
        .unwrap()
        .remaining = Quantity(1_000);
    runtime.soil_mut(&mut store, SiteId(202)).unwrap().water = 1;
    prepared.commit(&runtime, &mut store).unwrap(); // Unobserved land is not a global guard.
}
#[test]
fn invalid_observers_and_missing_local_soil_do_not_publish() {
    let (runtime, mut store, facts) = setup();
    for invalid in [
        vec![facts[0], facts[0]],
        vec![
            facts[0],
            ObserverArea {
                actor: facts[0].actor,
                area: AreaId(2),
            },
        ],
        vec![ObserverArea {
            actor: PersonId(99),
            area: AreaId(1),
        }],
        vec![ObserverArea {
            actor: PersonId(1),
            area: AreaId(0),
        }],
    ] {
        let before = store.snapshot().unwrap();
        assert!(
            runtime
                .prepare_local_observations(&store, SimTime(100), &invalid)
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    game_land::remove_soil(&mut store, SiteId(200)).unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .prepare_local_observations(&store, SimTime(100), &facts)
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn strict_component_validation_rejects_the_whole_observation_batch() {
    let runtime = runtime();
    let mut strict = game_ecs::Schema::new();
    strict
        .register::<Resource>("ecology/resource", Resource::validate)
        .unwrap();
    strict
        .register::<Plot>("ecology/plot", Plot::validate)
        .unwrap();
    game_land::register_components(&mut strict).unwrap();
    strict
        .register::<Knowledge>("ecology/knowledge", |row| {
            row.validate()?;
            if row.known_resources.values().any(|view| !view.accessible) {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(strict);
    store.ensure(StableId(1)).unwrap();
    store.ensure(StableId(2)).unwrap();
    runtime
        .insert_resource(&mut store, deposit(100, 1))
        .unwrap();
    let facts = [
        ObserverArea {
            actor: PersonId(1),
            area: AreaId(1),
        },
        ObserverArea {
            actor: PersonId(2),
            area: AreaId(1),
        },
    ];
    let prepared = runtime
        .prepare_local_observations(&store, SimTime(100), &facts)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(prepared.commit(&runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

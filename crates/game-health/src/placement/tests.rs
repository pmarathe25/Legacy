use super::*;
fn fixture() -> (Runtime, Store, BTreeMap<PersonId, ShelterActorFacts>) {
    let runtime = Runtime::default();
    let mut schema = game_ecs::Schema::new();
    crate::register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/foreign", |value| {
            if value == "reject" {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    crate::insert_shelter(
        &mut store,
        Shelter {
            id: ShelterId(20),
            area: AreaId(10),
            custodian: PersonId(1),
            access: BTreeSet::from([PersonId(1)]),
            capacity: 1,
            insulation: 100,
            built_at: SimTime(0),
            timber_used: Quantity(1),
            labor_ms: 1,
        },
    )
    .unwrap();
    crate::insert_occupancy(
        &mut store,
        PersonId(1),
        Occupancy {
            shelter: ShelterId(20),
        },
    )
    .unwrap();
    store.spawn(StableId(2)).unwrap();
    let facts = [1, 2]
        .into_iter()
        .map(|actor| {
            (
                PersonId(actor),
                ShelterActorFacts {
                    actor: PersonId(actor),
                    carrier: (actor == 2).then_some(PersonId(1)),
                    alive: true,
                    physical_area: Some(AreaId(10)),
                    activity: HealthActivityFacts::default(),
                },
            )
        })
        .collect();
    (runtime, store, facts)
}
fn target(area: Option<AreaId>) -> ShelterPlacement {
    ShelterPlacement {
        actor: PersonId(2),
        final_area: area,
    }
}
#[test]
fn alive_placement_selects_real_effective_shelter_and_only_inherits_at_its_final_area() {
    for (alive, area, expected) in [
        (true, Some(AreaId(10)), true),
        (false, Some(AreaId(10)), false),
        (true, Some(AreaId(11)), false),
        (true, None, false),
    ] {
        let (mut runtime, mut store, mut facts) = fixture();
        facts.get_mut(&PersonId(2)).unwrap().alive = alive;
        runtime
            .prepare_placements(&store, &[], &[target(area)], &facts)
            .unwrap()
            .commit(&mut runtime, &mut store, &facts)
            .unwrap();
        assert_eq!(occupancy(&store, PersonId(2)).is_some(), expected);
        if expected {
            assert_eq!(
                occupancy(&store, PersonId(2)).unwrap().shelter,
                ShelterId(20)
            );
        }
    }
}
#[test]
fn leaving_and_placement_coalesce_rows_and_keep_first_departure_event_order() {
    let (mut runtime, mut store, facts) = fixture();
    crate::insert_occupancy(
        &mut store,
        PersonId(2),
        Occupancy {
            shelter: ShelterId(20),
        },
    )
    .unwrap();
    let effects = runtime
        .prepare_placements(
            &store,
            &[PersonId(2), PersonId(1), PersonId(2)],
            &[target(Some(AreaId(10)))],
            &facts,
        )
        .unwrap()
        .commit(&mut runtime, &mut store, &facts)
        .unwrap();
    assert_eq!(
        effects,
        vec![
            HealthEffect::ShelterLeft {
                shelter: ShelterId(20),
                actor: PersonId(2)
            },
            HealthEffect::ShelterLeft {
                shelter: ShelterId(20),
                actor: PersonId(1)
            }
        ]
    );
    assert!(occupancy(&store, PersonId(1)).is_none());
    assert!(occupancy(&store, PersonId(2)).is_some());
}
#[test]
fn actual_shelter_carrier_occupancy_activity_and_alive_chain_changes_reject_before_write() {
    for failure in 0..5 {
        let (mut runtime, mut store, mut facts) = fixture();
        let plan = runtime
            .prepare_placements(&store, &[], &[target(Some(AreaId(10)))], &facts)
            .unwrap();
        match failure {
            0 => crate::shelter_mut(&mut store, ShelterId(20)).unwrap().area = AreaId(11),
            1 => {
                crate::remove_occupancy(&mut store, PersonId(1)).unwrap();
            }
            2 => facts.get_mut(&PersonId(2)).unwrap().carrier = None,
            3 => facts.get_mut(&PersonId(1)).unwrap().activity.combat = true,
            _ => facts.get_mut(&PersonId(1)).unwrap().alive = false,
        }
        let before = store.snapshot().unwrap();
        assert!(plan.commit(&mut runtime, &mut store, &facts).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
}
#[test]
fn foreign_rejection_and_same_batch_selected_carrier_removal_preserve_all_owner_state() {
    for carrier in [false, true] {
        let (runtime, mut store, facts) = fixture();
        let plan = runtime
            .prepare_placements(&store, &[], &[target(Some(AreaId(10)))], &facts)
            .unwrap();
        let before = store.snapshot().unwrap();
        let header = runtime.clone();
        let mut batch = Batch::new();
        let _finish = plan
            .stage_into(&runtime, &store, &facts, &mut batch)
            .unwrap();
        if carrier {
            batch.remove::<Occupancy>(StableId(1));
        } else {
            batch.insert(StableId(2), "reject".to_owned());
        }
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(runtime, header);
    }
}
#[test]
fn projected_placement_and_observation_are_prepared_from_final_occupancy_and_admitted_once() {
    for reject in [false, true] {
        let (runtime, mut store, facts) = fixture();
        crate::insert_clinical(&mut store, PersonId(2), Clinical::default()).unwrap();
        let placement = runtime
            .prepare_placements(&store, &[], &[target(Some(AreaId(10)))], &facts)
            .unwrap();
        let actors = [PersonId(1), PersonId(2)].map(|actor| LocalHealthActor {
            actor,
            place: Some(AreaId(10)),
            physical_area: Some(AreaId(10)),
            carrier: None,
            tactical: None,
            alive: true,
            moving: false,
            activity: HealthActivityFacts::default(),
            health: 100,
            max_health: 100,
            temperature_c: Some(20),
        });
        let observation = placement
            .prepare_projected_observations(&runtime, &store, &facts, SimTime(10), &actors, &[])
            .unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let finish = placement
            .stage_into(&runtime, &store, &facts, &mut batch)
            .unwrap();
        observation
            .stage_into(&runtime, &store, &mut batch)
            .unwrap();
        if reject {
            batch.insert(StableId(2), "reject".to_owned());
        }
        let result = store.apply_batch(batch);
        if reject {
            assert!(result.is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            result.unwrap();
            assert!(finish.finish().is_empty());
            assert!(occupancy(&store, PersonId(2)).is_some());
            assert!(
                knowledge(&store, PersonId(1))
                    .unwrap()
                    .people
                    .contains_key(&PersonId(2))
            );
            assert!(
                knowledge(&store, PersonId(1)).unwrap().shelters[&ShelterId(20)]
                    .known_occupants
                    .contains(&PersonId(2))
            );
        }
    }
}
#[test]
fn two_disjoint_placements_share_one_selected_shelter_chain_and_sparse_owner_batch() {
    let (mut runtime, mut store, mut facts) = fixture();
    store.spawn(StableId(3)).unwrap();
    facts.insert(
        PersonId(3),
        ShelterActorFacts {
            actor: PersonId(3),
            carrier: Some(PersonId(1)),
            alive: true,
            physical_area: Some(AreaId(10)),
            activity: HealthActivityFacts::default(),
        },
    );
    let placements = [
        target(Some(AreaId(10))),
        ShelterPlacement {
            actor: PersonId(3),
            final_area: Some(AreaId(10)),
        },
    ];
    runtime
        .prepare_placements(&store, &[], &placements, &facts)
        .unwrap()
        .commit(&mut runtime, &mut store, &facts)
        .unwrap();
    for actor in [1, 2, 3] {
        assert_eq!(
            occupancy(&store, PersonId(actor)),
            Some(&Occupancy {
                shelter: ShelterId(20)
            })
        );
    }
}

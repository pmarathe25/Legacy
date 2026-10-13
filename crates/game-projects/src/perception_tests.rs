use super::*;
fn fixture(strict_memory: bool) -> (Runtime, Store, game_workshop::Definitions) {
    let mut schema = game_ecs::Schema::new();
    schema
        .register::<Project>("projects/project", Project::validate)
        .unwrap();
    schema
        .register::<Enterprise>("projects/enterprise", Enterprise::validate)
        .unwrap();
    schema
        .register::<Construction>("projects/construction", Construction::validate)
        .unwrap();
    schema
        .register::<Requests>("projects/requests", |_| Ok(()))
        .unwrap();
    if strict_memory {
        schema
            .register::<Knowledge>("projects/knowledge", |row| {
                row.validate()?;
                if !row.projects.is_empty() {
                    return Err(Error::InvalidConfiguration(
                        "project receiver rejected".into(),
                    ));
                }
                Ok(())
            })
            .unwrap();
    } else {
        schema
            .register::<Knowledge>("projects/knowledge", Knowledge::validate)
            .unwrap();
    }
    game_cargo::register_components(&mut schema).unwrap();
    insert_fixture(Store::with_schema(schema))
}
fn insert_fixture(mut store: Store) -> (Runtime, Store, game_workshop::Definitions) {
    insert_knowledge(&mut store, PersonId(3), Knowledge::default()).unwrap();
    game_cargo::insert_inventory(
        &mut store,
        game_cargo::Inventory {
            id: InventoryId(4),
            good: Good::Timber,
            quantity: Quantity(5_000),
            capacity: Quantity(10_000),
            location: game_cargo::InventoryLocation::Area(AreaId(2)),
            access: BTreeSet::from([PersonId(3)]),
            last_granted: None,
            spoils_at: None,
        },
    )
    .unwrap();
    let plan = Project {
        kind: ProjectKind::Workshop {
            recipe: game_workshop::RecipeId("tools".into()),
        },
        id: ProjectId(11),
        sponsor: PersonId(3),
        area: AreaId(2),
        materials: InventoryId(4),
        tool: InventoryId(5),
        input: InventoryId(6),
        output: InventoryId(7),
        beneficiaries: BTreeSet::from([PersonId(3)]),
        workers: BTreeSet::from([PersonId(3)]),
        support: BTreeSet::new(),
        status: ProjectStatus::Ready,
        blocker: None,
        progress_ms: 0,
        contributions: BTreeMap::new(),
        materials_consumed: false,
        finish_by: SimTime(600_000),
        enterprise: None,
    };
    insert_project(&mut store, plan.id, plan).unwrap();
    (
        Runtime::default(),
        store,
        crate::transition_tests::definitions(),
    )
}
fn construction_facts() -> ConstructionFacts {
    ConstructionFacts {
        actor: ActorFacts {
            actor: PersonId(3),
            now: SimTime(0),
            area: Some(AreaId(2)),
            capable: true,
            knows_project: true,
        },
        land_accessible: true,
        tool_available: true,
        materials_available: Quantity(5_000),
    }
}
fn facts() -> ObservationFacts<'static> {
    static OBSERVERS: [Observer; 1] = [Observer {
        actor: PersonId(3),
        area: Some(AreaId(2)),
    }];
    ObservationFacts {
        now: SimTime(0),
        observers: &OBSERVERS,
        accounts_available: false,
    }
}
#[test]
fn project_action_and_projected_memory_publish_once_in_the_same_native_batch() {
    let (mut runtime, mut store, definitions) = fixture(false);
    let change = runtime
        .prepare_construction(&store, ProjectId(11), construction_facts())
        .unwrap();
    let mut batch = Batch::new();
    let materials = game_cargo::prepare_consumptions(&store, &[(InventoryId(4), Quantity(5_000))])
        .unwrap()
        .stage_guarded_into(&store, &mut batch)
        .unwrap();
    let origin = change
        .stage_into(&runtime, &store, &definitions, &mut batch)
        .unwrap();
    let receiver = runtime
        .prepare_projected_observation(&store, &batch, &origin, facts())
        .unwrap()
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    assert!(knowledge(&store, PersonId(3)).unwrap().projects.is_empty());
    origin
        .validate_batch(&runtime, &store, &definitions, &batch)
        .unwrap();
    receiver.validate(&runtime, &store, &batch).unwrap();
    materials.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    origin.finish(&mut runtime);
    assert_eq!(
        project(&store, ProjectId(11)).unwrap().status,
        ProjectStatus::Active
    );
    assert_eq!(
        knowledge(&store, PersonId(3)).unwrap().projects[&ProjectId(11)].status,
        ProjectStatus::Active
    );
    assert!(construction(&store, PersonId(3)).is_some());
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(4))
            .unwrap()
            .quantity,
        Quantity(0)
    );
}
#[test]
fn strict_memory_schema_rejection_cannot_publish_construction_or_project_progress() {
    let (runtime, mut store, definitions) = fixture(true);
    let opening = store.snapshot().unwrap();
    let change = runtime
        .prepare_construction(&store, ProjectId(11), construction_facts())
        .unwrap();
    let mut batch = Batch::new();
    let materials = game_cargo::prepare_consumptions(&store, &[(InventoryId(4), Quantity(5_000))])
        .unwrap()
        .stage_guarded_into(&store, &mut batch)
        .unwrap();
    let origin = change
        .stage_into(&runtime, &store, &definitions, &mut batch)
        .unwrap();
    let receiver = runtime
        .prepare_projected_observation(&store, &batch, &origin, facts())
        .unwrap()
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    origin
        .validate_batch(&runtime, &store, &definitions, &batch)
        .unwrap();
    receiver.validate(&runtime, &store, &batch).unwrap();
    materials.validate(&store, &batch).unwrap();
    assert_eq!(
        store.apply_batch(batch),
        Err(Error::InvalidConfiguration(
            "project receiver rejected".into()
        ))
    );
    assert_eq!(store.snapshot().unwrap(), opening);
}
#[test]
fn stale_personal_memory_and_late_source_edits_reject_before_composed_publication() {
    let (runtime, mut store, definitions) = fixture(false);
    let change = runtime
        .prepare_construction(&store, ProjectId(11), construction_facts())
        .unwrap();
    let mut batch = Batch::new();
    let origin = change
        .stage_into(&runtime, &store, &definitions, &mut batch)
        .unwrap();
    let observation = runtime
        .prepare_projected_observation(&store, &batch, &origin, facts())
        .unwrap();
    knowledge_mut(&mut store, PersonId(3))
        .unwrap()
        .timber_hints
        .insert(AreaId(4));
    let changed = store.snapshot().unwrap();
    assert!(
        observation
            .stage_into(&runtime, &store, &mut batch)
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), changed);
    let observation = runtime
        .prepare_projected_observation(&store, &batch, &origin, facts())
        .unwrap();
    let receiver = observation
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    let mut foreign = project(&store, ProjectId(11)).unwrap().clone();
    foreign.status = ProjectStatus::Cancelled;
    batch.insert(StableId(11), foreign);
    assert_eq!(
        receiver.validate(&runtime, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), changed);
}

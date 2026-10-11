use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Foreign(u64);
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Foreign>("test/foreign", |row| {
            if row.0 == 99 {
                Err(Error::InvalidConfiguration("foreign rejection".into()))
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema
}
fn plan(id: u64) -> Project {
    Project {
        kind: ProjectKind::Workshop {
            recipe: game_workshop::RecipeId("tools".into()),
        },
        id: ProjectId(id),
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
        finish_by: SimTime(500),
        enterprise: None,
    }
}
fn fixture() -> (Runtime, Store, game_workshop::Definitions) {
    let mut runtime = Runtime::default();
    runtime.rules.construction_labor = SimDuration(100);
    let mut store = Store::with_schema(schema());
    insert_project(&mut store, ProjectId(11), plan(11)).unwrap();
    store.insert_or_spawn(StableId(3), Foreign(7)).unwrap();
    (runtime, store, crate::transition_tests::definitions())
}
fn facts() -> ConstructionFacts {
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
#[test]
fn native_project_admission_has_intrinsic_bounds_and_no_metadata_entity_maps() {
    let (runtime, mut store, _) = fixture();
    let before = store.snapshot().unwrap();
    let mut bad = plan(12);
    bad.progress_ms = 1;
    assert!(insert_project(&mut store, ProjectId(12), bad).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let meta = serde_json::to_value(&runtime).unwrap();
    assert_eq!(meta.as_object().unwrap().len(), 2);
    assert!(
        serde_json::from_value::<Runtime>(
            serde_json::json!({"rules":runtime.rules,"last_event":0,"projects":{}})
        )
        .is_err()
    );
}
#[test]
fn combined_foreign_admission_failure_keeps_rows_and_cursor_unpublished() {
    let (mut runtime, mut store, defs) = fixture();
    let before = store.snapshot().unwrap();
    let prepared = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap();
    let mut batch = Batch::new();
    let finish = prepared
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    batch.insert(StableId(3), Foreign(99));
    assert!(store.apply_batch(batch).is_err());
    drop(finish);
    assert_eq!(runtime.last_event, 0);
    assert_eq!(store.snapshot().unwrap(), before);
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    assert_eq!(store.get::<Foreign>(StableId(3)), Some(&Foreign(7)));
}
#[test]
fn relevant_slot_and_job_membership_changes_reject_prepared_construction() {
    let (runtime, mut store, defs) = fixture();
    let prepared = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap();
    let mut occupying = plan(12);
    occupying.materials_consumed = true;
    insert_project(&mut store, ProjectId(12), occupying).unwrap();
    assert_eq!(
        prepared.validate(&runtime, &store, &defs),
        Err(Error::Unavailable)
    );
    let (runtime, mut store, defs) = fixture();
    let prepared = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap();
    insert_construction(
        &mut store,
        PersonId(8),
        Construction {
            project: ProjectId(11),
            completes: SimTime(100),
        },
    )
    .unwrap();
    assert_eq!(
        prepared.validate(&runtime, &store, &defs),
        Err(Error::Unavailable)
    );
}
#[test]
fn labor_progress_and_exclusive_actor_guards_use_live_rows_without_cloning_metadata() {
    let (mut runtime, mut store, defs) = fixture();
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    let before = store.snapshot().unwrap();
    assert!(
        runtime
            .prepare_advance(
                &store,
                SimTime(0),
                SimTime(10),
                &[EnterpriseWork {
                    actor: PersonId(3),
                    site: SiteId(20)
                }]
            )
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let prepared = runtime
        .prepare_advance(&store, SimTime(0), SimTime(10), &[])
        .unwrap();
    insert_construction(
        &mut store,
        PersonId(8),
        Construction {
            project: ProjectId(12),
            completes: SimTime(10),
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        prepared.commit(&mut runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn selected_recipe_change_rejects_completion_before_any_native_publish() {
    let (mut runtime, mut store, mut defs) = fixture();
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    runtime
        .prepare_advance(&store, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    let completion = runtime
        .prepare_completion(
            &store,
            &defs,
            PersonId(3),
            CompletionFacts {
                now: SimTime(100),
                capable: true,
                asset_available: true,
                site: SiteId(20),
                enterprise: EnterpriseId(21),
            },
        )
        .unwrap();
    defs.catalog
        .recipes
        .get_mut(&game_workshop::RecipeId("tools".into()))
        .unwrap()
        .duration = SimDuration(7);
    let before = store.snapshot().unwrap();
    assert_eq!(
        completion.commit(&mut runtime, &mut store, &defs),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(enterprise(&store, EnterpriseId(21)).is_none());
}
#[test]
fn enterprise_labor_credits_the_explicit_body_once_and_preserves_other_fields() {
    let (mut runtime, mut store, defs) = fixture();
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    runtime
        .prepare_advance(&store, SimTime(0), SimTime(100), &[])
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    runtime
        .prepare_completion(
            &store,
            &defs,
            PersonId(3),
            CompletionFacts {
                now: SimTime(100),
                capable: true,
                asset_available: true,
                site: SiteId(20),
                enterprise: EnterpriseId(21),
            },
        )
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    let work = [EnterpriseWork {
        actor: PersonId(3),
        site: SiteId(20),
    }];
    runtime
        .prepare_advance(&store, SimTime(100), SimTime(110), &work)
        .unwrap()
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert_eq!(
        enterprise(&store, EnterpriseId(21))
            .unwrap()
            .labor
            .get(&PersonId(3)),
        Some(&10)
    );
    assert!(
        runtime
            .prepare_advance(&store, SimTime(110), SimTime(120), &[work[0], work[0]])
            .is_err()
    );
    assert_eq!(store.get::<Foreign>(StableId(3)), Some(&Foreign(7)));
    let restored = Store::restore(schema(), store.snapshot().unwrap()).unwrap();
    assert_eq!(restored.snapshot().unwrap(), store.snapshot().unwrap());
    runtime
        .validate_owned(&restored, &defs, SimTime(110))
        .unwrap();
}
#[test]
fn funding_bound_includes_completed_workshops_and_excludes_closed_or_consumed_plans() {
    let (runtime, mut store, _) = fixture();
    remove_project(&mut store, ProjectId(11)).unwrap();
    for (id, farm, status, consumed, finish) in [
        (1, false, ProjectStatus::Blocked, false, u64::MAX),
        (2, false, ProjectStatus::Completed, true, 0),
        (3, true, ProjectStatus::Blocked, false, u64::MAX),
        (4, true, ProjectStatus::Completed, true, 0),
        (5, false, ProjectStatus::Cancelled, false, u64::MAX),
        (6, false, ProjectStatus::Failed, false, u64::MAX),
        (7, true, ProjectStatus::Blocked, false, 60_000),
    ] {
        let mut row = plan(id);
        if farm {
            row.kind = ProjectKind::Farm {
                plot: SiteId(10),
                water: InventoryId(9),
            };
        }
        row.status = status;
        row.materials_consumed = consumed;
        row.finish_by = SimTime(finish);
        if status == ProjectStatus::Completed {
            row.enterprise = Some(EnterpriseId(100 + id));
            row.progress_ms = 100;
            row.contributions.insert(PersonId(3), 100);
        }
        insert_project(&mut store, ProjectId(id), row).unwrap();
    }
    assert_eq!(
        runtime
            .funding_proposal_bound(&store, SimTime(60_000))
            .unwrap(),
        3
    );
    assert_eq!(
        runtime.allocation_bound(&store, SimTime(60_000)).unwrap(),
        0
    );
}

#[test]
fn enterprise_reconciliation_uses_selected_recipe_and_guards_its_thresholds() {
    let (mut runtime, mut store, mut definitions) = fixture();
    let recipe_id = game_workshop::RecipeId("food".into());
    definitions.catalog.recipes.insert(
        recipe_id.clone(),
        game_workshop::Recipe {
            input_good: Good::Seed,
            input_quantity: Quantity(700),
            output_good: Good::Food,
            output_quantity: Quantity(300),
            duration: SimDuration(20),
        },
    );
    let mut completed = plan(11);
    completed.kind = ProjectKind::Workshop {
        recipe: recipe_id.clone(),
    };
    completed.status = ProjectStatus::Completed;
    completed.enterprise = Some(EnterpriseId(21));
    completed.materials_consumed = true;
    completed.progress_ms = 100;
    completed.contributions.insert(PersonId(3), 100);
    insert_project(&mut store, ProjectId(11), completed.clone()).unwrap();
    insert_enterprise(
        &mut store,
        EnterpriseId(21),
        Enterprise {
            kind: completed.kind.clone(),
            id: EnterpriseId(21),
            project: ProjectId(11),
            sponsor: completed.sponsor,
            area: completed.area,
            site: SiteId(20),
            beneficiaries: completed.beneficiaries,
            workers: completed.workers,
            status: EnterpriseStatus::Ready,
            blocker: None,
            inputs: BTreeMap::new(),
            produced: BTreeMap::new(),
            labor: BTreeMap::new(),
            operating_agreements: BTreeSet::new(),
        },
    )
    .unwrap();
    let conditions = [EnterpriseCondition {
        enterprise: EnterpriseId(21),
        active: false,
        crop_failed: false,
        output_room: Quantity(500),
        workers: vec![WorkerCondition {
            capable: true,
            land_accessible: true,
            tool_available: true,
            materials_available: Quantity(700),
        }],
    }];
    let ready = runtime
        .prepare_reconcile(&store, &definitions, SimTime(100), &[], &conditions, &[], 1)
        .unwrap();
    ready
        .commit(&mut runtime, &mut store, &definitions)
        .unwrap();
    assert_eq!(
        enterprise(&store, EnterpriseId(21)).unwrap().status,
        EnterpriseStatus::Ready
    );
    let stale = runtime
        .prepare_reconcile(&store, &definitions, SimTime(100), &[], &conditions, &[], 2)
        .unwrap();
    definitions
        .catalog
        .recipes
        .get_mut(&recipe_id)
        .unwrap()
        .input_quantity = Quantity(701);
    let before = store.snapshot().unwrap();
    assert_eq!(
        stale.commit(&mut runtime, &mut store, &definitions),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(runtime.last_event, 1);
    runtime
        .prepare_reconcile(&store, &definitions, SimTime(100), &[], &conditions, &[], 2)
        .unwrap()
        .commit(&mut runtime, &mut store, &definitions)
        .unwrap();
    assert_eq!(
        enterprise(&store, EnterpriseId(21)).unwrap().blocker,
        Some(ProjectBlocker::Materials)
    );
}

#[test]
fn construction_and_actual_material_consumption_share_one_atomic_admission() {
    let mut native_schema = schema();
    game_cargo::register_components(&mut native_schema).unwrap();
    let mut store = Store::with_schema(native_schema);
    let mut runtime = Runtime::default();
    runtime.rules.construction_labor = SimDuration(100);
    let definitions = crate::transition_tests::definitions();
    insert_project(&mut store, ProjectId(11), plan(11)).unwrap();
    store.insert_or_spawn(StableId(3), Foreign(7)).unwrap();
    game_cargo::insert_inventory(
        &mut store,
        game_cargo::Inventory {
            id: InventoryId(4),
            location: game_cargo::InventoryLocation::Area(AreaId(2)),
            quantity: Quantity(5_000),
            capacity: Quantity(8_000),
            good: Good::Timber,
            access: BTreeSet::from([PersonId(3)]),
            last_granted: None,
            spoils_at: None,
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    let metadata = runtime.clone();
    let prepare = |runtime: &Runtime, store: &Store| {
        let project = runtime
            .prepare_construction(store, ProjectId(11), facts())
            .unwrap();
        let debits = project
            .effects()
            .iter()
            .filter_map(|effect| match effect {
                Effect::Consume {
                    source, quantity, ..
                } => Some((*source, *quantity)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let stock = game_cargo::prepare_consumptions(store, &debits).unwrap();
        let mut batch = Batch::new();
        stock.stage_into(store, &mut batch).unwrap();
        let finish = project
            .stage_into(runtime, store, &definitions, &mut batch)
            .unwrap();
        (batch, finish)
    };
    let (mut batch, finish) = prepare(&runtime, &store);
    batch.insert(StableId(3), Foreign(99));
    assert!(store.apply_batch(batch).is_err());
    drop(finish);
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(runtime, metadata);
    let (batch, finish) = prepare(&runtime, &store);
    store.apply_batch(batch).unwrap();
    let effects = finish.finish(&mut runtime);
    assert!(effects.contains(&Effect::Consume {
        actor: PersonId(3),
        source: InventoryId(4),
        quantity: Quantity(5_000)
    }));
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(4))
            .unwrap()
            .quantity,
        Quantity(0)
    );
    assert_eq!(
        project(&store, ProjectId(11)).unwrap().status,
        ProjectStatus::Active
    );
    assert_eq!(
        construction(&store, PersonId(3)).unwrap().project,
        ProjectId(11)
    );
    assert_eq!(store.get::<Foreign>(StableId(3)), Some(&Foreign(7)));
    runtime
        .validate_owned(&store, &definitions, SimTime(0))
        .unwrap();
}

#[test]
fn bulk_interruption_coalesces_repeated_actors_and_rejects_new_competing_jobs() {
    let (mut runtime, mut store, definitions) = fixture();
    let mut other = plan(12);
    other.sponsor = PersonId(4);
    other.beneficiaries = BTreeSet::from([PersonId(4)]);
    other.workers = other.beneficiaries.clone();
    insert_project(&mut store, ProjectId(12), other).unwrap();
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &definitions)
        .unwrap();
    let mut next = facts();
    next.actor.actor = PersonId(4);
    runtime
        .prepare_construction(&store, ProjectId(12), next)
        .unwrap()
        .commit(&mut runtime, &mut store, &definitions)
        .unwrap();
    let prepared = runtime
        .prepare_interrupt_many(&store, &[PersonId(4), PersonId(3), PersonId(4)])
        .unwrap();
    insert_construction(
        &mut store,
        PersonId(8),
        Construction {
            project: ProjectId(11),
            completes: SimTime(100),
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        prepared.commit(&mut runtime, &mut store, &definitions),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
    remove_construction(&mut store, PersonId(8)).unwrap();
    let effects = runtime
        .prepare_interrupt_many(&store, &[PersonId(4), PersonId(3), PersonId(4)])
        .unwrap()
        .commit(&mut runtime, &mut store, &definitions)
        .unwrap();
    assert!(effects.is_empty());
    assert!(constructions(&store).next().is_none());
    for id in [ProjectId(11), ProjectId(12)] {
        assert_eq!(project(&store, id).unwrap().status, ProjectStatus::Ready);
        assert!(project(&store, id).unwrap().materials_consumed);
    }
    assert_eq!(store.get::<Foreign>(StableId(3)), Some(&Foreign(7)));
}

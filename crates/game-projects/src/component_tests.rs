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

fn completed_fixture() -> (Runtime, Store, game_workshop::Definitions) {
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
    (runtime, store, defs)
}
#[test]
fn project_finalization_guards_affinity_owned_rows_source_rules_and_cursor_before_publication() {
    let (mut runtime, mut store, defs) = fixture();
    let mut batch = Batch::new();
    let before = store.snapshot().unwrap();
    let finish = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    finish
        .validate_batch(&runtime, &store, &defs, &batch)
        .unwrap();
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &Batch::new())
            .is_err()
    );
    runtime.rules.construction_labor = SimDuration(101);
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
    runtime.rules.construction_labor = SimDuration(100);
    batch.remove::<Project>(StableId(11));
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    project_mut(&mut store, ProjectId(11)).unwrap().blocker = Some(ProjectBlocker::Space);
    let changed = store.snapshot().unwrap();
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), changed);
    let (mut runtime, store, defs) = completed_fixture();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_outputs(&store, &defs, &[], Some(7))
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    runtime.last_event = 1;
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
}
#[test]
fn selected_recipe_changes_and_late_global_job_membership_cannot_publish_accounting() {
    let (runtime, store, mut defs) = completed_fixture();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_input(&store, &defs, SiteId(20), Good::Timber, Quantity(2000))
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    defs.catalog
        .recipes
        .get_mut(&game_workshop::RecipeId("tools".into()))
        .unwrap()
        .duration = SimDuration(7);
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
    assert!(
        enterprise(&store, EnterpriseId(21))
            .unwrap()
            .inputs
            .is_empty()
    );
    let (runtime, store, defs) = fixture();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    batch.insert(
        StableId(4),
        Construction {
            project: ProjectId(12),
            completes: SimTime(100),
        },
    );
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
}
#[test]
fn input_noop_and_selected_membership_reject_new_or_moved_enterprises_but_allow_other_sites() {
    let (runtime, mut store, defs) = completed_fixture();
    let original = enterprise(&store, EnterpriseId(21)).unwrap().clone();
    for mode in 0..3 {
        let mut batch = Batch::new();
        let finish = runtime
            .prepare_input(&store, &defs, SiteId(30), Good::Timber, Quantity(2))
            .unwrap()
            .stage_into(&runtime, &store, &defs, &mut batch)
            .unwrap();
        let mut row = original.clone();
        row.id = if mode == 0 {
            EnterpriseId(21)
        } else {
            EnterpriseId(31)
        };
        row.site = if mode == 2 { SiteId(40) } else { SiteId(30) };
        batch.insert(StableId(row.id.0), row);
        assert_eq!(
            finish
                .validate_batch(&runtime, &store, &defs, &batch)
                .is_ok(),
            mode == 2
        );
    }
    // An existing unrelated enterprise update remains independent of the
    // selected site's input account; input is credited once after one batch.
    let mut other = original;
    other.id = EnterpriseId(31);
    other.site = SiteId(40);
    insert_enterprise(&mut store, other.id, other.clone()).unwrap();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_input(&store, &defs, SiteId(20), Good::Timber, Quantity(2))
        .unwrap()
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    other.blocker = Some(ProjectBlocker::Space);
    batch.insert(StableId(31), other);
    finish
        .validate_batch(&runtime, &store, &defs, &batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    let mut runtime = runtime;
    finish.finish(&mut runtime);
    assert_eq!(
        enterprise(&store, EnterpriseId(21)).unwrap().inputs[&Good::Timber],
        2
    );
}
#[test]
fn area_and_actor_membership_final_guards_track_only_their_selected_scopes() {
    let (runtime, store, defs) = fixture();
    for related in [false, true] {
        let mut batch = Batch::new();
        let finish = runtime
            .prepare_construction(&store, ProjectId(11), facts())
            .unwrap()
            .stage_into(&runtime, &store, &defs, &mut batch)
            .unwrap();
        let mut other = plan(12);
        other.materials_consumed = true;
        other.area = AreaId(if related { 2 } else { 3 });
        batch.insert(StableId(12), other);
        assert_eq!(
            finish
                .validate_batch(&runtime, &store, &defs, &batch)
                .is_err(),
            related
        );
    }
    let (mut runtime, mut store, defs) = fixture();
    runtime
        .prepare_construction(&store, ProjectId(11), facts())
        .unwrap()
        .commit(&mut runtime, &mut store, &defs)
        .unwrap();
    for related in [false, true] {
        let change = runtime.prepare_interrupt(&store, PersonId(3)).unwrap();
        let mut batch = Batch::new();
        let finish = change
            .stage_into(&runtime, &store, &defs, &mut batch)
            .unwrap();
        batch.insert(
            StableId(4),
            Construction {
                project: ProjectId(if related { 11 } else { 12 }),
                completes: SimTime(100),
            },
        );
        assert_eq!(
            finish
                .validate_batch(&runtime, &store, &defs, &batch)
                .is_err(),
            related
        );
    }
}
#[test]
fn fresh_project_allocations_reject_foreign_entities_and_late_other_component_edits() {
    let (runtime, mut store, defs) = fixture();
    store.insert_or_spawn(StableId(12), Foreign(7)).unwrap();
    let mut change = PreparedChange::new(&runtime);
    change.project(&store, ProjectId(12), plan(12));
    let before = store.snapshot().unwrap();
    assert!(
        change
            .stage_into(&runtime, &store, &defs, &mut Batch::new())
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), before);
    let mut change = PreparedChange::new(&runtime);
    change.project(&store, ProjectId(13), plan(13));
    let mut batch = Batch::new();
    let finish = change
        .stage_into(&runtime, &store, &defs, &mut batch)
        .unwrap();
    batch.insert(StableId(13), Foreign(7));
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &batch)
            .is_err()
    );
    assert!(project(&store, ProjectId(13)).is_none());
}

#[test]
fn farm_membership_guard_rejects_new_competing_plot_and_preserves_other_plots() {
    let (runtime, store, defs) = fixture();
    let people = BTreeSet::from([PersonId(3)]);
    let stocks = [
        (4, Good::Timber),
        (5, Good::Tool),
        (6, Good::Seed),
        (7, Good::Food),
        (8, Good::Water),
    ]
    .into_iter()
    .map(|(id, good)| {
        (
            InventoryId(id),
            StockFact {
                good,
                area: Some(AreaId(2)),
            },
        )
    })
    .collect();
    let action = ProjectAction::ProposeFarm {
        plot: SiteId(30),
        materials: InventoryId(4),
        tool: InventoryId(5),
        seed: InventoryId(6),
        water: InventoryId(8),
        food: InventoryId(7),
        beneficiaries: people.clone(),
        workers: people.clone(),
        finish_by: SimTime(500),
    };
    for (related, status) in [false, true].into_iter().flat_map(|related| {
        [ProjectStatus::Ready, ProjectStatus::Completed]
            .into_iter()
            .map(move |status| (related, status))
    }) {
        let change = runtime
            .prepare_proposal(
                &store,
                &defs,
                ProjectId(12),
                &action,
                ProposalFacts {
                    actor: facts().actor,
                    people: &people,
                    stocks: &stocks,
                    land_area: Some(AreaId(2)),
                    land_accessible: true,
                },
            )
            .unwrap();
        let mut batch = Batch::new();
        let finish = change
            .stage_into(&runtime, &store, &defs, &mut batch)
            .unwrap();
        let mut other = plan(13);
        other.kind = ProjectKind::Farm {
            plot: SiteId(if related { 30 } else { 31 }),
            water: InventoryId(8),
        };
        other.status = status;
        if status == ProjectStatus::Completed {
            other.enterprise = Some(EnterpriseId(99));
            other.materials_consumed = true;
            other.progress_ms = runtime.rules.construction_labor.0;
            other.contributions.insert(PersonId(3), other.progress_ms);
        }
        other.validate().unwrap();
        batch.insert(StableId(13), other);
        assert_eq!(
            finish
                .validate_batch(&runtime, &store, &defs, &batch)
                .is_err(),
            related
        );
    }
}
#[test]
fn workshop_consumption_and_input_accounting_stage_together_and_late_rejection_is_inert() {
    let (mut runtime, old, defs) = completed_fixture();
    let mut schema = old.schema().clone();
    game_cargo::register_components(&mut schema).unwrap();
    let mut snapshot = old.snapshot().unwrap();
    for key in schema.keys() {
        snapshot.columns.entry(key.into()).or_default();
    }
    let mut store = Store::restore(schema, snapshot).unwrap();
    let stock = |id, good, quantity| game_cargo::Inventory {
        id: InventoryId(id),
        good,
        quantity: Quantity(quantity),
        capacity: Quantity(20000),
        location: game_cargo::InventoryLocation::Area(AreaId(2)),
        access: BTreeSet::from([PersonId(3)]),
        last_granted: None,
        spoils_at: None,
    };
    for row in [
        stock(6, Good::Timber, 4000),
        stock(5, Good::Tool, 2000),
        stock(7, Good::Tool, 0),
    ] {
        game_cargo::insert_inventory(&mut store, row).unwrap();
    }
    let site = game_workshop::Site {
        id: SiteId(20),
        area: AreaId(2),
        recipe: game_workshop::RecipeId("tools".into()),
        input: InventoryId(6),
        tool: InventoryId(5),
        output: InventoryId(7),
        access: BTreeSet::from([PersonId(3)]),
        last_granted: None,
    };
    let consumption = defs
        .resolve_site(&site)
        .unwrap()
        .prepare_start(
            None,
            game_cargo::inventory(&store, InventoryId(6)).unwrap(),
            game_cargo::inventory(&store, InventoryId(5)).unwrap(),
            game_cargo::inventory(&store, InventoryId(7)).unwrap(),
            game_workshop::StartFacts {
                actor: PersonId(3),
                area: AreaId(2),
                capable: true,
                adult: true,
                fatigue_allows: true,
                site_access: true,
                occupied_by_other: false,
                input_usable: true,
                input_unreserved: Quantity(2000),
                tool_usable: true,
                rate: SkillRate::default(),
                body_rate: SkillRate::default(),
            },
        )
        .unwrap()
        .consumption
        .unwrap();
    let before = store.snapshot().unwrap();
    let stage = |store: &Store| {
        let mut batch = Batch::new();
        let cargo = consumption.stage_into(store, &mut batch).unwrap();
        let finish = runtime
            .prepare_input(store, &defs, site.id, Good::Timber, consumption.quantity)
            .unwrap()
            .stage_into(&runtime, store, &defs, &mut batch)
            .unwrap();
        (batch, cargo, finish)
    };
    let (mut rejected, cargo, finish) = stage(&store);
    rejected.remove::<Enterprise>(StableId(21));
    assert!(
        finish
            .validate_batch(&runtime, &store, &defs, &rejected)
            .is_err()
    );
    cargo.validate_batch(&rejected).unwrap();
    assert_eq!(store.snapshot().unwrap(), before);
    let (batch, cargo, finish) = stage(&store);
    cargo.validate_batch(&batch).unwrap();
    finish
        .validate_batch(&runtime, &store, &defs, &batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut runtime);
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(6))
            .unwrap()
            .quantity,
        Quantity(2000)
    );
    assert_eq!(
        enterprise(&store, EnterpriseId(21)).unwrap().inputs[&Good::Timber],
        2000
    );
}

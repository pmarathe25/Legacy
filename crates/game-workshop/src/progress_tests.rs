use super::*;
use game_ecs::{Batch, Schema, StableId, Store};

fn store(schema: Schema) -> Store {
    Store::with_schema(schema)
}
fn schema() -> Schema {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    schema
}
fn populated(mut store: Store, work: Job) -> Store {
    insert_site(&mut store, crate::tests::site()).unwrap();
    for stock in [
        crate::tests::stock(1, Good::Timber, 4_000),
        crate::tests::stock(2, Good::Tool, 1_000),
        crate::tests::stock(3, Good::Food, 0),
    ] {
        game_cargo::insert_inventory(&mut store, stock).unwrap();
    }
    insert_job(&mut store, PersonId(1), work).unwrap();
    store
}
fn work() -> Job {
    Job {
        site: SiteId(5),
        remaining: 10_001,
        rate: SkillRate {
            numerator: 7,
            denominator: 3,
        },
        body_rate: SkillRate::default(),
        work_remainder: 0,
        tool: Some(InventoryId(2)),
    }
}
fn facts(elapsed: u64) -> ProgressFacts {
    ProgressFacts {
        elapsed,
        effective_rate: SkillRate {
            numerator: 7,
            denominator: 3,
        },
        active: true,
        equipped: true,
    }
}
#[test]
fn native_progress_preserves_frozen_rates_and_fractional_slice_equivalence() {
    let definitions = crate::tests::definitions();
    let mut whole = populated(store(schema()), work());
    let mut split = populated(store(schema()), work());
    definitions
        .prepare_progress(&whole, PersonId(1), facts(151))
        .unwrap()
        .unwrap()
        .commit(&definitions, &mut whole)
        .unwrap();
    for elapsed in [100, 51] {
        let progress = definitions
            .prepare_progress(&split, PersonId(1), facts(elapsed))
            .unwrap()
            .unwrap();
        assert_eq!(progress.after().rate, work().rate);
        assert_eq!(progress.after().body_rate, work().body_rate);
        progress.commit(&definitions, &mut split).unwrap();
    }
    assert_eq!(whole.snapshot().unwrap(), split.snapshot().unwrap());
    let work = job(&whole, PersonId(1)).unwrap();
    assert_eq!(work.remaining, 9_649);
    assert_eq!(work.work_remainder, 1);
    assert_eq!(
        work.completion_after(facts(0).effective_rate).unwrap(),
        4_136
    );
}
#[test]
fn paused_or_unequipped_native_progress_has_no_elapsed_effect() {
    let definitions = crate::tests::definitions();
    let mut store = populated(store(schema()), work());
    let opening = store.snapshot().unwrap();
    for facts in [
        ProgressFacts {
            active: false,
            ..facts(9_000)
        },
        ProgressFacts {
            equipped: false,
            ..facts(9_000)
        },
    ] {
        definitions
            .prepare_progress(&store, PersonId(1), facts)
            .unwrap()
            .unwrap()
            .commit(&definitions, &mut store)
            .unwrap();
        assert_eq!(store.snapshot().unwrap(), opening);
    }
}
#[test]
fn native_progress_rejects_stale_jobs_sites_and_late_recipe_edits() {
    let mut definitions = crate::tests::definitions();
    let mut store = populated(store(schema()), work());
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(100))
        .unwrap()
        .unwrap();
    job_mut(&mut store, PersonId(1)).unwrap().remaining -= 1;
    let changed = store.snapshot().unwrap();
    assert!(progress.clone().commit(&definitions, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), changed);
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(100))
        .unwrap()
        .unwrap();
    let mut batch = Batch::new();
    let publication = progress.stage(&definitions, &store, &mut batch).unwrap();
    definitions
        .catalog
        .recipes
        .get_mut(&RecipeId("meal".into()))
        .unwrap()
        .duration
        .0 += 1;
    assert_eq!(
        publication.validate(&definitions, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), changed);
    definitions
        .catalog
        .recipes
        .get_mut(&RecipeId("meal".into()))
        .unwrap()
        .duration
        .0 -= 1;
    publication.validate(&definitions, &store, &batch).unwrap();
    let mut other = site(&store, SiteId(5)).unwrap().clone();
    other.access.clear();
    batch.insert(StableId(5), other);
    assert_eq!(
        publication.validate(&definitions, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), changed);
}
#[test]
fn strict_native_job_schema_rejects_progress_without_partial_elapsed_work() {
    let mut schema = Schema::new();
    schema
        .register::<Site>("workshop/site", Site::validate)
        .unwrap();
    schema
        .register::<Job>("workshop/job", |work| {
            work.validate()?;
            if work.remaining != 10_001 {
                return Err(Error::InvalidConfiguration("closing labor rejected".into()));
            }
            Ok(())
        })
        .unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = populated(store(schema), work());
    let definitions = crate::tests::definitions();
    let opening = store.snapshot().unwrap();
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(100))
        .unwrap()
        .unwrap();
    assert!(progress.commit(&definitions, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), opening);
}
#[test]
fn native_start_publishes_one_consumption_and_rejects_tampered_job_or_changed_tool() {
    let definitions = crate::tests::definitions();
    let mut store = populated(store(schema()), work());
    remove_job(&mut store, PersonId(1)).unwrap();
    let prepare = |store: &Store| {
        definitions
            .resolve_site(site(store, SiteId(5)).unwrap())
            .unwrap()
            .prepare_start(
                None,
                game_cargo::inventory(store, InventoryId(1)).unwrap(),
                game_cargo::inventory(store, InventoryId(2)).unwrap(),
                game_cargo::inventory(store, InventoryId(3)).unwrap(),
                crate::tests::facts(),
            )
            .unwrap()
    };
    let start = prepare(&store);
    let opening = store.snapshot().unwrap();
    let mut tampered = start.clone();
    tampered.job.remaining = 0;
    assert!(tampered.commit(&definitions, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), opening);
    let mut batch = Batch::new();
    let publication = start.stage_into(&definitions, &store, &mut batch).unwrap();
    batch.insert(StableId(2), crate::tests::stock(2, Good::Tool, 0));
    assert_eq!(
        publication.validate(&definitions, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), opening);
    start.commit(&definitions, &mut store).unwrap();
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(1))
            .unwrap()
            .quantity,
        Quantity(2_000)
    );
    assert!(job(&store, PersonId(1)).is_some());
}

#[test]
fn native_cancellation_discards_work_without_refund_and_rejects_stale_restart() {
    let mut store = populated(store(schema()), work());
    let cancellation = prepare_cancel(&store, PersonId(1));
    job_mut(&mut store, PersonId(1)).unwrap().remaining -= 1;
    let changed = store.snapshot().unwrap();
    assert_eq!(cancellation.commit(&mut store), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), changed);
    let input = game_cargo::inventory(&store, InventoryId(1))
        .unwrap()
        .quantity;
    prepare_cancel(&store, PersonId(1))
        .commit(&mut store)
        .unwrap();
    assert!(job(&store, PersonId(1)).is_none());
    assert_eq!(
        game_cargo::inventory(&store, InventoryId(1))
            .unwrap()
            .quantity,
        input
    );
    prepare_cancel(&store, PersonId(1))
        .commit(&mut store)
        .unwrap();
}

#[test]
fn completed_interval_publishes_output_directly_from_opening_job_and_composes_death_cancel() {
    let definitions = crate::tests::definitions();
    let mut store = populated(store(schema()), work());
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(4_287))
        .unwrap()
        .unwrap();
    assert_eq!(progress.progress().remaining, 0);
    let completed = progress
        .prepare_completion(
            &definitions,
            &store,
            CompletionFacts {
                actor: PersonId(1),
                active: true,
                alive_at_start: true,
                equipped: true,
            },
        )
        .unwrap()
        .unwrap();
    let receipt = completed.receipt();
    assert_eq!(receipt.quantity, Quantity(4_000));
    let mut batch = Batch::new();
    let publication = completed.stage(&definitions, &store, &mut batch).unwrap();
    let death = prepare_cancel(&store, PersonId(1))
        .stage_into_with_completion(&store, &mut batch, publication.completed_job())
        .unwrap();
    publication.validate(&definitions, &store, &batch).unwrap();
    death.validate(&store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert!(job(&store, PersonId(1)).is_none());
    assert_eq!(
        game_cargo::inventory(&store, receipt.output)
            .unwrap()
            .quantity,
        Quantity(4_000)
    );
    assert_eq!(
        site(&store, receipt.site).unwrap().last_granted,
        Some(PersonId(1))
    );
}
#[test]
fn terminal_work_and_foreign_body_schema_fail_together_without_partial_elapsed_output() {
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct ClosingBody(u64);
    let mut schema = schema();
    schema
        .register::<ClosingBody>("foreign/body", |body| {
            if body.0 == 0 {
                return Err(Error::InvalidConfiguration(
                    "foreign body result rejected".into(),
                ));
            }
            Ok(())
        })
        .unwrap();
    let mut store = populated(store(schema), work());
    store.insert_or_spawn(StableId(1), ClosingBody(1)).unwrap();
    let definitions = crate::tests::definitions();
    let before = store.snapshot().unwrap();
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(4_287))
        .unwrap()
        .unwrap();
    let completed = progress
        .prepare_completion(
            &definitions,
            &store,
            CompletionFacts {
                actor: PersonId(1),
                active: true,
                alive_at_start: true,
                equipped: true,
            },
        )
        .unwrap()
        .unwrap();
    let mut batch = Batch::new();
    batch.insert(StableId(1), ClosingBody(0));
    let publication = completed.stage(&definitions, &store, &mut batch).unwrap();
    publication.validate(&definitions, &store, &batch).unwrap();
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn terminal_work_checks_opening_life_equipment_capacity_and_late_output_or_recipe_changes() {
    let mut definitions = crate::tests::definitions();
    let mut store = populated(store(schema()), work());
    let progress = definitions
        .prepare_progress(&store, PersonId(1), facts(4_287))
        .unwrap()
        .unwrap();
    for facts in [
        CompletionFacts {
            actor: PersonId(1),
            active: true,
            alive_at_start: false,
            equipped: true,
        },
        CompletionFacts {
            actor: PersonId(1),
            active: true,
            alive_at_start: true,
            equipped: false,
        },
    ] {
        assert!(
            progress
                .prepare_completion(&definitions, &store, facts)
                .unwrap()
                .is_none()
        );
    }
    let facts = CompletionFacts {
        actor: PersonId(1),
        active: true,
        alive_at_start: true,
        equipped: true,
    };
    let completed = progress
        .prepare_completion(&definitions, &store, facts)
        .unwrap()
        .unwrap();
    let opening = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let publication = completed.stage(&definitions, &store, &mut batch).unwrap();
    definitions
        .catalog
        .recipes
        .get_mut(&RecipeId("meal".into()))
        .unwrap()
        .output_quantity
        .0 += 1;
    assert_eq!(
        publication.validate(&definitions, &store, &batch),
        Err(Error::Unavailable)
    );
    definitions
        .catalog
        .recipes
        .get_mut(&RecipeId("meal".into()))
        .unwrap()
        .output_quantity
        .0 -= 1;
    batch.insert(StableId(3), crate::tests::stock(3, Good::Food, 1));
    assert_eq!(
        publication.validate(&definitions, &store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), opening);
    game_cargo::inventory_mut(&mut store, InventoryId(3))
        .unwrap()
        .quantity = Quantity(20_000);
    assert!(
        progress
            .prepare_completion(&definitions, &store, facts)
            .unwrap()
            .is_none()
    );
}

#[test]
fn preevaluated_workers_share_site_while_receiving_cursor_changes_between_commits() {
    let definitions = crate::tests::definitions();
    for second_completes in [false, true] {
        let mut store = populated(store(schema()), work());
        insert_job(&mut store, PersonId(2), work()).unwrap();
        let first = definitions
            .prepare_progress(&store, PersonId(1), facts(4_287))
            .unwrap()
            .unwrap();
        let second = definitions
            .prepare_progress(
                &store,
                PersonId(2),
                facts(if second_completes { 4_287 } else { 100 }),
            )
            .unwrap()
            .unwrap();
        first
            .prepare_completion(
                &definitions,
                &store,
                CompletionFacts {
                    actor: PersonId(1),
                    active: true,
                    alive_at_start: true,
                    equipped: true,
                },
            )
            .unwrap()
            .unwrap()
            .commit(&definitions, &mut store)
            .unwrap();
        assert_eq!(
            site(&store, SiteId(5)).unwrap().last_granted,
            Some(PersonId(1))
        );
        if second_completes {
            second
                .prepare_completion(
                    &definitions,
                    &store,
                    CompletionFacts {
                        actor: PersonId(2),
                        active: true,
                        alive_at_start: true,
                        equipped: true,
                    },
                )
                .unwrap()
                .unwrap()
                .commit(&definitions, &mut store)
                .unwrap();
            assert!(job(&store, PersonId(2)).is_none());
            assert_eq!(
                site(&store, SiteId(5)).unwrap().last_granted,
                Some(PersonId(2))
            );
            assert_eq!(
                game_cargo::inventory(&store, InventoryId(3))
                    .unwrap()
                    .quantity,
                Quantity(8_000)
            );
        } else {
            let progress = second.progress();
            second.commit(&definitions, &mut store).unwrap();
            assert_eq!(
                job(&store, PersonId(2)).unwrap().remaining,
                progress.remaining
            );
            assert_eq!(
                site(&store, SiteId(5)).unwrap().last_granted,
                Some(PersonId(1))
            );
            assert_eq!(
                game_cargo::inventory(&store, InventoryId(3))
                    .unwrap()
                    .quantity,
                Quantity(4_000)
            );
        }
    }
}
#[test]
fn receiving_cursor_is_ignored_without_ignoring_declaration_or_recipe_changes() {
    for declaration_changed in [false, true] {
        let mut definitions = crate::tests::definitions();
        let mut store = populated(store(schema()), work());
        let progress = definitions
            .prepare_progress(&store, PersonId(1), facts(100))
            .unwrap()
            .unwrap();
        site_mut(&mut store, SiteId(5)).unwrap().last_granted = Some(PersonId(2));
        if declaration_changed {
            site_mut(&mut store, SiteId(5)).unwrap().input = InventoryId(9);
        } else {
            definitions
                .catalog
                .recipes
                .get_mut(&RecipeId("meal".into()))
                .unwrap()
                .input_quantity
                .0 += 1;
        }
        let opening = store.snapshot().unwrap();
        assert!(progress.commit(&definitions, &mut store).is_err());
        assert_eq!(store.snapshot().unwrap(), opening);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct WorkActivity(u8);
fn interval_store(reject_idle: bool) -> Store {
    let mut schema = schema();
    let validate: fn(&WorkActivity) -> Result<(), Error> = if reject_idle {
        |activity| {
            if activity.0 == 0 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        }
    } else {
        |_| Ok(())
    };
    schema
        .register::<WorkActivity>("test/work-activity", validate)
        .unwrap();
    let mut store = populated(store(schema), work());
    store.insert_or_spawn(StableId(1), WorkActivity(1)).unwrap();
    store
}
fn interval_facts() -> CompletionFacts {
    CompletionFacts {
        actor: PersonId(1),
        active: true,
        alive_at_start: true,
        equipped: true,
    }
}
#[test]
fn owner_work_interval_composes_cargo_output_with_guarded_receiving_idle() {
    let definitions = crate::tests::definitions();
    for died in [false, true] {
        let mut store = interval_store(false);
        let progress = definitions
            .prepare_progress(&store, PersonId(1), facts(4_287))
            .unwrap()
            .unwrap();
        let plan = prepare_work_interval(
            Some(&progress),
            &definitions,
            &store,
            interval_facts(),
            died,
        )
        .unwrap();
        assert!(!plan.failed());
        let receipt = plan.production().unwrap();
        let mut batch = Batch::new();
        let publication = plan
            .stage_into(&definitions, &store, &mut batch, WorkActivity(0))
            .unwrap();
        publication
            .completion
            .as_ref()
            .unwrap()
            .validate(&definitions, &store, &batch)
            .unwrap();
        assert!(publication.progress.is_none());
        assert_eq!(publication.activity.is_some(), !died);
        if let Some(guard) = &publication.activity {
            guard.validate(&store, &batch).unwrap();
        }
        store.apply_batch(batch).unwrap();
        assert!(job(&store, PersonId(1)).is_none());
        assert_eq!(
            game_cargo::inventory(&store, receipt.output)
                .unwrap()
                .quantity,
            Quantity(4_000)
        );
        assert_eq!(
            store.get::<WorkActivity>(StableId(1)),
            Some(&WorkActivity(if died { 1 } else { 0 }))
        );
    }
}
#[test]
fn interval_activity_rejection_and_late_replacement_preserve_opening_production() {
    let definitions = crate::tests::definitions();
    for reject_idle in [false, true] {
        let mut store = interval_store(reject_idle);
        let before = store.snapshot().unwrap();
        let progress = definitions
            .prepare_progress(&store, PersonId(1), facts(4_287))
            .unwrap()
            .unwrap();
        let plan = prepare_work_interval(
            Some(&progress),
            &definitions,
            &store,
            interval_facts(),
            false,
        )
        .unwrap();
        let mut batch = Batch::new();
        let publication = plan
            .stage_into(&definitions, &store, &mut batch, WorkActivity(0))
            .unwrap();
        let guard = publication.activity.unwrap();
        if reject_idle {
            assert!(store.apply_batch(batch).is_err());
        } else {
            batch.insert(StableId(1), WorkActivity(2));
            assert!(guard.validate(&store, &batch).is_err());
        }
        assert_eq!(before, store.snapshot().unwrap());
    }
}
#[test]
fn interval_completion_failure_and_unequipped_pause_retain_work() {
    let definitions = crate::tests::definitions();
    for equipped in [false, true] {
        let mut store = interval_store(false);
        game_cargo::inventory_mut(&mut store, InventoryId(3))
            .unwrap()
            .capacity = Quantity(0);
        let progress = definitions
            .prepare_progress(&store, PersonId(1), facts(4_287))
            .unwrap()
            .unwrap();
        let plan = prepare_work_interval(
            Some(&progress),
            &definitions,
            &store,
            CompletionFacts {
                equipped,
                ..interval_facts()
            },
            false,
        )
        .unwrap();
        assert_eq!(plan.failed(), equipped);
        assert!(plan.production().is_none());
        let mut batch = Batch::new();
        let publication = plan
            .stage_into(&definitions, &store, &mut batch, WorkActivity(0))
            .unwrap();
        publication
            .progress
            .unwrap()
            .validate(&definitions, &store, &batch)
            .unwrap();
        assert!(publication.activity.is_none());
        store.apply_batch(batch).unwrap();
        assert!(job(&store, PersonId(1)).is_some());
        assert_eq!(
            store.get::<WorkActivity>(StableId(1)),
            Some(&WorkActivity(1))
        );
    }
}

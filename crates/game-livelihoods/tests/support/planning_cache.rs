use super::support;
use game_core::*;
use game_sim::*;
use game_work_planning::{Goal, Plan};
use support::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Check {
    person: PersonId,
    inventory: InventoryId,
    goals: Vec<Goal>,
}
impl Request for Check {
    const KEY: &'static str = "test.compare-plans";
    type Response = Option<Plan>;
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct FailedRefresh(Check);
impl Request for FailedRefresh {
    const KEY: &'static str = "test.failed-refresh";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct WarmMany(Vec<Check>);
impl Request for WarmMany {
    const KEY: &'static str = "test.warm-many-plans";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Warm(Check);
impl Request for Warm {
    const KEY: &'static str = "test.warm-plans";
    type Response = ();
}
fn configured_builder() -> Builder {
    let mut b = builder();
    b.request::<WarmMany>(|tx, r| {
        let queries: Vec<_> =
            r.0.iter()
                .map(|r| game_work_planning::Query {
                    person: r.person,
                    inventories: vec![r.inventory],
                    goals: &r.goals,
                })
                .collect();
        game_work_planning::prepare(tx, &queries);
        Ok(())
    })
    .unwrap();
    b.request::<Warm>(|tx, Warm(r)| {
        game_work_planning::prepare(
            tx,
            &[game_work_planning::Query {
                person: r.person,
                inventories: vec![r.inventory],
                goals: &r.goals,
            }],
        );
        Ok(())
    })
    .unwrap();
    b.request::<Check>(|tx, r| {
        let actual = game_work_planning::choose(tx, r.person, &[r.inventory], &r.goals)?;
        let reference = game_work_planning::uncached(tx, r.person, &[r.inventory], &r.goals)?;
        assert_eq!(actual, reference);
        assert_eq!(
            game_work_planning::choose(tx, r.person, &[r.inventory], &r.goals)?,
            reference
        );
        Ok(actual)
    })
    .unwrap();
    b.request::<FailedRefresh>(|tx, r| {
        tx.call(game_cargo::Deposit {
            inventory: r.0.inventory,
            good: game_cargo::TIMBER,
            quantity: 12,
        })?;
        tx.call(game_knowledge::ObserveStore {
            person: r.0.person,
            inventory: r.0.inventory,
        })?;
        tx.call(r.0)?;
        Err(Error::Rejected("discard this observation".into()))
    })
    .unwrap();
    b
}
fn setup_plans() -> (Host, Check) {
    let (mut h, p, i, area) = setup();
    h.dispatch(game_ecology::Create(game_ecology::Patch {
        area,
        good: game_cargo::TIMBER,
        stock: 100,
        capacity: 100,
        renew: 0,
    }))
    .unwrap();
    h.dispatch(
        game_workshop::load(&mut game_content::Store::standard(), h.get().unwrap()).unwrap(),
    )
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: p,
        inventory: i,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveSurroundings { person: p })
        .unwrap();
    let check = Check {
        person: p,
        inventory: i,
        goals: vec![Goal {
            object: "charcoal".into(),
            quantity: 6,
            priority: 50,
            affinity: "discipline".into(),
            concept: "fuel".into(),
        }],
    };
    (
        configured_builder().load(&h.save().unwrap(), 4).unwrap(),
        check,
    )
}
#[test]
fn plan_reuse_matches_search_after_observation_rollback_stock_changes_and_loading() {
    let (mut h, check) = setup_plans();
    let before = h.query(check.clone()).unwrap().unwrap();
    assert!(
        before
            .steps
            .iter()
            .any(|s| matches!(s, game_work_planning::Step::Gather { .. }))
    );
    for _ in 0..12 {
        h.dispatch(game_knowledge::ObserveStore {
            person: check.person,
            inventory: check.inventory,
        })
        .unwrap();
        assert_eq!(h.query(check.clone()).unwrap(), Some(before.clone()));
    }
    let retained = h.get::<game_knowledge::Knowledge>().unwrap().clone();
    let save = h.save().unwrap();
    assert!(h.dispatch(FailedRefresh(check.clone())).is_err());
    assert_eq!(h.save().unwrap(), save);
    assert_eq!(h.query(check.clone()).unwrap(), Some(before.clone()));
    deposit(&mut h, check.inventory, "timber", 12);
    h.dispatch(game_knowledge::ObserveStore {
        person: check.person,
        inventory: check.inventory,
    })
    .unwrap();
    let stocked = h.query(check.clone()).unwrap().unwrap();
    assert!(
        !stocked
            .steps
            .iter()
            .any(|s| matches!(s, game_work_planning::Step::Gather { .. }))
    );
    assert_eq!(
        retained.0[&check.person]
            .get(&game_knowledge::store_key(check.inventory))
            .unwrap()
            .value
            .decode::<game_knowledge::Stock>()
            .unwrap()
            .get(&game_cargo::TIMBER),
        None
    );
    let loaded = configured_builder().load(&h.save().unwrap(), 2).unwrap();
    assert_eq!(loaded.query(check.clone()).unwrap(), Some(stocked));
    let (other, other_check) = setup_plans();
    assert_eq!(other_check.person, check.person);
    assert_eq!(other.query(other_check).unwrap(), Some(before));
    assert_eq!(
        h.query(check.clone()).unwrap(),
        loaded.query(check).unwrap()
    );
}
#[test]
fn goal_ranking_and_personal_traits_are_part_of_the_planning_inputs() {
    let (mut h, mut check) = setup_plans();
    check.goals.push(Goal {
        object: "timber".into(),
        quantity: 5,
        priority: 50,
        affinity: "discipline".into(),
        concept: "shelter".into(),
    });
    for _ in 0..20 {
        for (concept, outcome) in [("fuel", 1000), ("shelter", -1000)] {
            h.dispatch(game_relationships::LearnValue {
                person: check.person,
                concept: concept.into(),
                outcome,
            })
            .unwrap();
        }
    }
    let first = h.query(check.clone()).unwrap().unwrap();
    for _ in 0..40 {
        for (concept, outcome) in [("fuel", -1000), ("shelter", 1000)] {
            h.dispatch(game_relationships::LearnValue {
                person: check.person,
                concept: concept.into(),
                outcome,
            })
            .unwrap();
        }
    }
    let next = h.query(check.clone()).unwrap().unwrap();
    assert_ne!(first.goal, next.goal);
    for value in [0, 1000, 16000] {
        let t = h.get::<game_traits::Traits>().unwrap();
        let v = value.clamp(
            t.definitions["intelligence"].minimum,
            t.definitions["intelligence"].maximum,
        );
        h.dispatch(game_traits::Set {
            person: check.person,
            profile: [("intelligence".into(), v)].into(),
        })
        .unwrap();
        h.query(check.clone()).unwrap();
    }
}

#[test]
fn prepared_queries_revalidate_after_observation_goal_changes_and_rollback() {
    let (mut h, mut check) = setup_plans();
    let save = h.save().unwrap();
    h.query(Warm(check.clone())).unwrap();
    assert_eq!(h.save().unwrap(), save);
    let initial = h.query(check.clone()).unwrap().unwrap();
    h.query(Warm(check.clone())).unwrap();
    deposit(&mut h, check.inventory, "timber", 12);
    // Physical stock alone does not change the character's remembered plan.
    assert_eq!(h.query(check.clone()).unwrap(), Some(initial.clone()));
    h = configured_builder().load(&h.save().unwrap(), 4).unwrap();
    h.query(Warm(check.clone())).unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: check.person,
        inventory: check.inventory,
    })
    .unwrap();
    let changed = h.query(check.clone()).unwrap().unwrap();
    assert_ne!(changed, initial);
    assert!(
        !changed
            .steps
            .iter()
            .any(|s| matches!(s, game_work_planning::Step::Gather { .. }))
    );
    h = configured_builder().load(&h.save().unwrap(), 4).unwrap();
    h.query(Warm(check.clone())).unwrap();
    // Different goals must not accept the previously prepared winner.
    check.goals[0].object = "timber".into();
    check.goals[0].quantity = 50;
    h.query(check.clone()).unwrap();
    h.query(Warm(check.clone())).unwrap();
    assert!(h.dispatch(FailedRefresh(check.clone())).is_err());
    h.query(check).unwrap();
}

#[test]
fn independent_private_inputs_prepare_on_multiple_workers_and_match_the_serial_oracle() {
    let (mut h, first) = setup_plans();
    let area = game_transport::area(h.get().unwrap(), first.person).unwrap();
    let mut checks = vec![first.clone()];
    for _ in 0..7 {
        let p = person(&mut h, area);
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(p),
                capacity: 2000,
            })
            .unwrap();
        h.dispatch(game_knowledge::ObserveSurroundings { person: p })
            .unwrap();
        h.dispatch(game_knowledge::ObserveStore {
            person: p,
            inventory,
        })
        .unwrap();
        checks.push(Check {
            person: p,
            inventory,
            goals: first.goals.clone(),
        });
    }
    let save = h.save().unwrap();
    let reference: Vec<_> = checks.iter().map(|r| h.query(r.clone()).unwrap()).collect();
    for workers in [1, 2, 4] {
        let loaded = configured_builder().load(&save, workers).unwrap();
        loaded.query(WarmMany(checks.clone())).unwrap();
        assert_eq!(loaded.save().unwrap(), save);
        let actual: Vec<_> = checks
            .iter()
            .map(|r| loaded.query(r.clone()).unwrap())
            .collect();
        assert_eq!(actual, reference);
    }
}

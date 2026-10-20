mod support;
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
fn configured_builder() -> Builder {
    let mut b = builder();
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

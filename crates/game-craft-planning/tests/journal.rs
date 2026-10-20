use game_core::*;
use game_craft_planning::{Search, Step};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[path = "support/reference.rs"]
mod reference;

fn objects() -> game_objects::Objects {
    serde_json::from_value(serde_json::json!({
        "items": (1..=80).map(|id| (format!("good-{id}"), serde_json::json!({
            "id": id, "name": format!("Good {id}"), "mass": id % 5 + 1
        }))).collect::<BTreeMap<_, _>>(), "stamps": []
    }))
    .unwrap()
}

// The copying oracle checks traversal order, shared node budgets,
// branch rejection and exact winning stock, rather than just a target quantity.
#[test]
fn journal_matches_copying_search_across_cycles_capacities_and_budgets() {
    let objects = objects();
    for seed in 0..1200 {
        let n = |i| random(seed, i, 19);
        let mut recipes = game_workshop::RecipeChoices::new();
        for index in 0..18 {
            let output = GoodId(1 + n(index) % 8);
            recipes.entry(output).or_default().push((
                format!("recipe-{index}"),
                game_workshop::Recipe {
                    work: None,
                    inputs: [
                        (GoodId(1 + n(index + 20) % 8), 1 + n(index + 40) % 3),
                        (GoodId(1 + n(index + 60) % 8), 1),
                    ]
                    .into(),
                    output,
                    quantity: 1 + n(index + 80) % 4,
                    duration: SimDuration(1 + n(index + 100) % 100),
                    skill: "craft".into(),
                },
            ));
        }
        let patches: Vec<_> = (0..6)
            .map(|i| {
                (
                    i,
                    game_ecology::Patch {
                        area: AreaId(1),
                        good: GoodId(1 + n(i + 130) % 8),
                        stock: n(i + 150) % 30,
                        capacity: 30,
                        renew: 0,
                    },
                )
            })
            .collect();
        let possible: BTreeSet<_> = (1..=8)
            .filter(|i| n(i + 180) % 5 != 0)
            .map(GoodId)
            .collect();
        let stock = (1..=80).map(|i| (GoodId(i), n(i + 200) % 4)).collect();
        let mut search = Search {
            recipes: &recipes,
            patches: &patches,
            stock,
            steps: VecDeque::new(),
            gathered: BTreeMap::new(),
            cost: seed % 7,
            objects: &objects,
            capacity: 200 + n(300) % 600,
            possible: &possible,
            depth: 1 + (n(301) % 8) as usize,
            steps_limit: (n(302) % 24) as usize,
        };
        let mut old = reference::Search {
            recipes: &recipes,
            patches: &patches,
            stock: search.stock.clone(),
            steps: VecDeque::new(),
            gathered: BTreeMap::new(),
            cost: search.cost,
            objects: &objects,
            capacity: search.capacity,
            possible: &possible,
            depth: search.depth,
            steps_limit: search.steps_limit,
        };
        let (mut budget, mut old_budget) = ((n(303) % 100) as usize, (n(303) % 100) as usize);
        let (mut path, mut old_path) = (vec![], vec![]);
        let good = GoodId(1 + n(304) % 8);
        let amount = 1 + n(305) % 20;
        let old_result = old.supply(good, amount, &mut old_path, &mut old_budget);
        assert_eq!(
            search.supply(good, amount, &mut path, &mut budget),
            old_result,
            "seed {seed}"
        );
        assert_eq!((budget, path), (old_budget, old_path), "seed {seed}");
        assert_eq!(
            (&search.stock, &search.gathered, search.cost),
            (&old.stock, &old.gathered, old.cost),
            "seed {seed}"
        );
        assert_eq!(
            serde_json::to_value(&search.steps).unwrap(),
            serde_json::to_value(&old.steps).unwrap(),
            "seed {seed}"
        );
    }
}

#[test]
fn rejected_and_overflowing_branches_restore_existing_stock_steps_and_cost() {
    let objects = objects();
    let possible = [GoodId(1), GoodId(2)].into();
    let patches = [(
        7,
        game_ecology::Patch {
            area: AreaId(1),
            good: GoodId(1),
            stock: 10,
            capacity: 10,
            renew: 0,
        },
    )];
    let recipes = game_workshop::RecipeChoices::new();
    let mut search = Search {
        recipes: &recipes,
        patches: &patches,
        stock: [(GoodId(2), 3)].into(),
        steps: [Step::Craft {
            recipe: "earlier".into(),
        }]
        .into(),
        gathered: [(7, 1)].into(),
        cost: u64::MAX,
        objects: &objects,
        capacity: 100,
        possible: &possible,
        depth: 8,
        steps_limit: 32,
    };
    let initial = (
        search.stock.clone(),
        search.steps.clone(),
        search.gathered.clone(),
        search.cost,
    );
    let mut path = vec![GoodId(80)];
    assert!(search.supply(GoodId(1), 5, &mut path, &mut 100).is_none());
    assert_eq!(path, vec![GoodId(80)]);
    assert_eq!(
        (&search.stock, &search.steps, &search.gathered, search.cost),
        (&initial.0, &initial.1, &initial.2, initial.3)
    );
    search.cost = 0;
    search.capacity = 1;
    assert!(search.supply(GoodId(1), 5, &mut vec![], &mut 100).is_none());
    assert_eq!(
        (&search.stock, &search.steps, &search.gathered),
        (&initial.0, &initial.1, &initial.2)
    );
}

#[test]
fn a_recipe_consuming_its_own_output_cannot_promise_missing_inputs() {
    let objects = objects();
    let recipes = [(
        GoodId(1),
        vec![(
            "cycle".into(),
            game_workshop::Recipe {
                work: None,
                inputs: [(GoodId(1), 1)].into(),
                output: GoodId(1),
                quantity: 1,
                duration: MINUTE,
                skill: "craft".into(),
            },
        )],
    )]
    .into();
    let possible = [GoodId(1)].into();
    let mut search = Search {
        recipes: &recipes,
        patches: &[],
        stock: [(GoodId(1), 1)].into(),
        steps: VecDeque::new(),
        gathered: BTreeMap::new(),
        cost: 0,
        objects: &objects,
        capacity: 100,
        possible: &possible,
        depth: 8,
        steps_limit: 32,
    };
    assert!(search.supply(GoodId(1), 2, &mut vec![], &mut 100).is_none());
    assert_eq!(search.stock[&GoodId(1)], 1);
    assert!(search.steps.is_empty());
}

#[test]
fn incremental_load_matches_full_scan_when_consumption_recovers_from_overflow() {
    let objects = objects();
    for (input, held, ballast, capacity, accepted) in [
        (GoodId(1), u64::MAX, (GoodId(4), 0), 3, true),
        (GoodId(5), u64::MAX, (GoodId(4), 1), 8, true),
        (GoodId(5), u64::MAX, (GoodId(4), 1), 7, false),
        (GoodId(1), u64::MAX, (GoodId(999), 0), 100, false),
    ] {
        let recipes = [(
            GoodId(2),
            vec![(
                "use-stock".into(),
                game_workshop::Recipe {
                    work: None,
                    inputs: [(input, held)].into(),
                    output: GoodId(2),
                    quantity: 1,
                    duration: MINUTE,
                    skill: "craft".into(),
                },
            )],
        )]
        .into();
        let possible = [input, GoodId(2)].into();
        let stock = [(input, held), ballast].into();
        let mut search = Search {
            recipes: &recipes,
            patches: &[],
            stock,
            steps: VecDeque::new(),
            gathered: BTreeMap::new(),
            cost: 0,
            objects: &objects,
            capacity,
            possible: &possible,
            depth: 8,
            steps_limit: 32,
        };
        let mut old = reference::Search {
            recipes: &recipes,
            patches: &[],
            stock: search.stock.clone(),
            steps: VecDeque::new(),
            gathered: BTreeMap::new(),
            cost: 0,
            objects: &objects,
            capacity,
            possible: &possible,
            depth: 8,
            steps_limit: 32,
        };
        assert_eq!(
            search.supply(GoodId(2), 1, &mut vec![], &mut 100).is_some(),
            accepted
        );
        assert_eq!(
            old.supply(GoodId(2), 1, &mut vec![], &mut 100).is_some(),
            accepted
        );
        assert_eq!(search.stock, old.stock);
        assert_eq!(
            serde_json::to_value(search.steps).unwrap(),
            serde_json::to_value(old.steps).unwrap()
        );
    }
}

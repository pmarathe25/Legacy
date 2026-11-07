use game_production_model::RecipeCategory;
use game_skill_catalog::configure;
use game_skills::{CatalogTraining, Rule, Skills};

fn skills() -> Skills {
    Skills {
        rules: vec![Rule {
            recipe: 7,
            completions_per_level: 1,
            max_level: 4,
            work_per_level: 2,
            practice_each_tick: false,
        }],
        aliases: vec![],
        catalog_training: Some(CatalogTraining {
            completions_per_level: 2,
            max_level: 3,
            work_per_level: 1,
            practice_each_tick: true,
        }),
        records: vec![],
        events: vec![],
        lessons: vec![],
        teachers: vec![],
        inherit_teachers: false,
    }
}

#[test]
fn authored_categories_form_stable_groups_and_preserve_explicit_rules() {
    let mut state = skills();
    let rows = vec![
        RecipeCategory {
            recipe: 9,
            category: "cooking".into(),
        },
        RecipeCategory {
            recipe: 8,
            category: "wood".into(),
        },
        RecipeCategory {
            recipe: 7,
            category: "wood".into(),
        },
    ];
    configure(&mut state, &rows).unwrap();
    assert_eq!(state.canonical(8), 7);
    assert_eq!(state.canonical(9), 9);
    assert!(
        !state
            .rules
            .iter()
            .find(|rule| rule.recipe == 7)
            .unwrap()
            .practice_each_tick
    );
    assert!(
        state
            .rules
            .iter()
            .find(|rule| rule.recipe == 9)
            .unwrap()
            .practice_each_tick
    );
    assert_eq!(
        state
            .rules
            .iter()
            .find(|rule| rule.recipe == 7)
            .unwrap()
            .work_per_level,
        2
    );
    assert_eq!(
        state
            .rules
            .iter()
            .find(|rule| rule.recipe == 9)
            .unwrap()
            .completions_per_level,
        2
    );
    let saved = state.clone();
    configure(&mut state, &rows).unwrap();
    assert_eq!(state, saved);
    let mut duplicate = rows.clone();
    duplicate.push(rows[0].clone());
    assert!(configure(&mut state, &duplicate).is_err());
    assert_eq!(state, saved);
}

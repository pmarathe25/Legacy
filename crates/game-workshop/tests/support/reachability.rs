use game_core::*;
use game_workshop::*;
fn recipe(input: GoodId, output: GoodId) -> Recipe {
    Recipe {
        work: None,
        inputs: [(input, 1)].into(),
        output,
        quantity: 1,
        duration: MINUTE,
        skill: "work".into(),
    }
}
#[test]
fn cached_ingredients_cannot_leak_across_inputs_definitions_or_world_snapshots() {
    let mut workshop = Workshop::default();
    workshop
        .recipes
        .insert("cycle-a".into(), recipe(GoodId(3), GoodId(4)));
    workshop
        .recipes
        .insert("cycle-b".into(), recipe(GoodId(4), GoodId(3)));
    assert!(workshop.reachable([]).is_empty());
    workshop
        .recipes
        .insert("source".into(), recipe(GoodId(2), GoodId(3)));
    assert!(workshop.reachable([GoodId(2)]).contains(&GoodId(4)));
    assert!(!workshop.reachable([GoodId(5)]).contains(&GoodId(4)));
    let old = workshop.clone();
    workshop
        .recipes
        .insert("source".into(), recipe(GoodId(2), GoodId(6)));
    assert!(workshop.reachable([GoodId(2)]).contains(&GoodId(6)));
    assert!(!workshop.reachable([GoodId(2)]).contains(&GoodId(4)));
    assert!(old.reachable([GoodId(2)]).contains(&GoodId(4)));
    assert!(
        !old.choices()
            .values()
            .flatten()
            .any(|(_, r)| r.output == GoodId(6))
    );
}

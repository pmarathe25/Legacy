use game_parallel::{GoodsDirection, GoodsTransfer, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap()
}
#[test]
fn witnessed_theft_moves_real_goods_without_transferring_title_and_changes_only_witness_trust() {
    let mut kernel = Kernel::compile(world()).unwrap();
    let normal = GoodsTransfer {
        person: 1,
        good: 1,
        quantity: 2,
        direction: GoodsDirection::Pickup,
    };
    assert!(kernel.transfer_goods(&[normal]).is_err());
    let draft = kernel.prepare().unwrap();
    let theft = kernel.steal_goods(1, 1, 2).unwrap();
    assert!(theft.witnessed);
    assert_eq!(theft.owner, 2);
    assert!(kernel.commit(draft).is_err());
    let result = kernel.export();
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        7
    );
    assert_eq!(
        result.goods_cargo.as_ref().unwrap().holdings[0].items[0].quantity,
        2
    );
    assert_eq!(result.property.as_ref().unwrap().claims[0].owner, 2);
    assert_eq!(result.property.as_ref().unwrap().thefts, vec![theft]);
    assert_eq!(result.opinions.as_ref().unwrap().index().trust(2, 1), -2);
    assert_eq!(result.opinions.as_ref().unwrap().index().trust(1, 2), 0);
    assert_eq!(
        game_parallel_observation::ruler(&result)
            .unwrap()
            .thefts
            .len(),
        1
    );
    let saved = serde_json::to_string(&result).unwrap();
    assert_eq!(
        Kernel::compile(World::from_json(&saved).unwrap())
            .unwrap()
            .export(),
        result
    );
}
#[test]
fn an_absent_owner_does_not_automatically_witness_or_learn_of_the_taking() {
    let mut state = world();
    let index = state.regions[0]
        .people
        .iter()
        .position(|person| person.id == 2)
        .unwrap();
    let owner = state.regions[0].people.remove(index);
    state.regions[1].people.push(owner);
    let mut kernel = Kernel::compile(state).unwrap();
    assert!(!kernel.steal_goods(1, 1, 2).unwrap().witnessed);
    let mut result = kernel.export();
    assert!(result.opinions.as_ref().unwrap().records.is_empty());
    assert_eq!(
        game_parallel_observation::ruler(&result)
            .unwrap()
            .thefts
            .len(),
        1
    );
    result.lineage.as_mut().unwrap().current = Some(2);
    assert!(
        game_parallel_observation::ruler(&result)
            .unwrap()
            .thefts
            .is_empty()
    );
}
#[test]
fn invalid_or_authorized_takings_leave_state_and_prepared_generation_intact() {
    let mut kernel = Kernel::compile(world()).unwrap();
    let before = kernel.export();
    let draft = kernel.prepare().unwrap();
    assert!(kernel.steal_goods(1, 1, 1000).is_err());
    assert!(kernel.steal_goods(1, 1, 0).is_err());
    assert!(kernel.steal_goods(2, 1, 1).is_err());
    assert_eq!(kernel.export(), before);
    kernel.commit(draft).unwrap();
}

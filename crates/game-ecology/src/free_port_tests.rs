use super::*;
use game_ecs::{Schema, StableId, Store};
#[test]
fn independent_personal_ports_keep_rows_separate_and_reject_invalid_inserts() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let actor = PersonId(2);
    ensure_knowledge(&mut store, actor)
        .unwrap()
        .observed_areas
        .insert(AreaId(1), SimTime(50));
    ensure_requests(&mut store, actor)
        .unwrap()
        .next_request
        .insert(ResourceId(10), SimTime(500));
    let row = Gathering {
        resource: ResourceId(10),
        destination: InventoryId(20),
        quantity: Quantity(1),
        tool: None,
        completes: SimTime(500),
        unauthorized: true,
    };
    insert_gathering(&mut store, actor, row.clone()).unwrap();
    gathering_mut(&mut store, actor).unwrap().quantity = Quantity(2);
    assert_eq!(gatherings(&store).next().unwrap().0, actor);
    assert_eq!(
        knowledge(&store, actor).unwrap().observed_areas[&AreaId(1)],
        SimTime(50)
    );
    assert_eq!(
        requests(&store, actor).unwrap().next_request[&ResourceId(10)],
        SimTime(500)
    );
    assert_eq!(all_knowledge(&store).count(), 1);
    assert_eq!(all_requests(&store).count(), 1);
    let mut invalid = row;
    invalid.resource = ResourceId(0);
    let before = store.snapshot().unwrap();
    assert!(insert_gathering(&mut store, PersonId(99), invalid).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(!store.contains(StableId(99)));
    assert_eq!(
        remove_gathering(&mut store, actor)
            .unwrap()
            .unwrap()
            .quantity,
        Quantity(2)
    );
    assert!(knowledge(&store, actor).is_some());
    assert!(requests(&store, actor).is_some());
    remove_knowledge(&mut store, actor).unwrap();
    remove_requests(&mut store, actor).unwrap();
    assert!(
        remove_gathering(&mut store, PersonId(999))
            .unwrap()
            .is_none()
    );
    assert!(
        remove_knowledge(&mut store, PersonId(999))
            .unwrap()
            .is_none()
    );
    assert!(
        remove_requests(&mut store, PersonId(999))
            .unwrap()
            .is_none()
    );
}

use super::*;

fn fixture(count: u64) -> (State, Vec<SimpleTransfer>) {
    let mut containers: Vec<_> = (1..=2)
        .map(|id| {
            Container::new(
                Place::Inventory(id),
                count,
                vec![Lot {
                    good: 1,
                    quantity: count / 4,
                    expires_at: None,
                }],
            )
            .unwrap()
        })
        .collect();
    containers.extend((1..=count).map(|id| Container::new(Place::Person(id), 1, vec![]).unwrap()));
    containers.sort_unstable_by_key(|row| row.place);
    let rows = (1..=count)
        .map(|id| SimpleTransfer {
            action: id,
            movement: Movement {
                id,
                from: Place::Inventory(id % 2 + 1),
                to: Some(Place::Person(id)),
                good: 1,
                quantity: 1,
            },
        })
        .collect();
    (
        State {
            tick: 0,
            revision: 0,
            containers,
        },
        rows,
    )
}

#[test]
fn grouped_sources_reject_unsupported_overlaps_and_unbounded_groups() {
    let (state, rows) = fixture(512);
    assert!(prepare(&state, 1, 17, &rows, true).unwrap().is_some());
    let mut duplicate_destination = rows.clone();
    duplicate_destination[5].movement.to = duplicate_destination[4].movement.to;
    assert!(
        prepare(&state, 1, 17, &duplicate_destination, true)
            .unwrap()
            .is_none()
    );
    let mut overlapping_source = rows.clone();
    overlapping_source[5].movement.to = Some(Place::Inventory(1));
    assert!(
        prepare(&state, 1, 17, &overlapping_source, true)
            .unwrap()
            .is_none()
    );
    let mut mixed_good = rows.clone();
    mixed_good[5].movement.good = 2;
    assert!(prepare(&state, 1, 17, &mixed_good, true).unwrap().is_none());
    let mut duplicate_id = rows.clone();
    duplicate_id[5].movement.id = duplicate_id[4].movement.id;
    assert!(
        prepare(&state, 1, 17, &duplicate_id, true)
            .unwrap()
            .is_none()
    );
    let mut missing = rows;
    missing[5].movement.to = Some(Place::Person(9999));
    assert!(prepare(&state, 1, 17, &missing, true).unwrap().is_none());
    let (state, oversized) = fixture(4098);
    assert!(prepare(&state, 1, 17, &oversized, true).unwrap().is_none());
}

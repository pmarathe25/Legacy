use super::Route;
use super::routing::Routing;
#[test]
fn next_leg_reports_the_selected_weighted_edge_and_stable_ties() {
    let routes = [
        Route {
            source: 1,
            destination: 2,
            ticks: 8,
        },
        Route {
            source: 1,
            destination: 3,
            ticks: 2,
        },
        Route {
            source: 3,
            destination: 4,
            ticks: 3,
        },
        Route {
            source: 1,
            destination: 2,
            ticks: 3,
        },
        Route {
            source: 2,
            destination: 4,
            ticks: 2,
        },
        Route {
            source: 5,
            destination: 1,
            ticks: 1,
        },
    ];
    let table = Routing::new(&routes, &[4]);
    assert_eq!(table.next_leg(1, 4), Some((2, 3)));
    assert_eq!(table.next(1, 4), Some(2));
    assert_eq!(table.next_leg(5, 4), Some((1, 1)));
    assert_eq!(table.next_leg(4, 4), None);
    assert_eq!(table.next_leg(4, 1), None);
}

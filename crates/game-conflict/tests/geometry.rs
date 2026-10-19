use game_conflict::{Map, Point};
#[test]
fn geometry_is_seeded_and_blocks_continuous_segments() {
    assert_eq!(
        Map::generate(8, 1000, 4).unwrap(),
        Map::generate(8, 1000, 4).unwrap()
    );
    let m = Map {
        width: 100,
        obstacles: vec![(Point { x: 50, y: 50 }, 10)],
    };
    assert!(!m.clear(Point { x: 0, y: 50 }, Point { x: 100, y: 50 }));
    assert!(m.clear(Point { x: 0, y: 0 }, Point { x: 100, y: 0 }));
}

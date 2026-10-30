use super::*;
fn rules() -> Rules {
    Rules {
        width: 100,
        height: 100,
        obstacle_count: 0,
        max_obstacle_size: 10,
        movement: 20,
        attack_range: 30,
    }
}
#[test]
fn generation_and_save_are_stable_and_damage_is_not_rerolled() {
    let mut config = rules();
    config.obstacle_count = 20;
    let first = Map::generate(3, 42, config.clone()).unwrap();
    assert_eq!(first, Map::generate(3, 42, config).unwrap());
    let mut damaged = first.clone();
    damaged.obstacles.remove(0);
    let text = serde_json::to_string(&damaged).unwrap();
    let restored: Map = serde_json::from_str(&text).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, damaged);
    assert_ne!(restored, first);
}
#[test]
fn movement_and_attacks_respect_obstacles_range_and_map_limits() {
    let mut map = Map::generate(1, 1, rules()).unwrap();
    map.obstacles.push(Obstacle {
        min: Point { x: 40, y: 40 },
        max: Point { x: 60, y: 60 },
    });
    assert!(map.can_move(Point { x: 10, y: 10 }, Point { x: 22, y: 26 }));
    assert!(!map.can_move(Point { x: 10, y: 10 }, Point { x: 23, y: 26 }));
    assert!(!map.can_attack(Point { x: 35, y: 50 }, Point { x: 65, y: 50 }));
    assert!(!map.visible(Point { x: 39, y: 40 }, Point { x: 61, y: 40 }));
    assert!(map.visible(Point { x: 39, y: 39 }, Point { x: 61, y: 39 }));
    assert!(!map.open(Point { x: 101, y: 0 }));
}
#[test]
fn geometry_does_not_overflow_at_maximum_coordinates() {
    let mut config = rules();
    config.width = u32::MAX;
    config.height = u32::MAX;
    config.movement = u32::MAX;
    config.attack_range = u32::MAX;
    let map = Map::generate(1, 1, config).unwrap();
    assert!(!map.can_move(
        Point { x: 0, y: 0 },
        Point {
            x: u32::MAX,
            y: u32::MAX
        }
    ));
    assert!(map.can_attack(Point { x: 0, y: 0 }, Point { x: u32::MAX, y: 0 }));
}

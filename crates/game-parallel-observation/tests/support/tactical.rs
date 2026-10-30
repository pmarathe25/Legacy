use super::*;
use game_parallel::{TacticalBattle, TacticalObstacle, TacticalPlacement};
#[test]
fn tactical_view_hides_occluded_and_remote_positions() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/tactics.toml")).unwrap();
    let tactics = world.tactics.as_mut().unwrap();
    tactics.maps[0].rules.attack_range = 20;
    tactics.maps[0].obstacles.push(TacticalObstacle {
        min: TacticalPoint { x: 5, y: 0 },
        max: TacticalPoint { x: 6, y: 3 },
    });
    tactics.battles = vec![
        TacticalBattle {
            encounter: 1,
            region: 1,
            people: [(1, 1), (2, 3), (3, 10)]
                .into_iter()
                .map(|(person, x)| TacticalPlacement {
                    person,
                    position: TacticalPoint { x, y: 1 },
                    last_move: None,
                })
                .collect(),
        },
        TacticalBattle {
            encounter: 2,
            region: 1,
            people: vec![TacticalPlacement {
                person: 4,
                position: TacticalPoint { x: 2, y: 1 },
                last_move: None,
            }],
        },
    ];
    world.lineage = World::from_toml(include_str!("../../../../content/parallel/guardians.toml"))
        .unwrap()
        .lineage;
    let person = world.regions[1].people.remove(0);
    world.regions[0].people.push(person);
    let full = crate::ruler(&world).unwrap();
    assert_eq!(
        full.location,
        crate::Location::Local {
            region: 1,
            food: 10,
            people: vec![1, 2]
        }
    );
    let observed = view(&world, 1).unwrap();
    assert_eq!(observed.position, TacticalPoint { x: 1, y: 1 });
    assert_eq!(observed.nearby, vec![(2, TacticalPoint { x: 3, y: 1 })]);
}

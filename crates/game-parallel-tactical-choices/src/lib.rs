//! Parallel individual pursuit and deterministic destination arbitration.
use game_parallel_conflict::{Conflict, Order};
use game_parallel_economy_person::Person;
use game_parallel_tactics::{Move, Point, Tactics};
use rayon::prelude::*;
use std::collections::HashMap;
mod path;

#[derive(Clone, Copy)]
struct Position {
    person: u64,
    encounter: u64,
    map: usize,
    point: Point,
    last_move: Option<u64>,
}
struct Candidate {
    encounter: u64,
    person: u64,
    destination: Point,
    priority: u64,
}
fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
fn axis(from: u32, to: u32, amount: u32) -> u32 {
    if from < to {
        from.saturating_add(amount.min(to - from))
    } else {
        from.saturating_sub(amount.min(from - to))
    }
}
fn distance(a: Point, b: Point) -> u128 {
    u128::from(a.x.abs_diff(b.x)).pow(2) + u128::from(a.y.abs_diff(b.y)).pow(2)
}
/// Resolve choices against positions at the beginning of a shared round.
pub fn prepare(
    conflict: &Conflict,
    tactics: &Tactics,
    people: &[Person],
    pursuers: &[u64],
    seed: u64,
    now: u64,
) -> Vec<Move> {
    if pursuers.is_empty() {
        return vec![];
    }
    let maps: HashMap<_, _> = tactics
        .maps
        .iter()
        .enumerate()
        .map(|(i, m)| (m.region, i))
        .collect();
    let mut positions: Vec<_> = tactics
        .battles
        .par_iter()
        .flat_map(|battle| {
            let map = maps[&battle.region];
            battle.people.par_iter().map(move |p| Position {
                person: p.person,
                encounter: battle.encounter,
                map,
                point: p.position,
                last_move: p.last_move,
            })
        })
        .collect();
    if positions.par_windows(2).any(|p| p[0].person > p[1].person) {
        positions.par_sort_unstable_by_key(|p| p.person);
    }
    let mut occupied: Vec<_> = positions
        .par_iter()
        .map(|p| (p.encounter, p.point.x, p.point.y))
        .collect();
    if occupied.par_windows(2).any(|p| p[0] > p[1]) {
        occupied.par_sort_unstable();
    }
    let dense_positions = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((positions.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    let dense_people = people.first().and_then(|first| {
        people.last().and_then(|last| {
            (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
        })
    });
    let dense_pursuers = pursuers.first().and_then(|first| {
        pursuers.last().and_then(|last| {
            (last.checked_sub(*first) == Some((pursuers.len() - 1) as u64)).then_some(*first)
        })
    });
    let find = |id: u64| {
        dense_positions
            .and_then(|first| id.checked_sub(first))
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| i < positions.len())
            .or_else(|| {
                dense_positions
                    .is_none()
                    .then(|| positions.binary_search_by_key(&id, |p| p.person).ok())
                    .flatten()
            })
            .map(|i| positions[i])
    };
    let person = |id: u64| {
        dense_people
            .and_then(|first| id.checked_sub(first))
            .and_then(|i| usize::try_from(i).ok())
            .filter(|&i| i < people.len())
            .or_else(|| {
                dense_people
                    .is_none()
                    .then(|| people.binary_search_by_key(&id, |p| p.id).ok())
                    .flatten()
            })
            .map(|i| &people[i])
    };
    let occupied = &occupied;
    let find = &find;
    let person = &person;
    let mut choices: Vec<_> = conflict
        .encounters
        .par_iter()
        .flat_map(|fight| {
            fight.fighters.par_iter().filter_map(move |fighter| {
                let is_pursuer = dense_pursuers.map_or_else(
                    || pursuers.binary_search(&fighter.person).is_ok(),
                    |first| {
                        fighter.person >= first && fighter.person - first < pursuers.len() as u64
                    },
                );
                if !is_pursuer {
                    return None;
                }
                let actor = person(fighter.person)?;
                let Order::Attack { target } = fighter.order else {
                    return None;
                };
                let from = find(fighter.person)?;
                let to = find(target)?;
                if actor.controlled
                    || actor.health == 0
                    || from.last_move == Some(now)
                    || from.encounter != to.encounter
                {
                    return None;
                }
                let target_alive = person(target).is_some_and(|p| p.health > 0);
                let map = &tactics.maps[from.map];
                if !target_alive || map.can_attack(from.point, to.point) {
                    return None;
                }
                let step = map.rules.movement;
                let waypoint = path::waypoint(map, from.point, to.point).unwrap_or(to.point);
                let points = [
                    Point {
                        x: axis(from.point.x, waypoint.x, step),
                        y: from.point.y,
                    },
                    Point {
                        x: from.point.x,
                        y: axis(from.point.y, waypoint.y, step),
                    },
                    Point {
                        x: axis(from.point.x, waypoint.x, 1),
                        y: from.point.y,
                    },
                    Point {
                        x: from.point.x,
                        y: axis(from.point.y, waypoint.y, 1),
                    },
                ];
                let destination = points
                    .into_iter()
                    .filter(|point| {
                        *point != from.point
                            && distance(*point, waypoint) < distance(from.point, waypoint)
                            && occupied
                                .binary_search(&(fight.id, point.x, point.y))
                                .is_err()
                            && map.can_move(from.point, *point)
                    })
                    .min_by_key(|p| (distance(*p, waypoint), p.x, p.y))?;
                Some(Candidate {
                    encounter: fight.id,
                    person: fighter.person,
                    destination,
                    priority: mix(seed
                        ^ now.rotate_left(19)
                        ^ fighter.person
                        ^ fight.id.rotate_left(37)),
                })
            })
        })
        .collect();
    let key = |c: &Candidate| {
        (
            c.encounter,
            c.destination.x,
            c.destination.y,
            c.priority,
            c.person,
        )
    };
    if choices.par_windows(2).any(|p| key(&p[0]) > key(&p[1])) {
        choices.par_sort_unstable_by_key(key);
    }
    let mut selected: Vec<_> = choices
        .par_chunk_by(|a, b| {
            (a.encounter, a.destination.x, a.destination.y)
                == (b.encounter, b.destination.x, b.destination.y)
        })
        .map(|group| Move {
            encounter: group[0].encounter,
            person: group[0].person,
            destination: group[0].destination,
        })
        .collect();
    if selected.par_windows(2).any(|p| p[0].person > p[1].person) {
        selected.par_sort_unstable_by_key(|m| m.person);
    }
    selected
}
pub fn apply(tactics: &mut Tactics, moves: &[Move], now: u64) {
    if moves.is_empty() {
        return;
    }
    let dense = moves.first().and_then(|first| {
        moves.last().and_then(|last| {
            (last.person.checked_sub(first.person) == Some((moves.len() - 1) as u64))
                .then_some(first.person)
        })
    });
    tactics.battles.par_iter_mut().for_each(|battle| {
        battle.people.par_iter_mut().for_each(|person| {
            let index = dense
                .and_then(|first| person.person.checked_sub(first))
                .and_then(|i| usize::try_from(i).ok())
                .filter(|&i| i < moves.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| {
                            moves
                                .binary_search_by_key(&person.person, |m| m.person)
                                .ok()
                        })
                        .flatten()
                });
            if let Some(i) = index {
                person.position = moves[i].destination;
                person.last_move = Some(now);
            }
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_conflict::{Encounter, Fighter};
    use game_parallel_tactics::{Battle, Map, Obstacle, Placement, Rules};
    #[test]
    fn contested_destination_has_one_deterministic_winner() {
        let rules = Rules {
            width: 10,
            height: 10,
            obstacle_count: 1,
            max_obstacle_size: 5,
            movement: 5,
            attack_range: 1,
        };
        let map = Map {
            region: 1,
            seed: 1,
            version: 1,
            rules,
            obstacles: vec![Obstacle {
                min: Point { x: 1, y: 0 },
                max: Point { x: 4, y: 2 },
            }],
        };
        let positions = [
            (1, Point { x: 5, y: 1 }),
            (2, Point { x: 0, y: 0 }),
            (3, Point { x: 0, y: 2 }),
        ];
        let tactics = Tactics {
            maps: vec![map],
            battles: vec![Battle {
                encounter: 1,
                region: 1,
                people: positions
                    .iter()
                    .map(|&(person, position)| Placement {
                        person,
                        position,
                        last_move: None,
                    })
                    .collect(),
            }],
        };
        let conflict = Conflict {
            damage: 1,
            next_id: 2,
            encounters: vec![Encounter {
                id: 1,
                region: 1,
                fighters: vec![
                    Fighter {
                        person: 1,
                        side: 1,
                        order: Order::Defend,
                    },
                    Fighter {
                        person: 2,
                        side: 2,
                        order: Order::Attack { target: 1 },
                    },
                    Fighter {
                        person: 3,
                        side: 2,
                        order: Order::Attack { target: 1 },
                    },
                ],
            }],
            events: vec![],
        };
        let people: Vec<_> = (1..=3)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: 0,
                willing_to_work: true,
                seek_meals: false,
                controlled: false,
            })
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| prepare(&conflict, &tactics, &people, &[2, 3], 7, 0))
        };
        let one = run(1);
        let four = run(4);
        assert_eq!(one.len(), 1);
        assert_eq!(
            (one[0].person, one[0].destination),
            (four[0].person, four[0].destination)
        );
        assert_eq!(one[0].destination, Point { x: 0, y: 3 });
        let mut next = tactics;
        apply(&mut next, &one, 0);
        next.validate(0).unwrap();
    }

    #[test]
    fn pursuit_goes_around_a_wall_even_when_it_must_first_move_away() {
        let map = Map {
            region: 1,
            seed: 1,
            version: 1,
            rules: Rules {
                width: 20,
                height: 20,
                obstacle_count: 1,
                max_obstacle_size: 10,
                movement: 2,
                attack_range: 2,
            },
            obstacles: vec![Obstacle {
                min: Point { x: 5, y: 4 },
                max: Point { x: 7, y: 12 },
            }],
        };
        let original = Tactics {
            maps: vec![map],
            battles: vec![Battle {
                encounter: 1,
                region: 1,
                people: vec![
                    Placement {
                        person: 1,
                        position: Point { x: 3, y: 8 },
                        last_move: None,
                    },
                    Placement {
                        person: 2,
                        position: Point { x: 9, y: 8 },
                        last_move: None,
                    },
                ],
            }],
        };
        let conflict = Conflict {
            damage: 1,
            next_id: 2,
            events: vec![],
            encounters: vec![Encounter {
                id: 1,
                region: 1,
                fighters: vec![
                    Fighter {
                        person: 1,
                        side: 1,
                        order: Order::Attack { target: 2 },
                    },
                    Fighter {
                        person: 2,
                        side: 2,
                        order: Order::Defend,
                    },
                ],
            }],
        };
        let people: Vec<_> = (1..=2)
            .map(|id| Person {
                id,
                region: Some(1),
                health: 100,
                hunger: 0,
                willing_to_work: false,
                seek_meals: false,
                controlled: false,
            })
            .collect();
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut tactics = original.clone();
                    for tick in 0..30 {
                        if tactics.can_attack(1, 1, 2) {
                            return tactics;
                        }
                        let moves = prepare(&conflict, &tactics, &people, &[1], 7, tick);
                        assert_eq!(moves.len(), 1, "pursuit stalled on tick {tick}");
                        let old = tactics.battles[0].people[0].position;
                        assert!(tactics.maps[0].can_move(old, moves[0].destination));
                        apply(&mut tactics, &moves, tick);
                    }
                    panic!("pursuit did not reach the target");
                })
        };
        assert_eq!(run(1), run(4));
    }
}

//! Measure actual personal-scene observation across worker counts.
use game_core::*;
use game_sim::*;
use std::time::Instant;

fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    b
}

fn main() {
    let people_count = game_performance::setting("LEGACY_POPULATION", 1000usize);
    let patches = game_performance::setting("LEGACY_LOCAL_PATCHES", 64usize);
    let rounds = game_performance::setting("LEGACY_ROUNDS", 3usize);
    assert!(people_count > 0 && patches > 0 && rounds > 0);
    game_performance::metrics::metadata();
    let mut host = builder().build(7, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    host.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "cohort".into(),
        })
        .unwrap();
    for _ in 0..patches {
        host.dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            capacity: 100,
            stock: 100,
            renew: 0,
            good: game_cargo::FOOD,
        }))
        .unwrap();
    }
    let mut people = Vec::with_capacity(people_count);
    for _ in 0..people_count {
        let person = host
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        host.dispatch(game_transport::Place { person, area })
            .unwrap();
        people.push(person);
    }
    let before = host.save().unwrap();
    let mut expected = None;
    for round in 0..rounds {
        let workers = if round % 2 == 0 {
            [1, 4, 12]
        } else {
            [12, 4, 1]
        };
        for count in workers {
            let mut replay = builder().load(&before, count).unwrap();
            let start = Instant::now();
            replay
                .dispatch(game_knowledge::ObserveFrame {
                    people: people.clone(),
                })
                .unwrap();
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let save = replay.save().unwrap();
            if let Some(reference) = &expected {
                assert_eq!(&save, reference, "worker count changed observation state");
            } else {
                expected = Some(save);
            }
            println!(
                "round={round} people={people_count} patches={patches} workers={count} observation_ms={elapsed:.3}"
            );
        }
    }
}

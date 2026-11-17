use game_economy_person::{HungerRate, Person, Rules, StarvationRate};

fn person(id: u64) -> Person {
    Person {
        id,
        region: Some(1),
        health: 10,
        hunger: 2,
        hunger_remainder: 0,
        starvation_remainder: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    }
}

fn rules() -> Rules {
    Rules {
        hunger_per_tick: 0,
        hunger_rate: Some(HungerRate {
            units: 0,
            per_millis: 1000,
        }),
        meal_threshold: 1,
        starvation_threshold: 2,
        starvation_damage: 0,
        starvation_rate: Some(StarvationRate {
            units: 1,
            per_millis: 1000,
        }),
        max_health: 10,
        carrying_capacity: 1,
        max_work_hunger: 10,
    }
}

#[test]
fn feeding_pauses_damage_without_losing_fractional_exposure() {
    let mut people = vec![person(1)];
    game_economy_person::progress_vitals(&mut people, &rules(), 500, 1000).unwrap();
    assert_eq!(
        (people[0].health, people[0].starvation_remainder),
        (10, 500)
    );
    people[0].hunger = 0;
    game_economy_person::progress_vitals(&mut people, &rules(), 1000, 1000).unwrap();
    assert_eq!(
        (people[0].health, people[0].starvation_remainder),
        (10, 500)
    );
    people[0].hunger = 2;
    game_economy_person::progress_vitals(&mut people, &rules(), 500, 1000).unwrap();
    assert_eq!((people[0].health, people[0].starvation_remainder), (9, 0));
}

#[test]
fn invalid_vitals_leave_the_entire_batch_unchanged() {
    let mut people = vec![person(1), person(2)];
    people[1].starvation_remainder = 1000;
    let before = people.clone();
    assert!(game_economy_person::progress_vitals(&mut people, &rules(), 500, 1000).is_err());
    assert_eq!(people, before);
}

#[test]
fn simultaneous_death_prediction_is_deterministic_across_workers() {
    let people: Vec<_> = (1..=4096)
        .map(|id| {
            let mut person = person(id);
            person.health = if id % 2 == 0 { 1 } else { 2 };
            person
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                game_economy_person::starvation_deaths_within(&people, &rules(), 1000, 1000)
                    .unwrap()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.len(), 2048);
    assert!(result.iter().all(|id| id % 2 == 0));
}

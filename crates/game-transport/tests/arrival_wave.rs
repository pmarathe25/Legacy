use game_core::*;
use game_sim::*;
use game_transport::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Seen(Vec<PersonId>);
impl Resource for Seen {
    const KEY: &'static str = "test.arrivals";
}
#[derive(Clone, Serialize, Deserialize)]
struct Seed;
#[derive(Clone, Serialize, Deserialize)]
struct Stagger;
impl Request for Stagger {
    const KEY: &'static str = "test.stagger";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Depart {
    person: PersonId,
    destination: AreaId,
}
impl Request for Depart {
    const KEY: &'static str = "test.depart";
    type Response = ();
}
impl Request for Seed {
    const KEY: &'static str = "test.seed";
    type Response = ();
}
fn builder(fail: bool, restart: bool) -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.resource::<Seen>().unwrap();
    b.request::<Depart>(|tx, r| {
        tx.call(Move {
            person: r.person,
            destination: r.destination,
        })?;
        Ok(())
    })
    .unwrap();
    b.request::<Stagger>(|tx, _| {
        let from = tx.call(game_land::CreateArea {
            name: "from".into(),
        })?;
        let to = tx.call(game_land::CreateArea { name: "to".into() })?;
        tx.call(game_land::Link {
            from,
            to,
            duration: SimDuration(10),
        })?;
        for index in 0..4096 {
            let person = PersonId(tx.allocate()?);
            tx.call(Place { person, area: from })?;
            tx.schedule(
                SimTime(index % 500),
                5,
                Depart {
                    person,
                    destination: to,
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Seed>(|tx, _| {
        let from = tx.call(game_land::CreateArea {
            name: "from".into(),
        })?;
        let to = tx.call(game_land::CreateArea { name: "to".into() })?;
        tx.call(game_land::Link {
            from,
            to,
            duration: SimDuration(10),
        })?;
        for _ in 0..4096 {
            let person = PersonId(tx.allocate()?);
            tx.call(Place { person, area: from })?;
            tx.call(Move {
                person,
                destination: to,
            })?;
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<ActionFinished>("test.finished", move |tx, _| {
        require(
            restart
                || tx
                    .get::<Positions>()?
                    .0
                    .values()
                    .all(|p| matches!(p, Location::At(_))),
            "partial arrival visibility",
        )
    })
    .unwrap();
    b.subscribe::<Arrived>("test.arrival", move |tx, e| {
        require(
            restart
                || tx
                    .get::<Positions>()?
                    .0
                    .values()
                    .all(|p| *p == Location::At(e.area)),
            "partial arrival visibility",
        )?;
        tx.edit::<Seen>()?.0.push(e.person);
        require(
            !fail || tx.get::<Seen>()?.0.len() < 4096,
            "last arrival rejected",
        )
    })
    .unwrap();
    if restart {
        b.subscribe::<Arrived>("test.restart", |tx, _| {
            if tx.get::<Seen>()?.0.len() == 1 {
                let person = *tx.get::<Positions>()?.0.keys().nth(1).unwrap();
                let destination = *tx.get::<game_land::Land>()?.0.keys().next().unwrap();
                tx.call(Cancel { person })?;
                tx.call(Move {
                    person,
                    destination,
                })?;
            }
            Ok(())
        })
        .unwrap();
    }
    b
}
#[test]
fn window_collects_staggered_departures_into_one_saved_completion_phase() {
    let mut initial = builder(false, false).build(7, 1).unwrap();
    initial
        .dispatch(ConfigureArrivalWindow(SimDuration(1000)))
        .unwrap();
    initial.dispatch(Stagger).unwrap();
    while initial.advance(SimTime(999), 128).unwrap().stop == Stop::Budget {}
    assert_eq!(initial.scheduled().count(), 4096);
    assert!(
        initial
            .scheduled()
            .all(|job| job.at == SimTime(1000) && job.priority == 9)
    );
    let saved = initial.save().unwrap();
    let mut results = vec![];
    for workers in [1, 4] {
        let mut h = builder(false, false).load(&saved, workers).unwrap();
        h.advance(SimTime(1000), 1).unwrap();
        assert_eq!(h.get::<Seen>().unwrap().0.len(), 4096);
        assert_eq!(h.scheduled().count(), 0);
        results.push(h.save().unwrap());
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn competing_listener_cannot_finish_a_replacement_journey() {
    let mut worlds = vec![];
    for workers in [1, 4] {
        let mut h = builder(false, true).build(7, workers).unwrap();
        h.dispatch(Seed).unwrap();
        h.advance(SimTime(10), 1).unwrap();
        assert_eq!(h.get::<Actions>().unwrap().0.len(), 1);
        assert_eq!(h.scheduled().count(), 1);
        assert_eq!(
            h.get::<Positions>()
                .unwrap()
                .0
                .values()
                .filter(|p| matches!(p, Location::Transit { .. }))
                .count(),
            1
        );
        worlds.push(h.save().unwrap());
    }
    assert_eq!(worlds[0], worlds[1]);
}
#[test]
fn arrival_position_barrier_is_worker_deterministic_and_preserves_all_listeners() {
    let mut initial = builder(false, false).build(7, 1).unwrap();
    initial.dispatch(Seed).unwrap();
    let saved = initial.save().unwrap();
    let mut results = vec![];
    for workers in [1, 4] {
        let mut h = builder(false, false).load(&saved, workers).unwrap();
        h.advance(SimTime(10), 1).unwrap();
        assert_eq!(h.get::<Seen>().unwrap().0.len(), 4096);
        assert!(h.get::<Seen>().unwrap().0.windows(2).all(|w| w[0] < w[1]));
        assert!(h.get::<Actions>().unwrap().0.is_empty());
        assert_eq!(h.scheduled().count(), 0);
        results.push(h.save().unwrap());
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn final_arrival_listener_failure_restores_positions_actions_jobs_and_notices() {
    let mut h = builder(true, false).build(7, 4).unwrap();
    h.dispatch(Seed).unwrap();
    let saved = h.save().unwrap();
    assert!(h.advance(SimTime(10), 1).is_err());
    assert_eq!(saved, h.save().unwrap());
}

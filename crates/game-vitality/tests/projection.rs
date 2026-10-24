use game_core::*;
use game_sim::*;
use game_vitality::*;
use serde::{Deserialize, Serialize};
const ABILITIES: &[&str] = &[
    "intelligence",
    "attention",
    "learning",
    "foresight",
    "resilience",
    "interpretation",
    "hearing",
    "strength",
    "speed",
    "endurance",
    "agility",
    "hardiness",
    "robustness",
    "size",
    "constitution",
    "metabolism",
    "recovery",
    "vocalization",
    "gesturing",
    "custom",
];
const DISPOSITIONS: &[&str] = &[
    "patience",
    "risk-tolerance",
    "sociability",
    "empathy",
    "compassion",
    "rigidity",
    "custom",
];
#[derive(Clone, Serialize, Deserialize)]
struct Check(PersonId);
impl Request for Check {
    const KEY: &'static str = "test.projection-check";
    type Response = (Vec<u64>, Vec<i64>, [bool; 3]);
}
fn check(tx: &Transaction<'_>, p: PersonId) -> Result<<Check as Request>::Response> {
    let traits = tx.get::<game_traits::Traits>().ok();
    let life = tx.get::<Vitality>().ok().and_then(|v| v.people.get(&p));
    let expected_ability = |key| {
        let raw = traits.map_or(1000, |t| t.ability(p, key));
        life.map_or(raw, |v| v.ability(raw, key, tx.now()))
    };
    let expected_disposition = |key| {
        let raw = traits.map_or(500, |t| t.disposition(p, key));
        life.map_or(raw, |v| v.disposition(raw, key, tx.now()))
    };
    let expected_ready = |key, ability, threshold| {
        life.is_none_or(|v| {
            v.maturity(tx.now(), key) * factor(expected_ability(ability)) / 1000 >= threshold
        })
    };
    let expected = (
        ABILITIES.iter().map(|key| expected_ability(key)).collect(),
        DISPOSITIONS
            .iter()
            .map(|key| expected_disposition(key))
            .collect(),
        [
            expected_ready("speed", "speed", 200),
            expected_ready("agility", "agility", 200),
            expected_ready("language", "intelligence", 400),
        ],
    );
    for _ in 0..3 {
        let actual = (
            ABILITIES.iter().map(|key| ability(tx, p, key)).collect(),
            DISPOSITIONS
                .iter()
                .map(|key| disposition(tx, p, key))
                .collect(),
            [can_walk(tx, p), can_manipulate(tx, p), can_converse(tx, p)],
        );
        assert_eq!(actual, expected);
    }
    Ok(expected)
}
#[derive(Clone, Serialize, Deserialize)]
struct DirectTraits {
    p: PersonId,
    value: i32,
}
impl Request for DirectTraits {
    const KEY: &'static str = "test.projection-direct-traits";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct DirectLife {
    p: PersonId,
    rollback: bool,
}
impl Request for DirectLife {
    const KEY: &'static str = "test.projection-direct-life";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Rollback(PersonId);
impl Request for Rollback {
    const KEY: &'static str = "test.projection-rollback";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    b.request::<DirectTraits>(|tx, r| {
        let t = tx.edit::<game_traits::Traits>()?;
        t.people
            .get_mut(&r.p)
            .unwrap()
            .insert("strength".into(), r.value);
        t.people
            .get_mut(&r.p)
            .unwrap()
            .insert("custom".into(), r.value);
        t.definitions.get_mut("patience").unwrap().maximum = r.value.max(1000);
        tx.call(Check(r.p))?;
        Ok(())
    })
    .unwrap();
    plugin(&mut b).unwrap();
    b.request::<Check>(|tx, r| check(tx, r.0)).unwrap();
    b.request::<DirectLife>(|tx, r| {
        let life = tx.edit::<Vitality>()?.people.get_mut(&r.p).unwrap();
        life.stress = 900;
        life.physical = 100;
        life.upbringing.distress += 10 * 365 * DAY.0;
        tx.call(Check(r.p))?;
        require(!r.rollback, "test rollback")
    })
    .unwrap();
    b.request::<Rollback>(|tx, r| {
        let before = tx.call(Check(r.0))?;
        assert!(
            tx.attempt(DirectLife {
                p: r.0,
                rollback: true
            })
            .is_err()
        );
        assert_eq!(tx.call(Check(r.0))?, before);
        Ok(())
    })
    .unwrap();
    b
}
fn populated() -> Host {
    let mut h = builder().build(2, 1).unwrap();
    let mut t = game_traits::Traits::default();
    for key in ABILITIES.iter().chain(DISPOSITIONS) {
        t.definitions.insert(
            (*key).into(),
            game_traits::Trait {
                name: (*key).into(),
                minimum: 0,
                maximum: 16000,
                default: 1000,
                founder_spread: 0,
                weights: Default::default(),
            },
        );
    }
    h.dispatch(game_traits::Install(t)).unwrap();
    for p in [PersonId(1), PersonId(129)] {
        h.dispatch(game_traits::Set {
            person: p,
            profile: [("strength".into(), 1000)].into(),
        })
        .unwrap();
        h.dispatch(Register {
            person: p,
            born: SimTime(0),
            span: Some(SimDuration(80 * 365 * DAY.0)),
            base_health: 100,
        })
        .unwrap();
    }
    h
}
#[test]
fn checked_projections_follow_staged_edits_rollback_clocks_collisions_and_worlds() {
    let mut h = populated();
    let other = populated();
    let saved = h.save().unwrap();
    for days in [0, 365, 6 * 365, 20 * 365, 70 * 365] {
        h.advance(SimTime(days * DAY.0), 1).unwrap();
        for p in [PersonId(1), PersonId(129), PersonId(1)] {
            h.query(Check(p)).unwrap();
        }
        h.dispatch(Rollback(PersonId(1))).unwrap();
        h.dispatch(DirectTraits {
            p: PersonId(1),
            value: 2000,
        })
        .unwrap();
        h.dispatch(DirectLife {
            p: PersonId(129),
            rollback: false,
        })
        .unwrap();
        other.query(Check(PersonId(1))).unwrap();
        h.query(Check(PersonId(1))).unwrap();
        let loaded = builder().load(&saved, 4).unwrap();
        loaded.query(Check(PersonId(1))).unwrap();
    }
    std::thread::spawn(move || h.query(Check(PersonId(1))).unwrap())
        .join()
        .unwrap();
}
#[test]
fn projections_keep_missing_resources_and_missing_person_defaults() {
    let mut bare = Builder::new();
    bare.request::<Check>(|tx, r| check(tx, r.0)).unwrap();
    bare.build(1, 1).unwrap().query(Check(PersonId(1))).unwrap();
    builder()
        .build(1, 1)
        .unwrap()
        .query(Check(PersonId(1)))
        .unwrap();
    let h = populated();
    h.query(Check(PersonId(99))).unwrap();
}

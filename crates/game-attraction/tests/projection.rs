use game_core::*;
use game_relationships::Social;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Check;
impl Request for Check {
    const KEY: &'static str = "test.appraisal.check";
    type Response = i64;
}
fn reference(tx: &Transaction<'_>, p: PersonId, q: PersonId) -> i64 {
    let s = tx.get::<Social>().unwrap();
    let relationship = s.relationships.get(&p).and_then(|r| r.get(&q));
    let mut score = relationship.map_or(0, |r| {
        i64::from(r.affection) * 2 + i64::from(r.trust) + i64::from(r.admiration) / 2
            - i64::from(r.fear) * 3
            - i64::from(r.grievance) * 3
    });
    let mut compatibility = 0;
    if let Some(row) = s.heard.get(&p).and_then(|r| r.get(&q)) {
        let mut n = 0;
        for (key, opinion) in row {
            let own = s.opinion(p, key);
            if opinion.coerced || key.starts_with("claim:") || own == 0 {
                continue;
            }
            compatibility +=
                i64::from(own.abs() + opinion.stance.abs() - (own - opinion.stance).abs()) / 2;
            n += 1;
            if n == 8 {
                break;
            }
        }
    }
    score += compatibility.clamp(-200, 200);
    let positions = tx.get::<game_transport::Positions>().unwrap();
    if game_transport::area(positions, p)
        .ok()
        .is_some_and(|a| game_transport::area(positions, q).ok() == Some(a))
    {
        let mut beauty = 0;
        for (feature, locus) in [("size", "taste-size"), ("strength", "taste-build")] {
            let ideal = (game_vitality::disposition(tx, p, locus)
                + i64::from(s.opinion(p, &format!("beauty:{feature}"))) * 5)
                .clamp(0, 1000);
            let apparent = (game_vitality::ability(tx, q, feature) / 200 * 100).min(1000) as i64;
            beauty += 50 - (ideal - apparent).abs() / 5;
        }
        score += beauty * game_vitality::disposition(tx, p, "aesthetic-sensitivity") / 1000;
    }
    score
}
#[derive(Clone, Serialize, Deserialize)]
struct Edit {
    kind: u8,
    rollback: bool,
}
impl Request for Edit {
    const KEY: &'static str = "test.appraisal.edit";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Rollback;
impl Request for Rollback {
    const KEY: &'static str = "test.appraisal.rollback";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("appraisal-test", 1).unwrap();
    b.resource::<Social>().unwrap();
    b.resource::<game_traits::Traits>().unwrap();
    b.resource::<game_vitality::Vitality>().unwrap();
    b.resource::<game_transport::Positions>().unwrap();
    b.request::<Check>(|tx, _| {
        let mut last = 0;
        for _ in 0..2 {
            for p in [PersonId(1), PersonId(129), PersonId(1)] {
                for q in (2..70).map(PersonId) {
                    last = game_attraction::attraction(tx, p, q);
                    assert_eq!(last, reference(tx, p, q));
                    assert_eq!(game_attraction::attraction(tx, p, q), last);
                }
            }
        }
        Ok(last)
    })
    .unwrap();
    b.request::<Edit>(|tx, r| {
        match r.kind {
            0 => {
                for n in [1, 129] {
                    tx.edit::<game_transport::Positions>()?
                        .0
                        .insert(PersonId(n), game_transport::Location::At(AreaId(1)));
                }
                tx.edit::<game_vitality::Vitality>()?.people.insert(
                    PersonId(2),
                    game_vitality::Life {
                        born: SimTime(0),
                        span: None,
                        base_health: 100,
                        expressed: true,
                        physical: 1000,
                        stress: 0,
                        satisfaction: 0,
                        updated: SimTime(0),
                        upbringing: Default::default(),
                    },
                );
                for name in [
                    "size",
                    "strength",
                    "taste-size",
                    "taste-build",
                    "aesthetic-sensitivity",
                ] {
                    tx.edit::<game_traits::Traits>()?.definitions.insert(
                        name.into(),
                        game_traits::Trait {
                            name: name.into(),
                            minimum: 0,
                            maximum: 16000,
                            default: 1000,
                            founder_spread: 0,
                            weights: Default::default(),
                        },
                    );
                }
                tx.edit::<Social>()?
                    .values
                    .entry(PersonId(1))
                    .or_default()
                    .insert("beauty:size".into(), 37);
            }
            1 => {
                tx.edit::<game_traits::Traits>()?
                    .people
                    .entry(PersonId(2))
                    .or_default()
                    .insert("strength".into(), 2000);
            }
            4 => {
                tx.edit::<game_vitality::Vitality>()?
                    .people
                    .get_mut(&PersonId(2))
                    .unwrap()
                    .physical = 400;
            }
            5 => {
                tx.edit::<game_traits::Traits>()?
                    .definitions
                    .get_mut("size")
                    .unwrap()
                    .default = 2700;
            }
            6 => {
                tx.edit::<Social>()?
                    .values
                    .get_mut(&PersonId(1))
                    .unwrap()
                    .insert("beauty:size".into(), -38);
            }
            2 => {
                tx.edit::<game_transport::Positions>()?
                    .0
                    .insert(PersonId(2), game_transport::Location::At(AreaId(2)));
            }
            _ => {
                tx.edit::<game_transport::Positions>()?
                    .0
                    .insert(PersonId(2), game_transport::Location::At(AreaId(1)));
            }
        }
        tx.call(Check)?;
        require(!r.rollback, "test rollback")
    })
    .unwrap();
    b.request::<Rollback>(|tx, _| {
        let before = tx.call(Check)?;
        for kind in 0..7 {
            assert!(
                tx.attempt(Edit {
                    kind,
                    rollback: true
                })
                .is_err()
            );
        }
        assert_eq!(before, tx.call(Check)?);
        Ok(())
    })
    .unwrap();
    b
}
#[test]
fn physical_view_matches_reference_through_edits_clock_changes_rollback_and_reload() {
    if std::env::var_os("LEGACY_APPRAISAL_CACHE").is_none_or(|v| v != "1") {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "physical_view_matches_reference_through_edits_clock_changes_rollback_and_reload",
            ])
            .env("LEGACY_APPRAISAL_CACHE", "1")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        return;
    }
    let mut h = builder().build(3, 1).unwrap();
    // Missing positions and profiles retain the original defaults.
    h.query(Check).unwrap();
    h.dispatch(Edit {
        kind: 3,
        rollback: false,
    })
    .unwrap();
    for kind in 0..7 {
        h.dispatch(Edit {
            kind,
            rollback: false,
        })
        .unwrap();
        h.query(Check).unwrap();
    }
    h.dispatch(Rollback).unwrap();
    h.advance(SimTime(DAY.0), 1).unwrap();
    h.query(Check).unwrap();
    let loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    loaded.query(Check).unwrap();
    std::thread::spawn(move || h.query(Check).unwrap())
        .join()
        .unwrap();
}

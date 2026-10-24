use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Act;
impl Request for Act {
    const KEY: &'static str = "test.late-offer-act";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Seen(usize);
impl Resource for Seen {
    const KEY: &'static str = "test.late-offers-seen";
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.resource::<Seen>().unwrap();
    b.request::<Act>(|_, _| Ok(())).unwrap();
    b
}
fn collect(hooked: bool) -> (Vec<Proposal>, usize) {
    let mut b = builder();
    if hooked {
        b.before::<Offer>("count-every-offer", |tx, _| {
            tx.edit::<Seen>()?.0 += 1;
            Ok(())
        })
        .unwrap();
    }
    b.subscribe::<Review>("offers", |tx, e| {
        for i in 0u64..4096 {
            if i % 400 == 0 {
                tx.call(game_traits::Set {
                    person: e.person,
                    profile: [("drive".into(), if i % 800 == 0 { -100 } else { 100 })].into(),
                })?;
            }
            let n = i.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = if i % 17 == 0 {
                i64::MAX
            } else {
                (n % 41) as i64 - 20
            };
            offer(
                tx,
                e.person,
                format!("offer:{i}:雪"),
                motive(value, -value, value, 0, 0),
                Act,
            )?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let mut traits = game_traits::Traits::default();
    traits.definitions.insert(
        "drive".into(),
        game_traits::Trait {
            name: "Drive".into(),
            minimum: -100,
            maximum: 100,
            default: 0,
            founder_spread: 0,
            weights: [("goal".into(), 1000)].into(),
        },
    );
    h.dispatch(game_traits::Install(traits)).unwrap();
    let offers = h
        .dispatch(Collect {
            person: PersonId(1),
        })
        .unwrap();
    (offers, h.get::<Seen>().unwrap().0)
}
#[test]
fn early_rejection_matches_hooked_full_submission_with_ties_saturation_and_weight_changes() {
    let (fast, seen) = collect(false);
    let (reference, hooked) = collect(true);
    assert_eq!(fast, reference);
    assert_eq!(seen, 0);
    assert_eq!(hooked, 4096, "hooks still observe losing offers");
}
#[test]
fn losing_invalid_and_duplicate_keys_still_fail() {
    let mut b = builder();
    b.subscribe::<Review>("offers", |tx, e| {
        for i in 0..=LIMIT {
            offer(
                tx,
                e.person,
                format!("winner:{i}"),
                motive(i64::MAX, 0, 0, 0, 0),
                Act,
            )?;
        }
        for key in ["".to_string(), "x".repeat(161)] {
            assert!(
                offer(tx, e.person, key, motive(i64::MIN, 0, 0, 0, 0), Act)
                    .unwrap_err()
                    .to_string()
                    .contains("invalid proposal key")
            );
        }
        let key = tx.call(Collect { person: e.person }).unwrap_err();
        assert!(key.to_string().contains("recursive choice collection"));
        // Read a retained key independently of which equal-score ID lost the top-K tie.
        let duplicate = (0..=LIMIT)
            .map(|i| format!("winner:{i}"))
            .min_by_key(|key| identity(key))
            .unwrap();
        assert!(
            offer(tx, e.person, duplicate, motive(i64::MIN, 0, 0, 0, 0), Act)
                .unwrap_err()
                .to_string()
                .contains("duplicate proposal key")
        );
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(1)
        })
        .unwrap()
        .len(),
        LIMIT
    );
}

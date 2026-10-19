use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counter(u64);
impl Resource for Counter {
    const KEY: &'static str = "test.counter";
}
#[derive(Clone, Serialize, Deserialize)]
struct Act {
    value: u64,
    fail: bool,
}
impl Request for Act {
    const KEY: &'static str = "test.act";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct Decide;
impl Request for Decide {
    const KEY: &'static str = "test.decide";
    type Response = String;
}
fn builder(reverse: bool, fail: bool) -> Builder {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.resource::<Counter>().unwrap();
    b.request::<Act>(|tx, r| {
        tx.edit::<Counter>()?.0 += r.value;
        Ok(!r.fail)
    })
    .unwrap();
    let providers = if reverse {
        [("a", 100, 100, fail), ("z", 20, 2, false)]
    } else {
        [("z", 100, 100, fail), ("a", 20, 2, false)]
    };
    for (name, goal, value, fail) in providers {
        b.subscribe::<Review>(name, move |tx, e| {
            offer(
                tx,
                e.person,
                format!("act:{value}"),
                motive(0, goal, 0, 0, 0),
                Act { value, fail },
            )?;
            Ok(())
        })
        .unwrap();
    }
    b.request::<Decide>(|tx, _| {
        let mut proposals = tx.call(Collect {
            person: PersonId(1),
        })?;
        proposals.sort_by_key(|p| std::cmp::Reverse(p.motive.score()));
        for p in proposals {
            if tx
                .attempt(Run(p.request))
                .is_ok_and(|o| o == Outcome::Performed)
            {
                return Ok(p.key);
            }
        }
        Err(Error::Rejected("no accepted option".into()))
    })
    .unwrap();
    b
}
#[test]
fn changing_provider_order_does_not_change_the_selected_action() {
    let mut a = builder(false, false).build(1, 1).unwrap();
    let mut b = builder(true, false).build(1, 4).unwrap();
    assert_eq!(a.dispatch(Decide).unwrap(), "act:100");
    assert_eq!(b.dispatch(Decide).unwrap(), "act:100");
    assert_eq!(a.save().unwrap(), b.save().unwrap());
}
#[test]
fn declining_an_offer_rolls_back_its_changes_before_trying_another() {
    let mut h = builder(false, true).build(1, 1).unwrap();
    assert_eq!(h.dispatch(Decide).unwrap(), "act:2");
    assert_eq!(h.get::<Counter>().unwrap().0, 2);
    let mut loaded = builder(true, true).load(&h.save().unwrap(), 4).unwrap();
    h.dispatch(Decide).unwrap();
    loaded.dispatch(Decide).unwrap();
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn collecting_offers_spends_no_materials_or_action_time() {
    let mut h = builder(false, false).build(1, 1).unwrap();
    assert_eq!(
        h.dispatch(Collect {
            person: PersonId(1)
        })
        .unwrap()
        .len(),
        2
    );
    assert_eq!(h.get::<Counter>().unwrap().0, 0);
    assert_eq!(h.now(), SimTime(0));
    assert!(h.get::<Actions>().unwrap().0.is_empty());
    assert!(h.save().is_ok());
}

#[test]
fn bounded_recall_retains_the_best_offer_even_when_its_key_sorts_last() {
    let mut b = builder(false, false);
    b.subscribe::<Review>("many", |tx, e| {
        for i in 0..LIMIT + 32 {
            offer(
                tx,
                e.person,
                format!("a:{i}"),
                motive(0, 1, 0, 0, 0),
                Act {
                    value: 1,
                    fail: false,
                },
            )?;
        }
        offer(
            tx,
            e.person,
            "zz:best",
            motive(0, 10000, 0, 0, 0),
            Act {
                value: 7,
                fail: false,
            },
        )?;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    assert_eq!(h.dispatch(Decide).unwrap(), "zz:best");
    assert_eq!(h.get::<Counter>().unwrap().0, 7);
}

#[test]
fn offer_limits_reassess_earlier_offers_when_personal_weights_change() {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Act>(|_, _| Ok(true)).unwrap();
    b.subscribe::<Review>("offers", |tx, e| {
        for i in 0..LIMIT {
            let goal = i < LIMIT / 2;
            offer(
                tx,
                e.person,
                format!("{}:{i}", if goal { "goal" } else { "value" }),
                motive(
                    0,
                    if goal { 20 } else { 0 },
                    if goal { 0 } else { 50 },
                    0,
                    0,
                ),
                Act {
                    value: 0,
                    fail: false,
                },
            )?;
        }
        tx.call(game_traits::Set {
            person: e.person,
            profile: [("drive".into(), 100)].into(),
        })?;
        offer(
            tx,
            e.person,
            "new-goal",
            motive(0, 10, 0, 0, 0),
            Act {
                value: 0,
                fail: false,
            },
        )?;
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let mut traits = game_traits::Traits::default();
    traits.definitions.insert(
        "drive".into(),
        game_traits::Trait {
            name: "Drive".into(),
            minimum: 0,
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
    assert_eq!(offers.len(), LIMIT);
    assert_eq!(
        offers.iter().filter(|p| p.key.starts_with("goal:")).count(),
        LIMIT / 2
    );
    assert_eq!(
        offers
            .iter()
            .filter(|p| p.key.starts_with("value:"))
            .count(),
        LIMIT / 2 - 1
    );
    assert!(offers.windows(2).all(|p| p[0].key < p[1].key));
}

#[test]
fn changing_a_proposals_numeric_motive_cannot_bypass_full_key_uniqueness() {
    let mut b = builder(false, false);
    b.subscribe::<Review>("duplicate", |tx, e| {
        for id in [11, 22] {
            let mut p = Proposal::new(
                "duplicate",
                motive(0, 1, 0, 0, 0),
                Act {
                    value: 0,
                    fail: false,
                },
            )?;
            p.motive.id = id;
            submit(tx, e.person, p)?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Collect {
            person: PersonId(1)
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
}

#[derive(Clone, Serialize, Deserialize)]
struct BeginTask;
impl Request for BeginTask {
    const KEY: &'static str = "test.begin-task";
    type Response = ();
}
#[test]
fn bounded_search_keeps_the_real_continuation_available_without_forcing_it_to_win() {
    let mut b = builder(false, false);
    b.request::<BeginTask>(|tx, _| {
        tx.start_action(
            PersonId(1),
            "task",
            Some(SimTime(HOUR.0)),
            Act {
                value: 0,
                fail: false,
            },
        )
        .map(|_| ())
    })
    .unwrap();
    b.subscribe::<Review>("crowded", |tx, e| {
        let action = tx.get::<Actions>()?.0[&e.person].id;
        for (key, continuation) in [("ongoing", action), ("unrelated-continuation", action + 1)] {
            let mut proposal = Proposal::new(
                key,
                motive(0, 0, 0, 1, 0),
                Act {
                    value: 0,
                    fail: false,
                },
            )?;
            proposal.continuation = Some(continuation);
            submit(tx, e.person, proposal)?;
        }
        for i in 0..LIMIT + 8 {
            offer(
                tx,
                e.person,
                format!("valuable:{i}"),
                motive(0, 1000, 0, 0, 0),
                Act {
                    value: 1,
                    fail: false,
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    let mut h = b.build(1, 1).unwrap();
    h.dispatch(BeginTask).unwrap();
    let offers = h
        .dispatch(Collect {
            person: PersonId(1),
        })
        .unwrap();
    assert_eq!(offers.len(), LIMIT);
    let current = offers
        .iter()
        .find(|p| p.key == "ongoing")
        .expect("active intention disappeared from bounded search");
    assert!(!offers.iter().any(|p| p.key == "unrelated-continuation"));
    let chosen = game_utility::choose(
        &offers.iter().map(|p| p.motive).collect::<Vec<_>>(),
        Some(current.motive.id),
        20,
        false,
    )
    .unwrap();
    assert_ne!(chosen.id, current.motive.id);
}

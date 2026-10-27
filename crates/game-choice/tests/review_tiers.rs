use game_choice::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Serialize, Deserialize)]
struct Draft {
    fast: bool,
    fanout: bool,
}
impl Request for Draft {
    const KEY: &'static str = "test.tier-draft";
    type Response = Vec<Proposal>;
}
#[derive(Clone, Serialize, Deserialize)]
struct RejectedDraft;
impl Request for RejectedDraft {
    const KEY: &'static str = "test.rejected-tier-draft";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct FramedFastDraft;
impl Request for FramedFastDraft {
    const KEY: &'static str = "test.framed-fast-draft";
    type Response = Vec<Proposal>;
}
#[derive(Clone, Serialize, Deserialize)]
struct CheckDeferred(Proposal);
impl Request for CheckDeferred {
    const KEY: &'static str = "test.check-deferred-provider";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct CatchFastFailure;
impl Request for CatchFastFailure {
    const KEY: &'static str = "test.catch-fast-failure";
    type Response = Vec<Proposal>;
}
fn builder(hooks: Arc<AtomicUsize>, with_hook: bool) -> Builder {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    b.plugin("test.review-tiers", 1).unwrap();
    b.request::<Draft>(|tx, r| {
        let mut offers = None;
        let draft = tx.draft(Offers::planning_tier(r.fanout, r.fast), |child| {
            offers = Some(child.call(Collect {
                person: PersonId(1),
            })?);
            Ok(())
        });
        require(draft.accepted, "tier draft rejected")?;
        offers.ok_or_else(|| Error::Missing("tier draft result".into()))
    })
    .unwrap();
    b.request::<RejectedDraft>(|tx, _| {
        let draft = tx.draft(Offers::planning_tier(false, true), |child| {
            child.call(Collect {
                person: PersonId(1),
            })?;
            Err(Error::Rejected("test rollback".into()))
        });
        require(!draft.accepted, "expected rejected draft")
    })
    .unwrap();
    b.request::<FramedFastDraft>(|tx, _| {
        let mut scope = Offers::planning_tier(false, true);
        scope.frame = tx.get::<Offers>()?.frame.clone();
        let mut offers = None;
        let draft = tx.draft(scope, |child| {
            offers = Some(child.call(Collect {
                person: PersonId(1),
            })?);
            Ok(())
        });
        require(draft.accepted, "framed fast draft rejected")?;
        offers.ok_or_else(|| Error::Missing("framed fast offers".into()))
    })
    .unwrap();
    b.request::<CheckDeferred>(|tx, r| {
        frame_stale_provider(tx, PersonId(1), &r.0, Some("test.deferred".into()))
    })
    .unwrap();
    b.subscribe::<game_decision_frames::Gather>("test.tier-frame", |tx, _| {
        tx.call(game_decision_frames::Batch {
            people: vec![PersonId(1)],
        })
    })
    .unwrap();
    provider(&mut b, "test.live", |tx, e| {
        offer(tx, e.person, "live", motive(5, 0, 0, 0, 0), Wait)?;
        Ok(())
    })
    .unwrap();
    deliberative_provider(&mut b, "test.deferred", |tx, e| {
        offer(tx, e.person, "deferred", motive(2, 0, 0, 0, 0), Wait)?;
        Ok(())
    })
    .unwrap();
    serial_provider(&mut b, "test.ordered", |tx, e| {
        offer(tx, e.person, "ordered", motive(3, 0, 0, 0, 0), Wait)?;
        Ok(())
    })
    .unwrap();
    b.subscribe::<Review>("test.unclassified", |tx, e| {
        offer(tx, e.person, "unclassified", motive(4, 0, 0, 0, 0), Wait)?;
        Ok(())
    })
    .unwrap();
    if with_hook {
        b.before::<Offer>("test.observe-offers", move |_, _| {
            hooks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    }
    b
}

#[test]
fn failed_fast_drafts_do_not_leak_and_stale_checks_remain_full() {
    let mut host = builder(Arc::new(AtomicUsize::new(0)), false)
        .build(8, 4)
        .unwrap();
    host.dispatch(RejectedDraft).unwrap();
    host.dispatch(ConfigureFrames {
        interval: Some(MINUTE),
    })
    .unwrap();
    host.dispatch(TuneFrames(FrameTuning {
        recheck_conflicts: true,
        ..Default::default()
    }))
    .unwrap();
    host.advance(SimTime(0), 1).unwrap();
    assert!(
        host.get::<Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .proposals[&PersonId(1)]
            .contains_key("test.deferred")
    );
    let full = host
        .dispatch(Collect {
            person: PersonId(1),
        })
        .unwrap();
    let deferred = full
        .into_iter()
        .find(|offer| offer.key == "deferred")
        .unwrap();
    assert!(!host.dispatch(CheckDeferred(deferred)).unwrap());
    assert!(
        host.dispatch(Draft {
            fast: true,
            fanout: true
        })
        .unwrap()
        .iter()
        .all(|offer| offer.key != "deferred")
    );
    assert!(
        host.dispatch(FramedFastDraft)
            .unwrap()
            .iter()
            .all(|p| p.key != "deferred")
    );
    assert!(
        host.dispatch(Collect {
            person: PersonId(1)
        })
        .unwrap()
        .iter()
        .any(|offer| offer.key == "deferred")
    );
}

#[test]
fn live_scoped_collection_restores_full_scope_after_failure() {
    let mut b = builder(Arc::new(AtomicUsize::new(0)), false);
    b.subscribe::<Review>("test.reject-fast", |tx, _| {
        if fast_review(tx) {
            Err(Error::Rejected("test fast error".into()))
        } else {
            Ok(())
        }
    })
    .unwrap();
    b.request::<CatchFastFailure>(|tx, _| {
        assert!(
            tx.attempt(CollectScoped {
                person: PersonId(1),
                fast: true
            })
            .is_err()
        );
        tx.call(Collect {
            person: PersonId(1),
        })
    })
    .unwrap();
    let mut host = b.build(8, 1).unwrap();
    let choices = host.dispatch(CatchFastFailure).unwrap();
    assert!(choices.iter().any(|p| p.key == "deferred"));
    host.save().unwrap();
}

#[test]
fn fast_scope_skips_only_opted_in_providers_and_never_leaks() {
    for (workers, with_hook) in [(1, false), (4, false), (1, true), (4, true)] {
        let hooks = Arc::new(AtomicUsize::new(0));
        let mut host = builder(hooks.clone(), with_hook).build(8, workers).unwrap();
        let keys = |offers: Vec<Proposal>| offers.into_iter().map(|p| p.key).collect::<Vec<_>>();
        for fanout in [false, true] {
            assert_eq!(
                keys(
                    host.dispatch(Draft {
                        fast: false,
                        fanout
                    })
                    .unwrap()
                ),
                ["deferred", "live", "ordered", "unclassified"]
            );
            assert_eq!(
                keys(host.dispatch(Draft { fast: true, fanout }).unwrap()),
                ["live", "ordered", "unclassified"]
            );
            assert_eq!(
                keys(
                    host.dispatch(Collect {
                        person: PersonId(1)
                    })
                    .unwrap()
                ),
                ["deferred", "live", "ordered", "unclassified"]
            );
        }
        assert_eq!(hooks.load(Ordering::SeqCst), if with_hook { 22 } else { 0 });
        let save = host.save().unwrap();
        let mut loaded = builder(Arc::new(AtomicUsize::new(0)), with_hook)
            .load(&save, 4)
            .unwrap();
        assert_eq!(
            host.dispatch(Draft {
                fast: true,
                fanout: false
            })
            .unwrap(),
            loaded
                .dispatch(Draft {
                    fast: true,
                    fanout: false
                })
                .unwrap()
        );
        assert_eq!(host.save().unwrap(), loaded.save().unwrap());
    }
}

mod support;
use game_characters::{Configure, Minds, Register, Rules, Start};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Work {
    reviews: u64,
    primary: u64,
    alternate: u64,
    changed: bool,
    blocked: bool,
    duration: SimDuration,
}
impl Resource for Work {
    const KEY: &'static str = "test.option-work";
}
#[derive(Clone, Serialize, Deserialize)]
struct Begin {
    person: PersonId,
    alternate: bool,
    known_changed: bool,
}
impl Request for Begin {
    const KEY: &'static str = "test.option-begin";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish(bool);
impl Request for Finish {
    const KEY: &'static str = "test.option-finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Change {
    person: PersonId,
    blocked: bool,
}
impl Request for Change {
    const KEY: &'static str = "test.option-change";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Arrival(PersonId, AreaId);
impl Request for Arrival {
    const KEY: &'static str = "test.option-arrival";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct RejectRefresh(PersonId);
impl Request for RejectRefresh {
    const KEY: &'static str = "test.option-reject-refresh";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Duration(SimDuration);
impl Request for Duration {
    const KEY: &'static str = "test.option-duration";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("option-test", 1).unwrap();
    b.resource::<Work>().unwrap();
    b.request::<Duration>(|tx, r| {
        tx.edit::<Work>()?.duration = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<RejectRefresh>(|tx, r| {
        tx.call(game_physiology::Damage {
            person: r.0,
            amount: 1,
        })?;
        tx.call(game_characters::Resume { person: r.0 })?;
        require(false, "reject after refreshing considered options")
    })
    .unwrap();
    b.request::<Arrival>(|tx, r| {
        tx.notify(game_transport::Arrived {
            person: r.0,
            area: r.1,
        })
    })
    .unwrap();
    b.request::<Begin>(|tx, r| {
        require(
            r.alternate || !tx.get::<Work>()?.blocked,
            "primary unavailable",
        )?;
        require(
            !tx.get::<Work>()?.blocked || r.known_changed == tx.get::<Work>()?.changed,
            "option assumptions changed",
        )?;
        tx.call(game_choice::Undertake {
            person: r.person,
            kind: "test-work".into(),
            duration: tx.get::<Work>()?.duration.max(MINUTE),
            request: Pending::immediate(Finish(r.alternate)),
        })?;
        Ok(())
    })
    .unwrap();
    b.request::<Finish>(|tx, r| {
        let work = tx.edit::<Work>()?;
        if r.0 {
            work.alternate += 1;
        } else {
            work.primary += 1;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Change>(|tx, r| {
        let work = tx.edit::<Work>()?;
        work.changed = true;
        work.blocked = r.blocked;
        tx.notify(game_choice::Changed { person: r.person })
    })
    .unwrap();
    game_choice::serial_provider(&mut b, "test.option-provider", |tx, e| {
        tx.edit::<Work>()?.reviews += 1;
        let work = tx.get::<Work>()?;
        let (changed, blocked) = (work.changed, work.blocked);
        if !blocked {
            game_choice::offer(
                tx,
                e.person,
                "primary",
                game_choice::motive(0, 1000, 0, 0, 0),
                Begin {
                    person: e.person,
                    alternate: false,
                    known_changed: changed,
                },
            )?;
        }
        game_choice::offer(
            tx,
            e.person,
            "alternate",
            game_choice::motive(0, if changed { 2000 } else { 1 }, 0, 0, 0),
            Begin {
                person: e.person,
                alternate: true,
                known_changed: changed,
            },
        )?;
        Ok(())
    })
    .unwrap();
    b
}
fn world(memory: Option<SimDuration>) -> (Host, PersonId, AreaId) {
    world_with_limit(memory, 16)
}
fn world_with_limit(memory: Option<SimDuration>, limit: usize) -> (Host, PersonId, AreaId) {
    world_at_rate(memory, limit, 1)
}
fn world_at_rate(
    memory: Option<SimDuration>,
    limit: usize,
    hunger: u64,
) -> (Host, PersonId, AreaId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(Configure(Rules {
        option_memory: memory,
        option_limit: limit,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: MINUTE,
        hunger_per_tick: hunger,
        fatigue_per_tick: 0,
        deprivation: 10000,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(Register {
        person,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    (h, person, area)
}
#[test]
fn consuming_a_shortlist_preserves_action_timing_and_refreshes_when_exhausted() {
    let (mut ordinary, _, _) = world(None);
    let (mut reused, _, _) = world(Some(SimDuration(5 * MINUTE.0)));
    for h in [&mut ordinary, &mut reused] {
        support::run(h, SimTime(9 * MINUTE.0));
        assert_eq!(
            h.get::<Work>().unwrap().primary + h.get::<Work>().unwrap().alternate,
            8
        );
    }
    assert_eq!(reused.get::<Work>().unwrap().reviews, 5);
    assert!(ordinary.get::<Work>().unwrap().reviews > 5);
}
#[test]
fn nonurgent_motives_wait_until_the_shortlist_is_refreshed() {
    let (mut h, person, _) = world(Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(Change {
        person,
        blocked: false,
    })
    .unwrap();
    support::run(&mut h, SimTime(4 * MINUTE.0));
    assert_eq!(
        h.get::<Minds>().unwrap().people[&person]
            .expectation
            .unwrap()
            .goal,
        1
    );
    assert_eq!(h.get::<Work>().unwrap().reviews, 2);
    support::run(&mut h, SimTime(5 * MINUTE.0));
    assert_eq!(
        h.get::<Minds>().unwrap().people[&person]
            .expectation
            .unwrap()
            .goal,
        2000
    );
    assert_eq!(h.get::<Work>().unwrap().reviews, 3);
}

#[test]
fn injury_and_arrival_invalidate_memory_immediately() {
    for arrival in [false, true] {
        let (mut h, person, area) = world(Some(HOUR));
        support::run(&mut h, SimTime(3 * MINUTE.0));
        h.dispatch(Change {
            person,
            blocked: false,
        })
        .unwrap();
        if arrival {
            h.dispatch(Arrival(person, area)).unwrap();
        } else {
            h.dispatch(game_physiology::Damage { person, amount: 1 })
                .unwrap();
        }
        assert!(
            h.get::<Minds>().unwrap().people[&person]
                .considered
                .is_none()
        );
        support::run(&mut h, SimTime(5 * MINUTE.0));
        assert!(h.get::<Work>().unwrap().alternate > 0);
        assert!(h.get::<Work>().unwrap().reviews > 1);
    }
}
#[test]
fn a_rejected_reused_start_refreshes_options_without_waiting() {
    let (mut h, person, _) = world(Some(HOUR));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(Change {
        person,
        blocked: true,
    })
    .unwrap();
    support::run(&mut h, SimTime(5 * MINUTE.0));
    assert!(h.get::<Work>().unwrap().alternate > 0);
    assert!(h.get::<Minds>().unwrap().people[&person].failures > 0);
    assert_eq!(h.get::<Actions>().unwrap().0[&person].kind, "test-work");
}
#[test]
fn urgent_hunger_refreshes_options_before_memory_expiry() {
    let (mut h, person, _) = world_at_rate(Some(HOUR), 16, 100);
    support::run(&mut h, SimTime(3 * MINUTE.0));
    let before = h.get::<Work>().unwrap().reviews;
    h.dispatch(game_physiology::Start).unwrap();
    support::run(&mut h, SimTime(4 * MINUTE.0));
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&person].hunger >= 100);
    assert!(h.get::<Work>().unwrap().reviews > before);
}
#[test]
fn remembered_owner_requests_survive_reload_workers_and_budget_changes() {
    let (mut one, person, _) = world(Some(SimDuration(5 * MINUTE.0)));
    support::run(&mut one, SimTime(3 * MINUTE.0));
    assert!(
        one.get::<Minds>().unwrap().people[&person]
            .considered
            .is_some()
    );
    let mut four = builder().load(&one.save().unwrap(), 4).unwrap();
    let target = SimTime(19 * MINUTE.0);
    while one.advance(target, 1).unwrap().stop == Stop::Budget {}
    while four.advance(target, 13).unwrap().stop == Stop::Budget {}
    assert_eq!(one.save().unwrap(), four.save().unwrap());
}
#[test]
fn abandonment_discards_considered_options() {
    let (mut h, person, _) = world(Some(HOUR));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(game_characters::Abandon { person }).unwrap();
    assert!(
        h.get::<Minds>().unwrap().people[&person]
            .considered
            .is_none()
    );
}

#[test]
fn a_failed_transaction_restores_memory_and_pending_actions_together() {
    let (mut h, person, _) = world(Some(HOUR));
    support::run(&mut h, SimTime(3 * MINUTE.0));
    let before = h.save().unwrap();
    assert!(h.dispatch(RejectRefresh(person)).is_err());
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn food_opportunities_are_recomputed_while_nonurgent_options_are_reused() {
    // Inspect the policy's food output directly, independently of which action wins.
    #[derive(Clone, Serialize, Deserialize)]
    struct FoodOptions(PersonId);
    impl Request for FoodOptions {
        const KEY: &'static str = "test.option-food";
        type Response = usize;
    }
    let mut b = builder();
    b.request::<FoodOptions>(|tx, r| {
        let state = tx.get::<Minds>()?;
        let rules = state.rules.clone();
        let memory = state.people[&r.0].considered.clone();
        let options = game_autonomy_policy::consider(tx, r.0, &rules, memory.as_deref(), false)?;
        require(options.reused, "test requires remembered nonurgent options")?;
        Ok(options.food?.len())
    })
    .unwrap();
    let (mut original, person, _) = world(Some(HOUR));
    support::run(&mut original, SimTime(3 * MINUTE.0));
    let mut h = b.load(&original.save().unwrap(), 1).unwrap();
    let before = h.dispatch(FoodOptions(person)).unwrap();
    let reviews = h.get::<Work>().unwrap().reviews;
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 50,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person,
        inventory: pack,
    })
    .unwrap();
    assert!(h.dispatch(FoodOptions(person)).unwrap() > before);
    assert_eq!(h.get::<Work>().unwrap().reviews, reviews);
}

#[test]
fn deferred_information_refreshes_at_expiry_even_during_a_long_action_and_after_reload() {
    let (mut one, person, _) = world_with_limit(Some(SimDuration(5 * MINUTE.0)), 1);
    one.dispatch(Duration(SimDuration(10 * MINUTE.0))).unwrap();
    support::run(&mut one, SimTime(3 * MINUTE.0));
    let action = one.get::<Actions>().unwrap().0[&person].id;
    one.dispatch(Change {
        person,
        blocked: false,
    })
    .unwrap();
    support::run(&mut one, SimTime(3 * MINUTE.0));
    assert_eq!(one.get::<Work>().unwrap().reviews, 1);
    assert_eq!(one.get::<Actions>().unwrap().0[&person].id, action);
    let mut four = builder().load(&one.save().unwrap(), 4).unwrap();
    for h in [&mut one, &mut four] {
        support::run(h, SimTime(6 * MINUTE.0));
        assert_eq!(h.get::<Work>().unwrap().reviews, 2);
        assert_eq!(
            h.get::<Minds>().unwrap().people[&person].chosen.as_deref(),
            Some("alternate")
        );
    }
    assert_eq!(one.save().unwrap(), four.save().unwrap());
}
#[test]
fn unused_memory_does_not_add_periodic_reviews_to_an_uninterrupted_action() {
    let (mut h, _, _) = world_with_limit(Some(SimDuration(5 * MINUTE.0)), 1);
    h.dispatch(Duration(SimDuration(10 * MINUTE.0))).unwrap();
    support::run(&mut h, SimTime(9 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 1);
}
#[test]
fn injury_invalidates_an_obsolete_expiry_without_an_extra_full_review() {
    let (mut h, person, _) = world_with_limit(Some(SimDuration(5 * MINUTE.0)), 1);
    h.dispatch(Duration(SimDuration(10 * MINUTE.0))).unwrap();
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(Change {
        person,
        blocked: false,
    })
    .unwrap();
    support::run(&mut h, SimTime(3 * MINUTE.0));
    support::run(&mut h, SimTime(4 * MINUTE.0));
    h.dispatch(game_physiology::Damage { person, amount: 1 })
        .unwrap();
    support::run(&mut h, SimTime(4 * MINUTE.0));
    let before = h.get::<Work>().unwrap().reviews;
    support::run(&mut h, SimTime(6 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, before);
}
#[test]
fn expiry_coinciding_with_completion_does_not_collect_options_twice() {
    let (mut h, person, _) = world_with_limit(Some(SimDuration(5 * MINUTE.0)), 1);
    h.dispatch(Duration(SimDuration(5 * MINUTE.0))).unwrap();
    support::run(&mut h, SimTime(3 * MINUTE.0));
    h.dispatch(Change {
        person,
        blocked: false,
    })
    .unwrap();
    support::run(&mut h, SimTime(6 * MINUTE.0));
    assert_eq!(h.get::<Work>().unwrap().reviews, 2);
}

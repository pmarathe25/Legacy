use super::*;
use crate::fixture::Fixture;
use crate::transition_tests::*;
use game_logistics::*;
use std::cell::RefCell;
use std::collections::BTreeSet;

struct Facts {
    food: Vec<FoodObservation>,
    stocks: Vec<KnownStock>,
    cargo: Vec<Cargo>,
    routes: KnownRoutes,
    target: TargetFacts,
    observed: Option<TargetObservation>,
    destination: Option<KnownDestination>,
    calls: RefCell<Vec<InventoryId>>,
}
impl Default for Facts {
    fn default() -> Self {
        Self {
            food: Vec::new(),
            stocks: Vec::new(),
            cargo: Vec::new(),
            routes: BTreeMap::from([
                (HOME, BTreeMap::from([(AWAY, SimDuration(10))])),
                (AWAY, BTreeMap::from([(HOME, SimDuration(10))])),
            ]),
            target: TargetFacts {
                at_area: Some(HOME),
                available_area: true,
                player: false,
                alive: true,
                can_fight: true,
                captor: None,
                trust: 0,
                empathy: 300,
                hunger: 0,
            },
            observed: None,
            destination: None,
            calls: RefCell::new(Vec::new()),
        }
    }
}
fn ready() -> Readiness {
    Readiness {
        actor_present: true,
        body_present: true,
        mind_present: true,
        alive: true,
        functional: true,
        independent: true,
        activity: ActivityClass::Idle,
        fatigue_limit: 100,
        can_depart: true,
        ..Readiness::default()
    }
}
impl AcquisitionSource<u8> for Facts {
    fn offers(&self, _: Good, _: Quantity, _: Quantity) -> Vec<Candidate<u8>> {
        Vec::new()
    }
}
impl MemberFacts<u8> for Facts {
    fn food(&self) -> impl Iterator<Item = FoodObservation> {
        self.food.iter().copied()
    }
    fn source_good(&self, id: InventoryId) -> Option<Good> {
        self.stocks
            .iter()
            .find(|s| s.inventory == id)
            .map(|s| s.good)
    }
    fn inventory_destination(&self, _: InventoryId) -> Option<KnownDestination> {
        self.destination.clone()
    }
    fn actor_location(&self) -> Location {
        Location::AtArea(HOME)
    }
    fn objective_knowledge(&self, _: &MissionObjective) -> ObjectiveKnowledge {
        ObjectiveKnowledge {
            target: self.observed,
            stocks: BTreeMap::new(),
        }
    }
    fn target(&self, _: PersonId) -> TargetFacts {
        self.target
    }
    fn delivery_facts(&self, req: &MemberDelivery<'_>) -> DeliveryFacts<'_, u8> {
        self.calls
            .borrow_mut()
            .push(req.delivery.requirement.source.unwrap());
        let cargo = self
            .cargo
            .iter()
            .cloned()
            .map(|mut c| {
                c.origin_quantity = req
                    .origin_cargo
                    .and_then(|tags| tags.get(&c.inventory))
                    .copied()
                    .map(Quantity)
                    .unwrap_or(Quantity(0));
                c
            })
            .collect();
        DeliveryFacts {
            now: SimTime(0),
            place: Location::AtArea(HOME),
            readiness: ready(),
            room: Quantity(5000),
            cargo,
            stocks: self.stocks.clone(),
            contacts: BTreeSet::from([A]),
            routes: &self.routes,
            destination: Some(req.destination.clone()),
            direct_transfer: false,
            additional: Vec::new(),
        }
    }
}
fn world(objective: MissionObjective) -> Fixture {
    let mut w = state();
    let f = facts();
    let mut p = proposal();
    p.objective = objective;
    p.supplies.clear();
    start(&mut w, p, &f);
    accept(&mut w, A, OFFER, &f);
    let m = mission_mut(&mut w.store, ID).unwrap();
    m.status = MissionStatus::Operating;
    m.members.get_mut(&A).unwrap().contacts.insert(ENEMY, HOME);
    w
}
fn motives() -> ServiceMotives {
    ServiceMotives {
        honesty: 900,
        leader_trust: 0,
        guardian_duty: false,
        payment: None,
    }
}
fn input(m: &ServiceMotives) -> MemberInput<'_> {
    MemberInput {
        actor: A,
        now: SimTime(0),
        player: false,
        in_combat: false,
        area: Some(HOME),
        communication_busy: false,
        activity: MemberActivity::Other,
        care_urgency: 0,
        hunger: 0,
        meal_relief: 100,
        meal_size: Quantity(1000),
        fatigue: 0,
        fatigue_limit: 100,
        health: 100,
        max_health: 100,
        motives: m,
    }
}
fn stock(id: u64) -> KnownStock {
    KnownStock {
        inventory: InventoryId(id),
        good: Good::Food,
        quantity: Quantity(5000),
        accessible: true,
        location: Location::AtArea(HOME),
        observed_at: SimTime(0),
        room: None,
    }
}
#[test]
fn occupation_care_food_and_recovery_priorities_match_the_scalar_policy() {
    let w = world(MissionObjective::Visit { destination: AWAY });
    let f = Facts::default();
    let m = motives();
    for activity in [
        MemberActivity::Eat,
        MemberActivity::Tell,
        MemberActivity::Escape,
        MemberActivity::ArmyApproach,
    ] {
        assert!(
            w.member_decision(
                &w.store,
                ID,
                MemberInput {
                    activity,
                    hunger: 100,
                    fatigue: 100,
                    ..input(&m)
                },
                &f
            )
            .attempts
            .is_empty()
        );
    }
    assert!(
        w.member_decision(
            &w.store,
            ID,
            MemberInput {
                care_urgency: 20_000,
                ..input(&m)
            },
            &f
        )
        .attempts
        .is_empty()
    );
    // No known food still consumes the pass; it does not fall through to Rest or travel.
    assert_eq!(
        w.member_decision(
            &w.store,
            ID,
            MemberInput {
                hunger: 100,
                fatigue: 100,
                ..input(&m)
            },
            &f
        ),
        MemberDecision::default()
    );
    assert_eq!(
        w.member_decision(
            &w.store,
            ID,
            MemberInput {
                fatigue: 50,
                ..input(&m)
            },
            &f
        )
        .attempts[0]
            .action,
        MemberAction::Rest
    );
    let guardian = ServiceMotives {
        guardian_duty: true,
        ..m
    };
    let decision = w.member_decision(
        &w.store,
        ID,
        MemberInput {
            activity: MemberActivity::Eat,
            care_urgency: 99_000,
            ..input(&guardian)
        },
        &f,
    );
    assert_eq!(
        decision.attempts[0].action,
        MemberAction::Mission(MissionAction::Leave { mission: ID })
    );
    assert_eq!(decision.explanation, Some(MemberExplanation::Guardian));
}
#[test]
fn food_selection_uses_dated_permission_then_carried_priority_and_stable_id() {
    let w = world(MissionObjective::Visit { destination: AWAY });
    let m = motives();
    let mut f = Facts::default();
    let food = FoodObservation {
        inventory: InventoryId(20),
        good: Good::Food,
        accessible: true,
        quantity: Quantity(1000),
        area: HOME,
        carried_by_actor: false,
    };
    f.food = vec![
        food,
        FoodObservation {
            inventory: InventoryId(23),
            carried_by_actor: true,
            ..food
        },
        FoodObservation {
            inventory: InventoryId(22),
            carried_by_actor: true,
            ..food
        },
        FoodObservation {
            inventory: InventoryId(1),
            carried_by_actor: true,
            accessible: false,
            ..food
        },
    ];
    let result = w.member_decision(
        &w.store,
        ID,
        MemberInput {
            hunger: 100,
            ..input(&m)
        },
        &f,
    );
    assert_eq!(
        result.attempts[0].action,
        MemberAction::Eat(InventoryId(22))
    );
    // An attempted, rejected meal never retries another stock in this pass.
    assert_eq!(
        result
            .attempts
            .into_iter()
            .next()
            .unwrap()
            .after_result(false),
        None
    );
}
#[test]
fn supply_order_waits_on_the_first_unknown_or_blocked_request() {
    let mut w = world(MissionObjective::Visit { destination: AWAY });
    let m = motives();
    let mut f = Facts {
        stocks: vec![stock(21)],
        ..Facts::default()
    };
    let mission = mission_mut(&mut w.store, ID).unwrap();
    mission.status = MissionStatus::Traveling;
    mission.proposal.supplies = vec![
        MissionSupply {
            carrier: A,
            source: InventoryId(20),
            quantity: Quantity(1000),
        },
        MissionSupply {
            carrier: A,
            source: InventoryId(21),
            quantity: Quantity(1000),
        },
    ];
    assert_eq!(
        w.member_decision(&w.store, ID, input(&m), &f).explanation,
        Some(MemberExplanation::SupplyPermission)
    );
    assert!(f.calls.borrow().is_empty());
    let mut absent = stock(20);
    absent.accessible = false;
    f.stocks.insert(0, absent);
    assert_eq!(
        w.member_decision(&w.store, ID, input(&m), &f).explanation,
        Some(MemberExplanation::SupplyRoute)
    );
    assert_eq!(*f.calls.borrow(), vec![InventoryId(20)]);
}
#[test]
fn actual_qualified_supply_continues_to_next_delivery_without_using_unrelated_cargo() {
    let mut w = world(MissionObjective::Visit { destination: AWAY });
    let m = motives();
    let mut f = Facts {
        stocks: vec![stock(20), stock(21)],
        ..Facts::default()
    };
    let mission = mission_mut(&mut w.store, ID).unwrap();
    mission.status = MissionStatus::Traveling;
    mission.proposal.supplies = vec![
        MissionSupply {
            carrier: A,
            source: InventoryId(20),
            quantity: Quantity(1000),
        },
        MissionSupply {
            carrier: A,
            source: InventoryId(21),
            quantity: Quantity(1000),
        },
    ];
    mission
        .members
        .get_mut(&A)
        .unwrap()
        .supply_cargo
        .insert(InventoryId(20), BTreeMap::from([(InventoryId(40), 1000)]));
    f.cargo = vec![Cargo {
        inventory: InventoryId(40),
        good: Good::Food,
        quantity: Quantity(1000),
        origin: Some(InventoryId(20)),
        origin_quantity: Quantity(0),
        room: Quantity(4000),
    }];
    let decision = w.member_decision(&w.store, ID, input(&m), &f);
    assert_eq!(*f.calls.borrow(), vec![InventoryId(20), InventoryId(21)]);
    assert_eq!(
        decision.attempts[0].action,
        MemberAction::Carry {
            source: InventoryId(21),
            quantity: Quantity(1000)
        }
    );
    assert_eq!(
        decision
            .attempts
            .into_iter()
            .next()
            .unwrap()
            .after_result(false),
        None
    );
}
#[test]
fn confinement_failure_fallback_and_release_deadline_are_owned() {
    let w = world(MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(10),
    });
    let m = motives();
    let mut f = Facts::default();
    let attempt = w
        .member_decision(&w.store, ID, input(&m), &f)
        .attempts
        .remove(0);
    assert_eq!(attempt.action, MemberAction::Restrain(ENEMY));
    assert_eq!(attempt.clone().after_result(true), None);
    assert_eq!(
        attempt.after_result(false),
        Some(Failure::Action(MemberAction::Engage(ENEMY)))
    );
    f.target.captor = Some(A);
    assert_eq!(
        w.member_decision(&w.store, ID, input(&m), &f).attempts[0].action,
        MemberAction::Rest
    );
    assert_eq!(
        w.member_decision(
            &w.store,
            ID,
            MemberInput {
                now: SimTime(10),
                ..input(&m)
            },
            &f
        )
        .attempts[0]
            .action,
        MemberAction::Free(ENEMY)
    );
    f.target.captor = Some(ENEMY);
    assert!(
        w.member_decision(&w.store, ID, input(&m), &f)
            .attempts
            .is_empty()
    );
}
#[test]
fn rescue_escort_precedes_member_travel_and_has_no_execution_retry() {
    let w = world(MissionObjective::Rescue {
        person: ENEMY,
        return_to: AWAY,
    });
    let m = motives();
    let mut f = Facts {
        observed: Some(TargetObservation {
            person: ENEMY,
            area: HOME,
            alive: true,
            health: 100,
            incapacitation: 10,
            captor: None,
            restricted: false,
            observed_at: SimTime(0),
        }),
        ..Facts::default()
    };
    let decision = w.member_decision(&w.store, ID, input(&m), &f);
    assert_eq!(
        decision.attempts[0].action,
        MemberAction::Move {
            actor: ENEMY,
            destination: AWAY
        }
    );
    assert_eq!(
        decision.attempts[1].action,
        MemberAction::Move {
            actor: A,
            destination: AWAY
        }
    );
    assert_eq!(decision.attempts[0].clone().after_result(false), None);
    assert_eq!(
        decision.attempts[1].clone().after_result(false),
        Some(Failure::Explain(MemberExplanation::RouteUnknown))
    );
    f.target.player = true;
    assert_eq!(
        w.member_decision(&w.store, ID, input(&m), &f)
            .attempts
            .len(),
        1
    );
}
#[test]
fn selected_delivery_is_admitted_by_real_cargo_without_assembly() {
    let w = world(MissionObjective::Deliver {
        source: InventoryId(20),
        destination: InventoryId(21),
        quantity: Quantity(1000),
    });
    let m = motives();
    let mut f = Facts {
        stocks: vec![stock(20)],
        destination: Some(KnownDestination {
            destination: Destination::Inventory(InventoryId(21)),
            location: Location::AtArea(HOME),
            observed_at: SimTime(0),
            accessible: true,
            room: Some(Quantity(5000)),
            direct_transfer: false,
        }),
        ..Facts::default()
    };
    let decision = w.member_decision(&w.store, ID, input(&m), &f);
    let MemberAction::Carry { source, quantity } = decision.attempts[0].action else {
        panic!("expected physical pickup")
    };
    let source_row = game_cargo::Inventory {
        id: source,
        location: game_cargo::InventoryLocation::Area(HOME),
        quantity: Quantity(5000),
        capacity: Quantity(5000),
        access: BTreeSet::from([A]),
        last_granted: None,
        good: Good::Food,
        spoils_at: None,
    };
    let destination = game_cargo::Inventory {
        id: InventoryId(40),
        location: game_cargo::InventoryLocation::Carried(A),
        quantity: Quantity(0),
        ..source_row.clone()
    };
    let actor = game_cargo::ActorFacts {
        actor: A,
        alive: true,
        health: 100,
        independent: true,
        confined: false,
        in_combat: false,
        receiving_care: false,
        stationary: true,
        fatigue: 0,
        fatigue_limit: 100,
    };
    let job = game_cargo::prepare_load(
        actor,
        &source_row,
        &destination,
        quantity,
        game_cargo::SourceFacts {
            contact: true,
            accessible: true,
            unreserved: source_row.quantity,
            record_permission: true,
        },
        game_cargo::ReceiverFacts {
            contact: true,
            accessible: true,
            accepted: false,
            body_room: Quantity(5000),
            transport_allows: true,
        },
        SimTime(0),
        SimDuration(5),
    )
    .unwrap();
    assert_eq!(job.source, InventoryId(20));
    assert_eq!(job.quantity, Quantity(1000));
    f.stocks.clear();
    assert_eq!(
        w.member_decision(&w.store, ID, input(&m), &f).explanation,
        Some(MemberExplanation::SourceUnknown)
    );
}

#[test]
fn production_member_executor_uses_restraint_failure_to_engage_and_preserves_success() {
    struct Execute {
        actions: Vec<MemberAction<u8>>,
        restraint_success: bool,
    }
    impl MemberExecution<u8> for Execute {
        fn action(&mut self, _: MissionId, _: PersonId, action: &MemberAction<u8>) -> bool {
            self.actions.push(action.clone());
            !matches!(action, MemberAction::Restrain(_)) || self.restraint_success
        }
        fn explain(&mut self, _: MissionId, _: PersonId, _: MemberExplanation) {
            panic!("unexpected explanation");
        }
    }
    let w = world(MissionObjective::Confine {
        person: ENEMY,
        until: SimTime(100),
    });
    let facts = Facts::default();
    for (restraint_success, expected) in [
        (
            false,
            vec![MemberAction::Restrain(ENEMY), MemberAction::Engage(ENEMY)],
        ),
        (true, vec![MemberAction::Restrain(ENEMY)]),
    ] {
        let decision = w.member_decision(&w.store, ID, input(&motives()), &facts);
        let mut port = Execute {
            actions: Vec::new(),
            restraint_success,
        };
        execute_member_decision(&mut port, ID, A, decision);
        assert_eq!(port.actions, expected);
    }
}

use super::*;
use std::cell::RefCell;
struct Providers {
    calls: RefCell<Vec<(Good, Quantity)>>,
    willing: bool,
}
impl AcquisitionSource<u8> for Providers {
    fn offers(&self, good: Good, quantity: Quantity, _: Quantity) -> Vec<Candidate<u8>> {
        self.calls.borrow_mut().push((good, quantity));
        vec![Candidate {
            willing: self.willing,
            offer: AcquisitionOffer {
                key: if good == Good::Food { 1 } else { 2 },
                good,
                source: None,
                location: Location::AtArea(AreaId(1)),
                observed_at: SimTime(0),
                accessible: true,
                quantity: OfferYield::Known(Quantity(1000)),
                requires_carrying_room: false,
                requirements: if good == Good::Food {
                    vec![Prerequisite {
                        requirement: Requirement {
                            good: Good::Tool,
                            quantity: Quantity(1),
                            source: None,
                        },
                        placement: PrerequisitePlacement::AccessibleAtProvider { inventory: None },
                    }]
                } else {
                    Vec::new()
                },
                action: if good == Good::Food { 1 } else { 2 },
            },
        }]
    }
}
fn facts(routes: &KnownRoutes) -> DeliveryFacts<'_, u8> {
    DeliveryFacts {
        now: SimTime(0),
        place: Location::AtArea(AreaId(1)),
        readiness: Readiness {
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
        },
        room: Quantity(3000),
        cargo: Vec::new(),
        stocks: Vec::new(),
        contacts: BTreeSet::from([PersonId(1)]),
        routes,
        destination: Some(KnownDestination {
            destination: Destination::Contact(PersonId(1)),
            location: Location::AtArea(AreaId(1)),
            observed_at: SimTime(0),
            accessible: true,
            room: None,
            direct_transfer: false,
        }),
        direct_transfer: false,
        additional: Vec::new(),
    }
}
fn delivery() -> Delivery {
    Delivery {
        requirement: Requirement {
            good: Good::Food,
            quantity: Quantity(1000),
            source: None,
        },
        destination: Destination::Contact(PersonId(1)),
        remaining: Quantity(1000),
        last_receipt: 0,
    }
}
#[test]
fn owner_expands_prerequisites_once_and_respects_character_willingness() {
    let routes = KnownRoutes::new();
    let source = Providers {
        calls: RefCell::new(Vec::new()),
        willing: true,
    };
    assert_eq!(
        delivery().plan_with_acquisition(facts(&routes), Quantity(1), &source),
        Step::Acquire(2)
    );
    assert_eq!(
        *source.calls.borrow(),
        vec![(Good::Food, Quantity(1000)), (Good::Tool, Quantity(1))]
    );
    let unwilling = Providers {
        calls: RefCell::new(Vec::new()),
        willing: false,
    };
    assert!(matches!(
        delivery().plan_with_acquisition(facts(&routes), Quantity(1), &unwilling),
        Step::Blocked(_)
    ));
}
#[test]
fn missing_rows_and_busy_readiness_preserve_old_planning_failures() {
    let routes = KnownRoutes::new();
    let source = Providers {
        calls: RefCell::new(Vec::new()),
        willing: true,
    };
    assert_eq!(
        delivery().plan_with_acquisition(
            DeliveryFacts::absent(false, false, false),
            Quantity(1),
            &source
        ),
        Step::Blocked(BlockReason::CannotTravel)
    );
    assert_eq!(
        delivery().plan_with_acquisition(
            DeliveryFacts::absent(true, true, false),
            Quantity(1),
            &source
        ),
        Step::Blocked(BlockReason::UnknownRoute)
    );
    let mut facts = facts(&routes);
    facts.readiness.carried = true;
    assert_eq!(
        delivery().plan_with_acquisition(facts, Quantity(1), &source),
        Step::Continue
    );
}

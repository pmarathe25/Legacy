use super::*;
use std::collections::BTreeMap;

fn needs() -> WorkNeeds {
    WorkNeeds {
        hunger: 0,
        meal_relief: 100,
        fatigue: 60,
        fatigue_limit: 100,
        health: 100,
        max_health: 100,
    }
}
fn symptoms() -> SymptomsView {
    SymptomsView {
        person: PersonId(4),
        fever: true,
        nausea: false,
        cough: false,
        shivering: false,
        wounded: false,
        observed_at: SimTime(10),
    }
}
fn facts() -> OfferReviewFacts {
    OfferReviewFacts {
        player_controlled: false,
        patient_present: true,
        caregiver_capable: true,
        contact: true,
        trust: Some(0),
    }
}
fn treatment(caregiver: u64, accepted: bool) -> Offer {
    Offer {
        caregiver: PersonId(caregiver),
        patient: PersonId(4),
        herbs: InventoryId(100),
        water: InventoryId(101),
        expires: SimTime(100),
        accepted,
    }
}

#[test]
fn review_deadlines_and_player_control_do_not_invent_a_pause_or_consume_work() {
    let mut owner = Runtime::default();
    assert!(!owner.review_due(SimTime(0)));
    assert!(!owner.review_due(SimTime(MINUTE.0 - 1)));
    assert!(owner.review_due(SimTime(MINUTE.0)));
    owner.record_review(SimTime(MINUTE.0));
    assert!(!owner.review_due(SimTime(MINUTE.0)));
    let mut store = component_store();
    for (actor, work) in [
        (
            PersonId(1),
            Work::Drink {
                source: InventoryId(100),
                quantity: Quantity(1),
            },
        ),
        (
            PersonId(2),
            Work::Boil {
                source: InventoryId(100),
                quantity: Quantity(1),
                tool: InventoryId(102),
            },
        ),
    ] {
        insert_job(
            &mut store,
            Job {
                actor,
                area: AreaId(1),
                completes: SimTime(100),
                work,
            },
        )
        .unwrap();
    }
    let opening = store.snapshot().unwrap();
    let candidates = owner.review_work(&store);
    assert_eq!(store.snapshot().unwrap(), opening);
    assert_eq!(
        candidates
            .iter()
            .map(|review| review.actor)
            .collect::<Vec<_>>(),
        vec![PersonId(1), PersonId(2)]
    );
    assert!(!candidates[0].should_pause(false, needs()));
    assert!(!candidates[1].should_pause(true, needs()));
    assert!(candidates[1].should_pause(false, needs()));
    remove_job(&mut store, PersonId(2)).unwrap();
    assert!(candidates[1].should_pause(false, needs()));
}

#[test]
fn care_responses_use_current_contact_and_trust_without_reading_unavailable_symptoms() {
    let owner = Runtime::default();
    let mut store = component_store();
    insert_offer(&mut store, treatment(2, false)).unwrap();
    let reviews = owner.review_offers(&store);
    let review = &reviews[0];
    for rejected in [
        OfferReviewFacts {
            player_controlled: true,
            ..facts()
        },
        OfferReviewFacts {
            patient_present: false,
            ..facts()
        },
        OfferReviewFacts {
            caregiver_capable: false,
            ..facts()
        },
        OfferReviewFacts {
            contact: false,
            ..facts()
        },
        OfferReviewFacts {
            trust: None,
            ..facts()
        },
    ] {
        assert_eq!(
            review.response(rejected, || panic!(
                "unavailable symptoms must not be queried"
            )),
            None
        );
    }
    assert_eq!(
        review.response(facts(), symptoms),
        Some(HealthAction::AcceptTreatment {
            caregiver: PersonId(2)
        })
    );
    assert_eq!(
        review.response(
            OfferReviewFacts {
                trust: Some(-101),
                ..facts()
            },
            symptoms
        ),
        Some(HealthAction::RefuseTreatment {
            caregiver: PersonId(2)
        })
    );
    assert!(!offer(&store, PersonId(2), PersonId(4)).unwrap().accepted);
}

#[test]
fn treatments_and_healer_selection_preserve_native_order_and_individual_availability() {
    let owner = Runtime::default();
    let mut store = component_store();
    for (actor, accepted) in [(3, false), (2, true), (1, true)] {
        insert_offer(&mut store, treatment(actor, accepted)).unwrap();
    }
    let instructions = owner.accepted_treatments(&store, PersonId(1));
    assert_eq!(
        instructions,
        vec![(
            PersonId(2),
            HealthAction::Treat {
                patient: PersonId(4),
                herbs: InventoryId(100),
                water: InventoryId(101),
            }
        )]
    );
    let mut presence_reads = Vec::new();
    let mut busy_reads = Vec::new();
    let healers = owner.review_healers(
        (1..=5).map(PersonId),
        |actor| HealerQualifications {
            is_ruler: actor == PersonId(1),
            medicine: if actor == PersonId(2) { 0 } else { 1 },
        },
        |actor| {
            presence_reads.push(actor);
            actor != PersonId(3)
        },
        |actor| {
            busy_reads.push(actor);
            actor == PersonId(4)
        },
    );
    assert_eq!(healers, vec![PersonId(5)]);
    assert_eq!(presence_reads, vec![PersonId(3), PersonId(4), PersonId(5)]);
    assert_eq!(busy_reads, vec![PersonId(4), PersonId(5)]);
    assert!(!offer(&store, PersonId(3), PersonId(4)).unwrap().accepted);
}

#[test]
fn patient_opportunities_keep_dated_memory_and_reborrow_offer_and_resting_facts() {
    let owner = Runtime::default();
    let mut store = component_store();
    insert_knowledge(
        &mut store,
        PersonId(2),
        Knowledge {
            people: BTreeMap::from([(PersonId(4), symptoms())]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    let opening = store.snapshot().unwrap();
    let patients = owner.remembered_patients(&store, PersonId(2), |_| true, |_| false);
    assert_eq!(store.snapshot().unwrap(), opening);
    assert_eq!(patients[0].symptoms.observed_at, SimTime(10));
    assert!(!patients[0].offer_exists);
    assert!(!patients[0].busy);
    insert_offer(&mut store, treatment(2, true)).unwrap();
    let patients = owner.remembered_patients(&store, PersonId(2), |_| false, |_| true);
    assert!(patients[0].offer_exists);
    assert!(patients[0].busy);
    assert!(!patients[0].contact);
    assert_eq!(patients[0].symptoms.observed_at, SimTime(10));
}

use game_assembly::Simulation;
use game_core::*;

fn mistaken_need(continuing: bool) -> (Simulation, PersonId, PersonId, InventoryId) {
    let mut s = Simulation::new("kernel-fixture", 17, 1).unwrap();
    let h = s.debug_host_mut();
    let carer = PersonId(h.get::<game_openings::Opening>().unwrap().names["guardian-1"]);
    let area = game_transport::area(h.get().unwrap(), carer).unwrap();
    let patient = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: patient,
        area,
    })
    .unwrap();
    h.dispatch(game_social_life::Register {
        person: patient,
        name: "Tired child".into(),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: carer,
        profile: [
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
            ("compassion".into(), 1000),
            ("intelligence".into(), 4000),
            ("foresight".into(), 6000),
            ("interpretation".into(), 2000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: patient,
        profile: [("gesturing".into(), 0), ("vocalization".into(), 1000)].into(),
    })
    .unwrap();
    h.dispatch(game_physiology::Strain {
        person: patient,
        amount: 200,
    })
    .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(carer),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: carer,
        inventory: pack,
        recognized_by: [carer].into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: carer,
        inventory: pack,
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    let notice = h
        .dispatch(game_communication::Notice {
            observer: carer,
            person: patient,
        })
        .unwrap();
    assert_eq!(notice.guessed_need.as_deref(), Some("food"));
    assert!(notice.estimated_need >= 200);
    assert_eq!(
        h.get::<game_physiology::Bodies>().unwrap().people[&patient].hunger,
        0
    );
    let error = h
        .dispatch(game_physiology::Serve {
            caregiver: carer,
            patient,
            inventory: pack,
            good: game_cargo::FOOD,
            quantity: 10,
        })
        .unwrap_err();
    assert!(error.to_string().contains("offered food declined"));
    if continuing {
        h.dispatch(game_households::RememberFoodCare {
            caregiver: carer,
            intent: Some(game_households::FoodCare {
                beneficiary: patient,
                home: area,
                observed_need: 200,
                observed_at: h.now(),
                returning: true,
                plan: game_characters::Plan::Meal {
                    inventory: pack,
                    good: Some(game_cargo::FOOD),
                },
            }),
        })
        .unwrap();
    }
    (s, carer, patient, pack)
}

#[test]
fn declined_food_reconsiders_with_failure_evidence_for_new_and_continuing_care() {
    for continuing in [false, true] {
        let (mut s, carer, patient, pack) = mistaken_need(continuing);
        let mut loaded = Simulation::load(&s.save().unwrap(), 4).unwrap();
        for sim in [&mut s, &mut loaded] {
            let h = sim.debug_host_mut();
            let offers = h.dispatch(game_choice::Collect { person: carer }).unwrap();
            let offer = offers
                .into_iter()
                .find(|p| p.key == format!("care:food:{}", patient.0))
                .unwrap();
            assert_eq!(
                h.dispatch(game_choice::Run(offer.request)).unwrap(),
                game_choice::Outcome::Reconsider
            );
            assert!(
                !h.get::<game_households::Households>()
                    .unwrap()
                    .food_care
                    .contains_key(&carer)
            );
            assert_eq!(
                h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::FOOD),
                100
            );
            assert!(h.notices_since(0).any(|n| n.kind == "care.feeding-failed"));
            // Failed help leaves the actor free to perform another useful action.
            h.dispatch(game_physiology::Rest { person: carer }).unwrap();
            assert_eq!(h.get::<game_sim::Actions>().unwrap().0[&carer].kind, "rest");
        }
        assert_eq!(s.save().unwrap(), loaded.save().unwrap());
    }
}

#[test]
fn stale_food_reviews_reconsider_during_travel_and_after_road_interruption() {
    for continuing in [false, true] {
        for interrupted in [false, true] {
            let (mut s, carer, patient, pack) = mistaken_need(continuing);
            let h = s.debug_host_mut();
            h.dispatch(game_households::Foster {
                caregiver: carer,
                child: patient,
            })
            .unwrap();
            let area = game_transport::area(h.get().unwrap(), carer).unwrap();
            let destination = *h.get::<game_land::Land>().unwrap().0[&area]
                .links
                .keys()
                .next()
                .unwrap();
            h.dispatch(game_transport::Move {
                person: carer,
                destination,
            })
            .unwrap();
            if interrupted {
                h.dispatch(game_transport::Cancel { person: carer })
                    .unwrap();
            }
            let before = h.get::<game_transport::Positions>().unwrap().0[&carer].clone();
            let saved = s.save().unwrap();
            let mut loaded = Simulation::load(&saved, 4).unwrap();
            for sim in [&mut s, &mut loaded] {
                let h = sim.debug_host_mut();
                // Exercise the same saved callback that can outlive its local opportunity.
                let review = serde_json::from_value(serde_json::json!({
                    "key": "care.food-review", "payload": {"caregiver": carer}
                }))
                .unwrap();
                assert_eq!(
                    h.dispatch(game_choice::Run(review)).unwrap(),
                    game_choice::Outcome::Reconsider
                );
                h.dispatch(game_households::TendFood { caregiver: carer })
                    .unwrap();
                assert_eq!(
                    h.get::<game_transport::Positions>().unwrap().0[&carer],
                    before
                );
                assert_eq!(
                    h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::FOOD),
                    100
                );
                assert_eq!(
                    h.get::<game_households::Households>()
                        .unwrap()
                        .food_care
                        .contains_key(&carer),
                    continuing
                );
            }
            assert_eq!(s.save().unwrap(), loaded.save().unwrap());
        }
    }
}

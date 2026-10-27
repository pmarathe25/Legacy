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
fn observed_refusal_is_remembered_without_repeated_same_food_offers_after_reload() {
    for continuing in [false, true] {
        let (mut s, carer, patient, pack) = mistaken_need(continuing);
        let h = s.debug_host_mut();
        h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
            15 * MINUTE.0,
        )))
        .unwrap();
        let offer = h
            .dispatch(game_choice::Collect { person: carer })
            .unwrap()
            .into_iter()
            .find(|p| p.key == format!("care:food:{}", patient.0))
            .unwrap();
        assert_eq!(
            h.dispatch(game_choice::Run(offer.request)).unwrap(),
            game_choice::Outcome::Reconsider
        );
        let group = &h
            .get::<game_households::Households>()
            .unwrap()
            .food_refusals[&carer][&patient];
        assert!(group.foods.contains_key(&game_cargo::FOOD));
        assert!(group.retry.is_some());
        let mut loaded = Simulation::load(&s.save().unwrap(), 4).unwrap();
        for sim in [&mut s, &mut loaded] {
            let h = sim.debug_host_mut();
            let failures = |h: &game_sim::Host| {
                h.notices()
                    .filter(|n| {
                        n.kind == "care.feeding-failed"
                            && n.data["caregiver"] == carer.0
                            && n.data["patient"] == patient.0
                            && n.data["good"] == game_cargo::FOOD.0
                    })
                    .count()
            };
            let before = failures(h);
            if let Some(offer) = h
                .dispatch(game_choice::Collect { person: carer })
                .unwrap()
                .into_iter()
                .find(|p| p.key == format!("care:food:{}", patient.0))
            {
                h.dispatch(game_choice::Run(offer.request)).unwrap();
            }
            assert_eq!(failures(h), before);
            assert_eq!(
                h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(game_cargo::FOOD),
                100
            );
        }
        assert_eq!(s.save().unwrap(), loaded.save().unwrap());
    }
}

#[test]
fn a_changed_serving_size_is_not_hidden_by_a_prior_food_refusal() {
    let (mut s, carer, patient, pack) = mistaken_need(false);
    let h = s.debug_host_mut();
    h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
        15 * MINUTE.0,
    )))
    .unwrap();
    h.dispatch(game_care::Assist {
        caregiver: carer,
        beneficiary: patient,
    })
    .unwrap();
    let refusal = h
        .get::<game_households::Households>()
        .unwrap()
        .food_refusals[&carer][&patient]
        .foods[&game_cargo::FOOD]
        .iter()
        .find(|refusal| refusal.quantity > 1)
        .unwrap()
        .clone();
    assert!(refusal.quantity > 1);
    h.dispatch(game_cargo::Consume {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 99,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: carer,
        inventory: pack,
    })
    .unwrap();
    let area = game_transport::area(h.get().unwrap(), carer).unwrap();
    h.dispatch(game_households::RememberFoodCare {
        caregiver: carer,
        intent: Some(game_households::FoodCare {
            beneficiary: patient,
            home: area,
            observed_need: refusal.observed_need,
            observed_at: h.now(),
            returning: true,
            plan: game_characters::Plan::Meal {
                inventory: pack,
                good: Some(game_cargo::FOOD),
            },
        }),
    })
    .unwrap();
    let sequence = h.notices().last().map_or(0, |notice| notice.sequence);
    h.dispatch(game_households::TendFood { caregiver: carer })
        .unwrap();
    assert!(
        h.notices_since(sequence)
            .any(
                |n| (n.kind == "care.feeding-failed" || n.kind == "physiology.fed")
                    && n.data["caregiver"] == carer.0
                    && n.data["patient"] == patient.0
                    && n.data["good"] == game_cargo::FOOD.0
                    && n.data["quantity"] == 1
            )
    );
}

#[test]
fn newly_observed_alternative_food_can_be_served_after_a_refusal() {
    let (mut s, carer, patient, pack) = mistaken_need(false);
    {
        let h = s.debug_host_mut();
        h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
            15 * MINUTE.0,
        )))
        .unwrap();
        h.dispatch(game_care::Assist {
            caregiver: carer,
            beneficiary: patient,
        })
        .unwrap();
        assert!(
            h.get::<game_households::Households>()
                .unwrap()
                .food_refusals[&carer][&patient]
                .foods
                .contains_key(&game_cargo::FOOD)
        );
        h.dispatch(game_physiology::Exert {
            person: patient,
            energy: 50_000,
            fatigue: 0,
        })
        .unwrap();
    }
    let candidates: Vec<_> = s
        .debug_host()
        .get::<game_objects::Objects>()
        .unwrap()
        .food_rules()
        .keys()
        .copied()
        .filter(|good| *good != game_cargo::FOOD)
        .collect();
    let saved = s.save().unwrap();
    let alternate = candidates
        .into_iter()
        .find(|good| {
            let mut probe = Simulation::load(&saved, 1).unwrap();
            let h = probe.debug_host_mut();
            if h.dispatch(game_cargo::Deposit {
                inventory: pack,
                good: *good,
                quantity: 1,
            })
            .is_err()
            {
                return false;
            }
            matches!(
                h.dispatch(game_physiology::TryServe(game_physiology::Serve {
                    caregiver: carer,
                    patient,
                    inventory: pack,
                    good: *good,
                    quantity: 1,
                })),
                Ok(game_physiology::OfferOutcome::Accepted)
            )
        })
        .expect("fixture needs another accepted single-unit food");
    let h = s.debug_host_mut();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: alternate,
        quantity: 1,
    })
    .unwrap();
    h.dispatch(game_knowledge::ObserveStore {
        person: carer,
        inventory: pack,
    })
    .unwrap();
    let sequence = h.notices().last().map_or(0, |notice| notice.sequence);
    h.dispatch(game_care::Assist {
        caregiver: carer,
        beneficiary: patient,
    })
    .unwrap();
    assert!(h.notices_since(sequence).any(|n| n.kind == "physiology.fed"
        && n.data["caregiver"] == carer.0
        && n.data["patient"] == patient.0
        && n.data["good"] == alternate.0));
}

#[test]
fn refusal_retry_expires_and_replays_after_reload() {
    let (mut s, carer, patient, _) = mistaken_need(false);
    let h = s.debug_host_mut();
    h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
        15 * MINUTE.0,
    )))
    .unwrap();
    h.dispatch(game_care::Assist {
        caregiver: carer,
        beneficiary: patient,
    })
    .unwrap();
    let (at, retry) = h
        .get::<game_households::Households>()
        .unwrap()
        .food_refusals[&carer][&patient]
        .retry
        .unwrap();
    assert!(h.scheduled().any(|job| job.id == retry));
    let mut loaded = Simulation::load(&s.save().unwrap(), 4).unwrap();
    for sim in [&mut s, &mut loaded] {
        let h = sim.debug_host_mut();
        while h.advance(at, 1000).unwrap().stop == game_sim::Stop::Budget {}
        assert!(!h.scheduled().any(|job| job.id == retry));
    }
    assert_eq!(s.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn refusals_for_two_serving_sizes_do_not_overwrite_each_other() {
    for continuing in [false, true] {
        let (mut s, carer, patient, _) = mistaken_need(continuing);
        let h = s.debug_host_mut();
        let second = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(carer),
                capacity: 1000,
            })
            .unwrap();
        h.dispatch(game_property::ClaimInventory {
            claimant: carer,
            inventory: second,
            recognized_by: [carer].into(),
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory: second,
            good: game_cargo::FOOD,
            quantity: 5,
        })
        .unwrap();
        h.dispatch(game_knowledge::ObserveStore {
            person: carer,
            inventory: second,
        })
        .unwrap();
        h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
            15 * MINUTE.0,
        )))
        .unwrap();
        let offer = h
            .dispatch(game_choice::Collect { person: carer })
            .unwrap()
            .into_iter()
            .find(|offer| offer.key == format!("care:food:{}", patient.0))
            .unwrap();
        h.dispatch(game_choice::Run(offer.request)).unwrap();
        let group = &h
            .get::<game_households::Households>()
            .unwrap()
            .food_refusals[&carer][&patient];
        let quantities: std::collections::BTreeSet<_> = group.foods[&game_cargo::FOOD]
            .iter()
            .map(|r| r.quantity)
            .collect();
        assert!(
            quantities.len() >= 2,
            "expected separate refused serving sizes, got {quantities:?}"
        );
        let before = h
            .notices()
            .filter(|n| {
                n.kind == "care.feeding-failed"
                    && n.data["caregiver"] == carer.0
                    && n.data["patient"] == patient.0
                    && n.data["good"] == game_cargo::FOOD.0
            })
            .count();
        h.dispatch(game_care::Assist {
            caregiver: carer,
            beneficiary: patient,
        })
        .unwrap();
        let after = h
            .notices()
            .filter(|n| {
                n.kind == "care.feeding-failed"
                    && n.data["caregiver"] == carer.0
                    && n.data["patient"] == patient.0
                    && n.data["good"] == game_cargo::FOOD.0
            })
            .count();
        assert_eq!(before, after);
    }
}

#[test]
fn observed_action_end_and_new_urgency_reopen_food_without_reopening_persistent_urgency() {
    let (mut s, carer, patient, _) = mistaken_need(false);
    let h = s.debug_host_mut();
    h.dispatch(game_households::ConfigureFoodRefusals(SimDuration(
        15 * MINUTE.0,
    )))
    .unwrap();
    h.dispatch(game_care::Assist {
        caregiver: carer,
        beneficiary: patient,
    })
    .unwrap();
    let records: Vec<_> = h
        .get::<game_households::Households>()
        .unwrap()
        .food_refusals[&carer][&patient]
        .foods
        .iter()
        .flat_map(|(good, rows)| rows.iter().map(move |refusal| (*good, refusal.clone())))
        .collect();
    assert!(!records.is_empty());
    for (good, mut refusal) in records.clone() {
        refusal.urgent = true;
        h.dispatch(game_households::RememberFoodRefusal {
            caregiver: carer,
            patient,
            good,
            refusal,
        })
        .unwrap();
    }
    h.dispatch(game_households::ClearNewFoodUrgency {
        caregiver: carer,
        patient,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_households::Households>()
            .unwrap()
            .food_refusals[&carer][&patient]
            .foods
            .values()
            .map(Vec::len)
            .sum::<usize>(),
        records.len()
    );
    let (good, mut refusal) = records[0].clone();
    let quantity = refusal.quantity;
    refusal.urgent = false;
    h.dispatch(game_households::RememberFoodRefusal {
        caregiver: carer,
        patient,
        good,
        refusal,
    })
    .unwrap();
    h.dispatch(game_households::ClearNewFoodUrgency {
        caregiver: carer,
        patient,
    })
    .unwrap();
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .food_refusals
            .get(&carer)
            .and_then(|patients| patients.get(&patient))
            .and_then(|group| group.foods.get(&good))
            .is_some_and(|rows| rows.iter().any(|refusal| refusal.quantity == quantity))
    );
    // An observed completion invalidates only records tied to that particular action.
    let urgent = records.iter().any(|(_, refusal)| refusal.urgent);
    for (good, mut refusal) in records {
        refusal.action = Some(777);
        h.dispatch(game_households::RememberFoodRefusal {
            caregiver: carer,
            patient,
            good,
            refusal,
        })
        .unwrap();
    }
    assert_eq!(
        h.dispatch(game_households::ClearObservedFoodAction {
            patient,
            action: 777
        })
        .unwrap(),
        vec![(carer, urgent)]
    );
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .food_refusals
            .get(&carer)
            .and_then(|patients| patients.get(&patient))
            .is_none()
    );
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

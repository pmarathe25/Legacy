use game_assembly::Simulation;
use game_core::*;

#[test]
fn local_care_owner_batch_preserves_production_observation_hooks_and_reload() {
    let mut initial = Simulation::new("kernel-fixture", 27, 1).unwrap();
    let h = initial.debug_host_mut();
    let names = h.get::<game_openings::Opening>().unwrap().names.clone();
    let mut meals = Vec::new();
    for name in ["guardian-1", "guardian-2"] {
        let caregiver = PersonId(names[name]);
        let area = game_transport::area(h.get().unwrap(), caregiver).unwrap();
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
            name: name.into(),
        })
        .unwrap();
        h.dispatch(game_physiology::Exert {
            person: patient,
            energy: 80_000,
            fatigue: 0,
        })
        .unwrap();
        let inventory = h
            .dispatch(game_cargo::Create {
                custody: game_cargo::Custody::Carried(caregiver),
                capacity: 100,
            })
            .unwrap();
        h.dispatch(game_property::ClaimInventory {
            claimant: caregiver,
            inventory,
            recognized_by: [caregiver].into(),
        })
        .unwrap();
        h.dispatch(game_cargo::Deposit {
            inventory,
            good: game_cargo::FOOD,
            quantity: 20,
        })
        .unwrap();
        meals.push(game_physiology::Serve {
            caregiver,
            patient,
            inventory,
            good: game_cargo::FOOD,
            quantity: 10,
        });
    }
    h.dispatch(game_communication::Enable).unwrap();
    let save = initial.save().unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut sim = Simulation::load(&save, workers).unwrap();
        let host = sim.debug_host_mut();
        let before: Vec<_> = meals
            .iter()
            .map(|m| {
                host.get::<game_inquiry::Inquiry>()
                    .unwrap()
                    .people
                    .get(&m.patient)
                    .map_or(0, |e| e.count)
            })
            .collect();
        assert_eq!(
            host.dispatch(game_physiology::ServeLocalMeals {
                meals: meals.clone(),
                minimum_parallel: 2,
            })
            .unwrap(),
            vec![game_physiology::OfferOutcome::Accepted; 2]
        );
        let observations = host.get::<game_observations::Observations>().unwrap();
        for (meal, count) in meals.iter().zip(before) {
            assert!(!observations.care.contains_key(&meal.patient));
            assert!(!observations.meals.contains_key(&meal.patient));
            assert!(
                host.get::<game_inquiry::Inquiry>()
                    .unwrap()
                    .people
                    .get(&meal.patient)
                    .is_some_and(|e| e.count >= count + 4),
                "care relief plus tasting evidence must survive"
            );
            assert_eq!(
                host.get::<game_cargo::Cargo>().unwrap().0[&meal.inventory]
                    .amount(game_cargo::FOOD),
                10
            );
        }
        let after = sim.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&after, expected);
        } else {
            reference = Some(after);
        }
    }
}

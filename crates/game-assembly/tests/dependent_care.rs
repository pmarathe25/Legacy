use game_assembly::Simulation;
use game_core::*;

fn run(s: &mut Simulation, at: SimTime) {
    while s.advance(at, 128).unwrap().stop == game_sim::Stop::Budget {}
}

#[test]
fn a_motivated_carer_returns_to_remembered_need_after_separation_and_loading() {
    let mut s = Simulation::new("kernel-fixture", 41, 1).unwrap();
    let h = s.debug_host_mut();
    let caregiver = PersonId(h.get::<game_openings::Opening>().unwrap().names["guardian-1"]);
    let home = game_transport::area(h.get().unwrap(), caregiver).unwrap();
    let child = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: child,
        area: home,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: caregiver,
        profile: [
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
            ("compassion".into(), 1000),
            ("foresight".into(), 6000),
            ("intelligence".into(), 4000),
        ]
        .into(),
    })
    .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(caregiver),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: caregiver,
        inventory: pack,
        recognized_by: [caregiver].into(),
    })
    .unwrap();
    let grain = h
        .get::<game_objects::Objects>()
        .unwrap()
        .resolve("grain")
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: grain,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    run(&mut s, SimTime(20 * MINUTE.0));
    s.debug_host_mut()
        .dispatch(game_physiology::Rest { person: child })
        .unwrap();
    run(&mut s, SimTime(21 * MINUTE.0));
    let h = s.debug_host_mut();
    let impression = h
        .dispatch(game_communication::Notice {
            observer: caregiver,
            person: child,
        })
        .unwrap();
    assert_eq!(impression.guessed_need.as_deref(), Some("food"));
    h.dispatch(game_transport::Move {
        person: caregiver,
        destination: AreaId(2),
    })
    .unwrap();
    run(&mut s, SimTime(21 * MINUTE.0 + 2 * MINUTE.0));
    let mut loaded = Simulation::load(&s.save().unwrap(), 4).unwrap();
    for sim in [&mut s, &mut loaded] {
        sim.debug_host_mut()
            .dispatch(game_characters::Abandon { person: child })
            .unwrap();
        sim.debug_host_mut()
            .dispatch(game_choice::Undertake {
                person: child,
                kind: "express-need".into(),
                duration: HOUR,
                request: game_sim::Pending::new(game_knowledge::ObserveSurroundings {
                    person: child,
                })
                .unwrap(),
            })
            .unwrap();
        assert!(
            sim.debug_host_mut()
                .dispatch(game_care::Assist {
                    caregiver,
                    beneficiary: child
                })
                .unwrap()
        );
        run(sim, SimTime(21 * MINUTE.0 + 5 * MINUTE.0));
        let h = sim.debug_host();
        assert_eq!(
            game_transport::area(h.get().unwrap(), caregiver).unwrap(),
            home
        );
        assert!(h.get::<game_physiology::Bodies>().unwrap().people[&child].hunger < 6);
        assert!(h.get::<game_cargo::Cargo>().unwrap().0[&pack].amount(grain) < 20);
    }
    assert_eq!(s.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn exceptional_newborn_potential_does_not_enable_adult_physical_actions() {
    let mut s = Simulation::new("kernel-fixture", 19, 1).unwrap();
    let h = s.debug_host_mut();
    let patch = h.get::<game_openings::Opening>().unwrap().names["patch"];
    let area = h.get::<game_ecology::Ecology>().unwrap().patches[&patch].area;
    let child = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: child,
        area,
    })
    .unwrap();
    h.dispatch(game_vitality::Register {
        person: child,
        born: h.now(),
        span: Some(SimDuration(80 * 365 * DAY.0)),
        base_health: 100,
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person: child,
        profile: [
            ("agility".into(), 16000),
            ("speed".into(), 16000),
            ("strength".into(), 16000),
            ("intelligence".into(), 16000),
        ]
        .into(),
    })
    .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(child),
            capacity: 2000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: child,
        inventory,
        recognized_by: [child].into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    let recipe = h
        .get::<game_workshop::Workshop>()
        .unwrap()
        .recipes
        .keys()
        .next()
        .unwrap()
        .clone();
    let before = h.save().unwrap();
    let movement = h.dispatch(game_transport::Move {
        person: child,
        destination: AreaId(1),
    });
    assert!(movement.unwrap_err().to_string().contains("walk"));
    let gathering = h.dispatch(game_ecology::Gather {
        person: child,
        patch,
        quantity: 1,
        destination: inventory,
    });
    assert!(gathering.unwrap_err().to_string().contains("locomotion"));
    let eating = h.dispatch(game_physiology::ConsumeEdible {
        person: child,
        inventory,
        good: game_cargo::FOOD,
        quantity: 1,
    });
    assert!(eating.unwrap_err().to_string().contains("motor"));
    let crafting = h.dispatch(game_workshop::Craft {
        person: child,
        inventory,
        recipe,
    });
    assert!(crafting.unwrap_err().to_string().contains("manipulate"));
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn returning_to_a_dead_dependent_retires_the_intent_without_rejecting_the_chooser() {
    let mut s = Simulation::new("kernel-fixture", 7, 1).unwrap();
    let h = s.debug_host_mut();
    let parent = PersonId(h.get::<game_openings::Opening>().unwrap().names["guardian-1"]);
    let home = game_transport::area(h.get().unwrap(), parent).unwrap();
    let child = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place {
        person: child,
        area: home,
    })
    .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(parent),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_households::RememberFoodCare {
        caregiver: parent,
        intent: Some(game_households::FoodCare {
            beneficiary: child,
            home,
            observed_need: 60,
            observed_at: h.now(),
            returning: true,
            plan: game_characters::Plan::Meal {
                inventory: pack,
                good: Some(game_cargo::FOOD),
            },
        }),
    })
    .unwrap();
    h.dispatch(game_physiology::Damage {
        person: child,
        amount: 10000,
    })
    .unwrap();
    h.dispatch(game_choice::Collect { person: parent }).unwrap();
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .food_care
            .contains_key(&parent)
    );
}

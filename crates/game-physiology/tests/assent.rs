mod support;
use game_core::*;
use game_physiology::*;
use support::*;

#[test]
fn serving_yourself_cannot_bypass_a_committed_action() {
    let mut f = setup();
    f.host.dispatch(Rest { person: f.ruler }).unwrap();
    let before = f.host.save().unwrap();
    let served = f.host.dispatch(Serve {
        caregiver: f.ruler,
        patient: f.ruler,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 10,
    });
    let consumed = f.host.dispatch(ConsumeEdible {
        person: f.ruler,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 10,
    });
    assert_eq!(
        served.unwrap_err().to_string(),
        consumed.unwrap_err().to_string()
    );
    assert_eq!(before, f.host.save().unwrap());
}

#[test]
fn recipient_considers_the_offered_dose_and_can_decline_without_effects() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(35 * MINUTE.0));
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Serve {
                caregiver: f.ruler,
                patient: f.other,
                inventory: f.store,
                good: game_cargo::FOOD,
                quantity: 1000,
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
    f.host
        .dispatch(Serve {
            caregiver: f.ruler,
            patient: f.other,
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1,
        })
        .unwrap();
    assert_eq!(food(&f.host, f.store), 999);
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.other].hunger, 29);
}

#[test]
fn feeding_cannot_silently_interrupt_a_recipients_journey() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(35 * MINUTE.0));
    f.host
        .dispatch(game_transport::Move {
            person: f.other,
            destination: f.away,
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Serve {
                caregiver: f.ruler,
                patient: f.other,
                inventory: f.store,
                good: game_cargo::FOOD,
                quantity: 1,
            })
            .is_err()
    );
    assert_eq!(before, f.host.save().unwrap());
}

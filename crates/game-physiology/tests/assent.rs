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
fn typed_offer_reports_actual_refusal_and_preserves_successful_serving() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(35 * MINUTE.0));
    let large = Serve {
        caregiver: f.ruler,
        patient: f.other,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 1000,
    };
    let stock = food(&f.host, f.store);
    let body = f.host.get::<Bodies>().unwrap().people[&f.other];
    let notices = f.host.notices().count();
    assert_eq!(
        f.host.dispatch(TryServe(large)).unwrap(),
        OfferOutcome::Declined
    );
    assert_eq!(food(&f.host, f.store), stock);
    assert_eq!(f.host.get::<Bodies>().unwrap().people[&f.other], body);
    assert_eq!(f.host.notices().count(), notices);
    let before = f.host.save().unwrap();
    let mut loaded = builder().load(&before, 4).unwrap();
    let portion = Serve {
        caregiver: f.ruler,
        patient: f.other,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 1,
    };
    assert_eq!(
        f.host.dispatch(TryServe(portion.clone())).unwrap(),
        OfferOutcome::Accepted
    );
    loaded.dispatch(portion).unwrap();
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}

#[test]
fn typed_offer_runs_serve_hooks_and_rolls_them_back_on_refusal() {
    let mut f = setup();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(35 * MINUTE.0));
    let saved = f.host.save().unwrap();
    let hooked = || {
        let mut b = builder();
        b.before::<Serve>("test.hook", |tx, r| {
            tx.allocate()?;
            tx.schedule(
                tx.now().after(MINUTE)?,
                20,
                Eat {
                    person: r.caregiver,
                    inventory: r.inventory,
                },
            )?;
            Ok(())
        })
        .unwrap();
        b
    };
    let mut h = hooked().load(&saved, 1).unwrap();
    let mut ordinary = hooked().load(&saved, 4).unwrap();
    let rejected = Serve {
        caregiver: f.ruler,
        patient: f.other,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 1000,
    };
    let before: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    assert_eq!(
        h.dispatch(TryServe(rejected)).unwrap(),
        OfferOutcome::Declined
    );
    let after: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    for key in ["resources", "jobs", "next_id", "next_notice", "notices"] {
        assert_eq!(before[key], after[key], "{key}");
    }
    let accepted = Serve {
        caregiver: f.ruler,
        patient: f.other,
        inventory: f.store,
        good: game_cargo::FOOD,
        quantity: 1,
    };
    assert_eq!(
        h.dispatch(TryServe(accepted.clone())).unwrap(),
        OfferOutcome::Accepted
    );
    ordinary.dispatch(accepted).unwrap();
    let mut actual: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(&ordinary.save().unwrap()).unwrap();
    actual["revision"] = serde_json::Value::Null;
    expected["revision"] = serde_json::Value::Null;
    assert_eq!(actual, expected);
}

#[test]
fn hook_error_with_decline_text_is_not_a_typed_food_refusal() {
    let f = setup();
    let saved = f.host.save().unwrap();
    let mut b = builder();
    b.before::<Serve>("test.reject", |_, _| {
        Err(Error::Rejected("offered food declined".into()))
    })
    .unwrap();
    let mut h = b.load(&saved, 1).unwrap();
    let before = h.save().unwrap();
    let error = h
        .dispatch(TryServe(Serve {
            caregiver: f.ruler,
            patient: f.other,
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1,
        }))
        .unwrap_err();
    assert_eq!(error, Error::Rejected("offered food declined".into()));
    assert_eq!(h.save().unwrap(), before);
}

#[test]
fn typed_offer_rejects_its_own_hooks_without_leaking_their_effects() {
    let f = setup();
    let saved = f.host.save().unwrap();
    let mut b = builder();
    b.before::<TryServe>("test.typed-hook", |tx, _| {
        tx.allocate()?;
        Ok(())
    })
    .unwrap();
    let mut h = b.load(&saved, 1).unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(TryServe(Serve {
            caregiver: f.ruler,
            patient: f.other,
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1
        }))
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
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

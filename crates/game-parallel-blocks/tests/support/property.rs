use crate::Engine;
use game_parallel::{GoodsDirection, GoodsTransfer, StartWork, World};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/property.toml")).unwrap()
}
#[test]
fn stock_rights_gate_manual_actions_and_autonomous_work() {
    let mut source = world();
    let before = source.clone();
    let request = GoodsTransfer {
        person: 2,
        good: 1,
        quantity: 1,
        direction: GoodsDirection::Pickup,
    };
    assert!(source.transfer_goods(&[request]).is_err());
    assert_eq!(source, before);
    assert!(
        source
            .start_work(&[StartWork {
                id: 5,
                worker: 2,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert_eq!(source, before);
    let mut engine = Engine::compile(source.clone()).unwrap();
    let before = engine.export();
    assert!(engine.transfer_goods(&[request]).is_err());
    assert!(
        engine
            .start_work(&[StartWork {
                id: 5,
                worker: 2,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert_eq!(engine.export(), before);
    source.step().unwrap();
    engine.step().unwrap();
    assert_eq!(engine.export(), source);
    assert_eq!(
        source
            .production
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .map(|job| job.worker)
            .collect::<Vec<_>>(),
        vec![1]
    );
    let people = source
        .production
        .as_ref()
        .unwrap()
        .validate_people(&source.regions, &source.travelers, source.tick)
        .unwrap();
    source
        .property
        .as_mut()
        .unwrap()
        .grant(&people, 1, 2, 1, 1)
        .unwrap();
    engine.property().grant(1, 2, 1, 1).unwrap();
    source.step().unwrap();
    engine.step().unwrap();
    assert_eq!(engine.export(), source);
    assert!(
        source
            .production
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .any(|job| job.worker == 2)
    );
}
#[test]
fn property_transfer_requires_recipient_consent_and_survives_reload() {
    let mut engine = Engine::compile(world()).unwrap();
    let offer = engine.property().offer(1, 2, 1, 1).unwrap();
    let before = engine.export();
    assert!(engine.property().decide(offer, 1, true).is_err());
    assert_eq!(engine.export(), before);
    engine = Engine::compile(World::from_json(&serde_json::to_string(&before).unwrap()).unwrap())
        .unwrap();
    engine.property().decide(offer, 2, true).unwrap();
    let state = engine.export();
    assert_eq!(state.property.as_ref().unwrap().claims[0].owner, 2);
    assert!(state.property.as_ref().unwrap().offers.is_empty());
    assert!(
        engine
            .transfer_goods(&[GoodsTransfer {
                person: 1,
                good: 1,
                quantity: 1,
                direction: GoodsDirection::Pickup
            }])
            .is_err()
    );
    engine
        .transfer_goods(&[GoodsTransfer {
            person: 2,
            good: 1,
            quantity: 1,
            direction: GoodsDirection::Pickup,
        }])
        .unwrap();
    state.validate().unwrap();
}

#[test]
fn death_and_reincarnation_do_not_transfer_stock_title() {
    let mut source = world();
    let family =
        World::from_toml(include_str!("../../../../content/parallel/lineage.toml")).unwrap();
    source.regions = family.regions;
    source.routes = family.routes;
    source.families = family.families;
    source.lineage = family.lineage;
    source
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|member| member.id == 1)
        .unwrap()
        .age = 69;
    source.validate().unwrap();
    let mut engine = Engine::compile(source).unwrap();
    for _ in 0..7 {
        engine.step().unwrap();
    }
    let state = engine.export();
    assert_ne!(state.lineage.as_ref().unwrap().current, Some(1));
    assert_eq!(state.property.as_ref().unwrap().claims[0].owner, 1);
    let before = engine.export();
    assert!(engine.property().offer(1, 2, 1, 1).is_err());
    assert!(
        engine
            .transfer_goods(&[GoodsTransfer {
                person: 2,
                good: 1,
                quantity: 1,
                direction: GoodsDirection::Pickup
            }])
            .is_err()
    );
    assert_eq!(engine.export(), before);
}

#[test]
fn reassignment_requires_current_access_but_existing_reservations_survive_revocation() {
    let mut source = world();
    source.autowork = None;
    let mut engine = Engine::compile(source.clone()).unwrap();
    let start = StartWork {
        id: 11,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    };
    source.start_work(&[start]).unwrap();
    engine.start_work(&[start]).unwrap();
    let before = engine.export();
    assert!(source.reassign_work(11, 2).is_err());
    assert!(engine.reassign_work(11, 2).is_err());
    assert_eq!(engine.export(), before);
    let people = source
        .production
        .as_ref()
        .unwrap()
        .validate_people(&source.regions, &source.travelers, source.tick)
        .unwrap();
    source
        .property
        .as_mut()
        .unwrap()
        .grant(&people, 1, 2, 1, 1)
        .unwrap();
    engine.property().grant(1, 2, 1, 1).unwrap();
    source.reassign_work(11, 2).unwrap();
    engine.reassign_work(11, 2).unwrap();
    source
        .property
        .as_mut()
        .unwrap()
        .revoke(&people, 1, 2, 1, 1)
        .unwrap();
    engine.property().revoke(1, 2, 1, 1).unwrap();
    for _ in 0..2 {
        source.step().unwrap();
        engine.step().unwrap();
    }
    assert_eq!(engine.export(), source);
    assert!(source.production.as_ref().unwrap().jobs.is_empty());
}

#[test]
fn accepting_a_title_cancels_competing_offers() {
    let mut engine = Engine::compile(world()).unwrap();
    let first = engine.property().offer(1, 2, 1, 1).unwrap();
    let second = engine.property().offer(1, 2, 1, 1).unwrap();
    engine.property().decide(first, 2, true).unwrap();
    let before = engine.export();
    assert!(before.property.as_ref().unwrap().offers.is_empty());
    assert!(engine.property().decide(second, 2, true).is_err());
    assert!(engine.property().decide(second, 2, false).is_err());
    assert_eq!(engine.export(), before);
}

use game_cargo::{Cargo, Changed, Consume, Custody, Deposit, FOOD, StockBalance, Transfer};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

type Snapshot = (
    InventoryId,
    Option<InventoryId>,
    Option<StockBalance>,
    Option<StockBalance>,
);
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Observed(Vec<Snapshot>);
impl Resource for Observed {
    const KEY: &'static str = "test.stock-effects";
}

#[derive(Clone, Serialize, Deserialize)]
struct FailAfterConsume(InventoryId);
impl Request for FailAfterConsume {
    const KEY: &'static str = "test.fail-after-consume";
    type Response = ();
}

fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    b.plugin("stock-effect-test", 1).unwrap();
    b.resource::<Observed>().unwrap();
    b.subscribe::<Changed>("test.capture-stock-effect", |tx, e| {
        tx.edit::<Observed>()?
            .0
            .push((e.inventory, e.destination, e.stock, e.destination_stock));
        Ok(())
    })
    .unwrap();
    b.request::<FailAfterConsume>(|tx, r| {
        tx.call(Consume {
            inventory: r.0,
            good: FOOD,
            quantity: 1,
        })?;
        Err(Error::Rejected("abort outer transaction".into()))
    })
    .unwrap();
    b
}

#[test]
fn stock_effects_capture_each_balance_and_roll_back_with_the_operation() {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "store".into(),
        })
        .unwrap();
    let source = h
        .dispatch(game_cargo::Create {
            custody: Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    let destination = h
        .dispatch(game_cargo::Create {
            custody: Custody::Area(area),
            capacity: 1000,
        })
        .unwrap();
    h.dispatch(Deposit {
        inventory: source,
        good: FOOD,
        quantity: 20,
    })
    .unwrap();
    h.dispatch(Transfer {
        from: source,
        to: destination,
        good: FOOD,
        quantity: 7,
    })
    .unwrap();
    h.dispatch(Consume {
        inventory: destination,
        good: FOOD,
        quantity: 5,
    })
    .unwrap();
    assert_eq!(
        h.get::<Observed>().unwrap().0,
        vec![
            (
                source,
                None,
                Some(StockBalance {
                    before: 0,
                    after: 20
                }),
                None
            ),
            (
                source,
                Some(destination),
                Some(StockBalance {
                    before: 20,
                    after: 13
                }),
                Some(StockBalance {
                    before: 0,
                    after: 7
                })
            ),
            (
                destination,
                None,
                Some(StockBalance {
                    before: 7,
                    after: 2
                }),
                None
            ),
        ]
    );
    let reservation = h
        .dispatch(game_cargo::ReserveSpace {
            inventory: destination,
            good: FOOD,
            quantity: 3,
        })
        .unwrap();
    h.dispatch(game_cargo::DepositReserved {
        inventory: destination,
        reservation,
        good: FOOD,
        quantity: 3,
    })
    .unwrap();
    assert_eq!(
        h.get::<Observed>().unwrap().0.last(),
        Some(&(
            destination,
            None,
            Some(StockBalance {
                before: 2,
                after: 5
            }),
            None,
        ))
    );
    let too_small = h
        .dispatch(game_cargo::Create {
            custody: Custody::Area(area),
            capacity: 0,
        })
        .unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Transfer {
            from: source,
            to: too_small,
            good: FOOD,
            quantity: 1
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    assert!(h.dispatch(FailAfterConsume(source)).is_err());
    assert_eq!(h.save().unwrap(), before);
    assert_eq!(h.get::<Cargo>().unwrap().0[&source].amount(FOOD), 13);
    let loaded = builder().load(&before, 4).unwrap();
    assert_eq!(loaded.save().unwrap(), before);
}

#[test]
fn older_change_payloads_remain_readable() {
    let change: Changed = serde_json::from_str(
        r#"{"inventory":1,"destination":null,"good":1,"quantity":2,"added":false}"#,
    )
    .unwrap();
    assert_eq!(change.stock, None);
    assert_eq!(change.destination_stock, None);
}

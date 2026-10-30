use game_core::*;
use game_nutrition::*;
use game_objects::*;
fn food() -> Food {
    Food {
        portion: 5,
        relief: 20,
        shelf_days: Some(2),
        nutrients: [("energy".into(), 20), ("protein".into(), 12)].into(),
        effects: FoodEffects {
            health: -2,
            mood: 20,
            alertness: -10,
            substance: Some("test".into()),
            dependence: 90,
            duration: HOUR,
        },
    }
}
#[test]
fn actual_diet_depletes_over_time_and_different_nutrients_restore_different_reserves() {
    let needs = Nutrition::default().needs;
    let mut d = Diet::new(&needs, SimTime(0));
    assert_eq!(d.status(&needs, SimTime(20 * DAY.0)).quality, 714);
    d.consume(&needs, GoodId(1), &food(), 5, SimTime(20 * DAY.0));
    assert_eq!(d.intake["protein"], 12000);
    assert_eq!(d.intake["energy"], 20000);
    assert_eq!(d.status(&needs, SimTime(31 * DAY.0)).quality, 0);
}
#[test]
fn repeated_substance_consumption_builds_tolerance_and_withdrawal_then_fades() {
    let needs = Nutrition::default().needs;
    let mut d = Diet::new(&needs, SimTime(0));
    let f = food();
    let first = d.consume(&needs, GoodId(1), &f, 5, SimTime(0));
    let mut last = first.clone();
    for i in 1..10 {
        last = d.consume(&needs, GoodId(1), &f, 5, SimTime(i * MINUTE.0));
    }
    assert!(last.mood < first.mood);
    assert!(last.alertness > first.alertness);
    assert!(d.status(&needs, SimTime(DAY.0)).craving > 0);
    assert_eq!(d.status(&needs, SimTime(101 * DAY.0)).craving, 0);
    assert_eq!(d.learned[&GoodId(1)].effects.mood, last.mood);
    assert!(!d.learned.contains_key(&GoodId(2)));
}

#[path = "support/evaluation.rs"]
mod evaluation;
#[path = "support/transactions.rs"]
mod transactions;

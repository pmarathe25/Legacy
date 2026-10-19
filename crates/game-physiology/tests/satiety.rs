mod support;
use game_core::*;
use game_physiology::*;
#[test]
fn swallowed_food_has_persistent_fullness_but_does_not_forbid_another_meal() {
    let mut f = support::setup();
    for _ in 0..4 {
        f.host
            .dispatch(Eat {
                person: f.ruler,
                inventory: f.store,
            })
            .unwrap();
    }
    assert_eq!(
        f.host.get::<game_digestion::Digestion>().unwrap().0[&f.ruler].amount,
        4000
    );
    let saved = f.host.save().unwrap();
    let mut loaded = support::builder().load(&saved, 4).unwrap();
    for h in [&mut f.host, &mut loaded] {
        support::run(h, SimTime(6 * HOUR.0));
        h.dispatch(Eat {
            person: f.ruler,
            inventory: f.store,
        })
        .unwrap();
        assert_eq!(
            h.get::<game_digestion::Digestion>().unwrap().0[&f.ruler].amount,
            1000
        );
    }
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}

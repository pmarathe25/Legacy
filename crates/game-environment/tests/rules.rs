use game_core::*;
use game_environment::*;
use game_sim::*;
#[test]
fn invalid_climate_definitions_are_inert() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Configure(Climate {
            seasons: vec![Season {
                duration: SimDuration(0),
                warmth: 20,
                rain: 1
            }]
        }))
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    h.dispatch(Configure(Climate {
        seasons: vec![Season {
            duration: DAY,
            warmth: 20,
            rain: 1,
        }],
    }))
    .unwrap();
    assert_eq!(
        h.get::<Climate>()
            .unwrap()
            .at(SimTime(DAY.0 * 4))
            .unwrap()
            .warmth,
        20
    );
}

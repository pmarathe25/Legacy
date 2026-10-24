use game_core::*;
use game_expression::*;
const YEAR: u64 = 365 * DAY.0;
fn life() -> Life {
    Life {
        born: SimTime(0),
        span: Some(SimDuration(80 * YEAR)),
        base_health: 100,
        expressed: true,
        physical: 700,
        stress: 170,
        satisfaction: -30,
        updated: SimTime(2 * YEAR),
        upbringing: Upbringing {
            since: SimTime(YEAR),
            deprivation: YEAR * 300,
            distress: YEAR * 100,
        },
    }
}
#[test]
fn exact_input_reuse_matches_uncached_formulas_through_age_condition_and_raw_changes() {
    for person in [PersonId(1), PersonId(129), PersonId(1)] {
        for age in [
            0,
            YEAR,
            6 * YEAR,
            18 * YEAR,
            40 * YEAR,
            80 * YEAR,
            90 * YEAR,
        ] {
            for raw in [0, 333, 1000, 16000] {
                for variant in 0..4 {
                    let mut input = life();
                    match variant {
                        1 => {
                            input.stress = 900;
                            input.physical = 250;
                        }
                        2 => input.upbringing.distress += YEAR * 10,
                        3 => input.span = None,
                        _ => {}
                    }
                    for key in [
                        "strength",
                        "intelligence",
                        "attention",
                        "agility",
                        "learning",
                        "unknown",
                    ] {
                        for _ in 0..2 {
                            assert_eq!(
                                ability(&input, person, key, SimTime(age), raw),
                                input.ability(raw, key, SimTime(age))
                            );
                        }
                    }
                    for key in [
                        "patience",
                        "risk-tolerance",
                        "sociability",
                        "empathy",
                        "compassion",
                        "rigidity",
                        "unknown",
                    ] {
                        for potential in [raw as i64, -100] {
                            for _ in 0..2 {
                                assert_eq!(
                                    disposition(&input, person, key, SimTime(age), potential),
                                    input.disposition(potential, key, SimTime(age))
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn restored_inputs_and_other_threads_do_not_observe_cached_changes_from_another_view() {
    let original = life();
    let changed = Life {
        stress: 1000,
        ..original
    };
    let at = SimTime(10 * YEAR);
    let p = PersonId(7);
    let expected = original.disposition(500, "rigidity", at);
    assert_eq!(disposition(&original, p, "rigidity", at, 500), expected);
    assert_eq!(
        disposition(&changed, p, "rigidity", at, 500),
        changed.disposition(500, "rigidity", at)
    );
    assert_eq!(disposition(&original, p, "rigidity", at, 500), expected);
    std::thread::spawn(move || {
        assert_eq!(disposition(&original, p, "rigidity", at, 500), expected)
    })
    .join()
    .unwrap();
}

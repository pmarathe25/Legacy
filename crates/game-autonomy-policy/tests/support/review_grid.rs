use game_autonomy_policy::{AttentionRates, Rules};
use game_choice::Schedule;
use game_core::*;

#[test]
fn delayed_attention_keeps_the_next_periodic_opportunity_instead_of_skipping_it() {
    let schedule = Schedule {
        epoch: SimTime(7),
        ..Default::default()
    };
    let period = SimDuration(12 * HOUR.0);
    let mut rules = Rules {
        review: period,
        ..Default::default()
    };
    for floor in [1, MINUTE.0] {
        rules.attention = Some(AttentionRates {
            ordinary: SimDuration(floor),
            evidence: SimDuration(floor),
        });
        for offset in [0, 1, MINUTE.0, period.0 - 1] {
            assert_eq!(
                rules
                    .next_review_at(SimTime(7 + period.0 + offset), &schedule)
                    .unwrap(),
                SimTime(7 + 2 * period.0)
            );
        }
        assert_eq!(
            rules
                .next_review_at(SimTime(7 + 2 * period.0), &schedule)
                .unwrap(),
            SimTime(7 + 3 * period.0)
        );
    }
    rules.attention = None;
    assert_eq!(
        rules
            .next_review_at(SimTime(8 + period.0), &schedule)
            .unwrap(),
        SimTime(8 + 2 * period.0)
    );
    assert!(rules.next_review_at(SimTime(u64::MAX), &schedule).is_err());
}

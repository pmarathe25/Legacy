use crate::Opinions;
use game_social::Encounter;
use rayon::prelude::*;

/// Each participant interprets the same meeting through their own policy.
pub(super) fn prepare(opinions: &Opinions, encounters: &[Encounter]) -> Vec<((u64, u64), i128)> {
    if encounters.is_empty() {
        return Vec::new();
    }
    let mut personal: Vec<_> = opinions
        .sensitivities
        .par_iter()
        .filter_map(|row| row.meeting_trust.map(|reaction| (row.person, reaction)))
        .collect();
    let mut directed: Vec<_> = opinions
        .sensitivities
        .par_iter()
        .flat_map_iter(|row| {
            row.relationship_cases.iter().filter_map(move |case| {
                case.meeting_trust
                    .map(|reaction| ((row.person, case.target), reaction))
            })
        })
        .collect();
    let opening = i128::from(opinions.rules.encounter_gain);
    if opening == 0 && personal.is_empty() && directed.is_empty() {
        return Vec::new();
    }
    if personal.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
        personal.par_sort_unstable_by_key(|row| row.0);
    }
    if directed.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
        directed.par_sort_unstable_by_key(|row| row.0);
    }
    encounters
        .par_iter()
        .flat_map_iter(|encounter| {
            [(encounter.a, encounter.b), (encounter.b, encounter.a)]
                .into_iter()
                .filter_map(|pair| {
                    let pressured = encounter.is_pressured(pair.0);
                    let reaction = |profile: crate::MeetingTrust| {
                        if pressured {
                            profile.pressured
                        } else {
                            profile.regular
                        }
                    };
                    let delta = directed
                        .binary_search_by_key(&pair, |row| row.0)
                        .ok()
                        .and_then(|index| reaction(directed[index].1))
                        .or_else(|| {
                            personal
                                .binary_search_by_key(&pair.0, |row| row.0)
                                .ok()
                                .and_then(|index| reaction(personal[index].1))
                        })
                        .map_or(if pressured { 0 } else { opening }, i128::from);
                    (delta != 0).then_some((pair, delta))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_meeting_can_reduce_increase_or_leave_directed_trust_unchanged() {
        let scenario = serde_json::json!({
            "rules":{"minimum":-20,"maximum":20,"initial":0,"fulfilled_gain":1,"default_loss":1,"encounter_gain":7},
            "sensitivities":[
                {"person":1,"meeting_trust":{"regular":-4,"pressured":3},
                    "relationship_cases":[{"target":2,"meeting_trust":{"regular":0,"pressured":-1}}]},
                {"person":2,"meeting_trust":{"regular":4}}
            ]
        });
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    let mut opinions: Opinions = serde_json::from_value(scenario.clone()).unwrap();
                    let meeting = |tick, a, b, pressured| Encounter {
                        tick,
                        region: 1,
                        a,
                        b,
                        pressured,
                        also_pressured: None,
                    };
                    opinions.advance_with_social(&[], &[], &[meeting(1, 1, 2, None)], 1);
                    assert_eq!(opinions.index().trust(1, 2), 0);
                    assert_eq!(opinions.index().trust(2, 1), 4);
                    opinions.advance_with_social(
                        &[],
                        &[],
                        &[meeting(2, 1, 2, Some(1)), meeting(2, 1, 3, None)],
                        2,
                    );
                    assert_eq!(opinions.index().trust(1, 2), -1);
                    assert_eq!(opinions.index().trust(2, 1), 8);
                    assert_eq!(opinions.index().trust(1, 3), -4);
                    assert_eq!(opinions.index().trust(3, 1), 7);
                    opinions
                })
        };
        assert_eq!(run(1), run(4));
    }
}

//! Subjective choice among a new form and familiar personal names.
use super::{Meaning, Mind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Namesake {
    pub person: u64,
    pub value: i64,
    /// Signed per-thousand response to the speaker's own directed affinity.
    #[serde(default)]
    pub affinity_weight: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamingPolicy {
    pub idle: i64,
    #[serde(default)]
    pub act_on_tie: bool,
    /// None means this person declines the newly generated form.
    #[serde(default)]
    pub novel_value: Option<i64>,
    /// Signed cost for each known person with exactly the same form.
    #[serde(default)]
    pub confusion_weight: i32,
    #[serde(default)]
    pub namesakes: Vec<Namesake>,
}

impl NamingPolicy {
    pub(crate) fn valid(&self) -> bool {
        !self
            .namesakes
            .windows(2)
            .any(|rows| rows[0].person >= rows[1].person)
            && self.namesakes.iter().all(|row| row.person != 0)
    }
}

/// Return an independently chosen form, or None if this person prefers silence.
pub(crate) fn choose<F: Fn(u64, u64) -> i32 + Sync>(
    mind: &Mind,
    target: u64,
    novel: Vec<u8>,
    affinity: &F,
    seed: u64,
    tick: u64,
) -> Option<Vec<u8>> {
    let policy = mind.naming.as_ref()?;
    let confusion = |sounds: &[u8]| {
        mind.words
            .iter()
            .filter(|word| matches!(word.meaning, Meaning::Person(_)) && word.sounds == sounds)
            .count() as i128
    };
    let score = |value: i128, sounds: &[u8]| {
        value - i128::from(policy.confusion_weight) * confusion(sounds)
    };
    let mut candidates: Vec<_> = policy
        .novel_value
        .into_iter()
        .map(|value| {
            (
                score(i128::from(value), &novel),
                super::random(seed ^ tick ^ mind.person ^ target),
                0,
                novel.clone(),
            )
        })
        .collect();
    for namesake in &policy.namesakes {
        let Some(word) = mind
            .words
            .binary_search_by_key(&Meaning::Person(namesake.person), |word| word.meaning)
            .ok()
            .map(|index| &mind.words[index])
        else {
            continue;
        };
        if !mind.phonemes.is_empty()
            && word
                .sounds
                .iter()
                .any(|sound| mind.phonemes.binary_search(sound).is_err())
        {
            continue;
        }
        let value = i128::from(namesake.value)
            + i128::from(namesake.affinity_weight)
                * i128::from(affinity(mind.person, namesake.person))
                / 1000;
        candidates.push((
            score(value, &word.sounds),
            super::random(seed ^ tick ^ mind.person ^ target ^ namesake.person.rotate_left(17)),
            namesake.person,
            word.sounds.clone(),
        ));
    }
    candidates
        .into_iter()
        .filter(|row| {
            row.0 > i128::from(policy.idle)
                || (row.0 == i128::from(policy.idle) && policy.act_on_tie)
        })
        .max()
        .map(|row| row.3)
}

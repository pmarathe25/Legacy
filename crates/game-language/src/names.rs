//! Subjective choice among a new form and familiar personal names.
use super::{Meaning, Mind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Signed cost for each known name one sound away.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub near_confusion_weight: i32,
    #[serde(default)]
    pub namesakes: Vec<Namesake>,
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Form {
    sounds: u128,
    length: usize,
}

impl Form {
    fn new(sounds: &[u8]) -> Self {
        let mut packed = [0; 16];
        packed[..sounds.len()].copy_from_slice(sounds);
        Self {
            sounds: u128::from_le_bytes(packed),
            length: sounds.len(),
        }
    }

    fn without(self, position: usize) -> Self {
        let shift = position as u32 * 8;
        let prefix = self.sounds & ((1u128 << shift) - 1);
        let suffix = self.sounds.checked_shr(shift + 8).unwrap_or(0) << shift;
        Self {
            sounds: prefix | suffix,
            length: self.length - 1,
        }
    }
}

#[derive(Default)]
struct PersonalNames {
    exact: HashMap<Form, usize>,
    substitutions: HashMap<(usize, Form), usize>,
    longer: HashMap<Form, usize>,
}

impl PersonalNames {
    fn from_words(mind: &Mind, near: bool) -> Self {
        let mut names = Self::default();
        for word in mind
            .words
            .iter()
            .filter(|word| matches!(word.meaning, Meaning::Person(_)))
        {
            let form = Form::new(&word.sounds);
            *names.exact.entry(form).or_default() += 1;
            if near {
                for (position, &sound) in word.sounds.iter().enumerate() {
                    let shorter = form.without(position);
                    *names.substitutions.entry((position, shorter)).or_default() += 1;
                    // Deleting either of two adjacent identical sounds produces the same form.
                    if position == 0 || word.sounds[position - 1] != sound {
                        *names.longer.entry(shorter).or_default() += 1;
                    }
                }
            }
        }
        names
    }

    fn confusion(&self, sounds: &[u8], policy: &NamingPolicy) -> i128 {
        let form = Form::new(sounds);
        let exact = self.exact.get(&form).copied().unwrap_or(0) as i128;
        if policy.near_confusion_weight == 0 {
            return exact * i128::from(policy.confusion_weight);
        }
        let mut near = self.longer.get(&form).copied().unwrap_or(0) as i128;
        for (position, &sound) in sounds.iter().enumerate() {
            let shorter = form.without(position);
            // The substitution index also contains every exact match, once per position.
            near += self
                .substitutions
                .get(&(position, shorter))
                .copied()
                .unwrap_or(0) as i128
                - exact;
            if position == 0 || sounds[position - 1] != sound {
                near += self.exact.get(&shorter).copied().unwrap_or(0) as i128;
            }
        }
        exact * i128::from(policy.confusion_weight)
            + near * i128::from(policy.near_confusion_weight)
    }
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
    let names = (policy.confusion_weight != 0 || policy.near_confusion_weight != 0)
        .then(|| PersonalNames::from_words(mind, policy.near_confusion_weight != 0));
    let score = |value: i128, sounds: &[u8]| {
        value
            - names
                .as_ref()
                .map_or(0, |names| names.confusion(sounds, policy))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Word;

    #[test]
    fn similar_forms_count_people_once_and_keep_exact_matches_separate() {
        let mut mind = Mind::fresh(1);
        mind.words = [
            vec![0, 0],
            vec![0],
            vec![0, 0, 0],
            vec![0, 1],
            vec![1, 1],
            vec![0, 0],
        ]
        .into_iter()
        .enumerate()
        .map(|(index, sounds)| Word {
            meaning: Meaning::Person(index as u64 + 2),
            sounds,
        })
        .collect();
        let names = PersonalNames::from_words(&mind, true);
        let policy = NamingPolicy {
            idle: 0,
            act_on_tie: false,
            novel_value: None,
            confusion_weight: 10,
            near_confusion_weight: 5,
            namesakes: Vec::new(),
        };
        assert_eq!(names.confusion(&[0, 0], &policy), 35);
        assert_eq!(names.confusion(&[0], &policy), 25);
        let long = [255; 16];
        assert_eq!(Form::new(&long).without(15), Form::new(&long[..15]));
    }
}

//! Subjective associations from a noticed signal and a later experienced gift.
use super::{Interpretation, Signals};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningRule {
    pub good: u32,
    pub meaning: u32,
    pub enabled: bool,
    pub value: i64,
    #[serde(default)]
    pub replace: bool,
}

/// A real accepted transfer personally experienced by the observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiftEvidence {
    pub actor: u64,
    pub observer: u64,
    pub good: u32,
}

/// A learner's own inference, never a claim about the sender's intention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningEvent {
    pub tick: u64,
    pub observer: u64,
    pub actor: u64,
    pub form: u32,
    pub good: u32,
    pub meaning: u32,
    pub previous: Option<u32>,
}

/// Prepared changes are applied only after the evidence-producing transaction commits.
pub struct LearningDraft {
    events: Vec<LearningEvent>,
}

impl LearningDraft {
    /// The owner must still be the generation used to prepare this draft.
    pub fn apply_validated(self, owner: &mut Signals) {
        owner.minds.par_iter_mut().for_each(|mind| {
            let first = self
                .events
                .partition_point(|row| row.observer < mind.person);
            let last = self
                .events
                .partition_point(|row| row.observer <= mind.person);
            for row in &self.events[first..last] {
                match mind
                    .interpretations
                    .binary_search_by_key(&row.form, |item| item.form)
                {
                    Ok(index) => mind.interpretations[index].meaning = row.meaning,
                    Err(index) => mind.interpretations.insert(
                        index,
                        Interpretation {
                            form: row.form,
                            meaning: row.meaning,
                        },
                    ),
                }
            }
        });
        owner.learning_events = self.events;
    }
}

impl Signals {
    /// Prepare personal associations from noticed prior signals and actual gifts.
    /// A listener may refuse, retain an old mapping, or infer a wrong meaning.
    pub fn learn_from_gifts(
        &self,
        gifts: &[GiftEvidence],
        tick: u64,
    ) -> Result<Option<Self>, &'static str> {
        let Some(draft) = self.prepare_gift_learning(gifts, tick)? else {
            return Ok(None);
        };
        let mut next = self.clone();
        draft.apply_validated(&mut next);
        Ok(Some(next))
    }

    /// Stage evidence-based inferences without cloning or changing the owner.
    pub fn prepare_gift_learning(
        &self,
        gifts: &[GiftEvidence],
        tick: u64,
    ) -> Result<Option<LearningDraft>, &'static str> {
        if gifts.is_empty()
            || !self
                .minds
                .par_iter()
                .any(|mind| !mind.learning_rules.is_empty())
        {
            return Ok(None);
        }
        if tick == 0
            || !self.valid_state(tick)
            || gifts.par_iter().any(|gift| {
                gift.actor == 0
                    || gift.observer == 0
                    || gift.actor == gift.observer
                    || gift.good == 0
            })
        {
            return Err("invalid signal learning evidence");
        }
        let mut noticed: Vec<_> = self
            .events
            .par_iter()
            .filter(|event| event.noticed && event.tick.checked_add(1) == Some(tick))
            .copied()
            .collect();
        if noticed.is_empty() {
            return Ok(None);
        }
        if noticed.par_windows(2).any(|rows| {
            (rows[0].observer, rows[0].actor, rows[0].form)
                > (rows[1].observer, rows[1].actor, rows[1].form)
        }) {
            noticed.par_sort_unstable_by_key(|event| (event.observer, event.actor, event.form));
        }
        let mut proposed: Vec<_> = gifts
            .par_iter()
            .filter_map(|gift| {
                let begin = noticed.partition_point(|event| {
                    (event.observer, event.actor) < (gift.observer, gift.actor)
                });
                let event = noticed
                    .get(begin)
                    .filter(|event| (event.observer, event.actor) == (gift.observer, gift.actor))?;
                let mind = self.mind(gift.observer)?;
                let previous = mind
                    .interpretations
                    .binary_search_by_key(&event.form, |row| row.form)
                    .ok()
                    .map(|i| mind.interpretations[i].meaning);
                let first = mind
                    .learning_rules
                    .partition_point(|rule| rule.good < gift.good);
                let last = mind
                    .learning_rules
                    .partition_point(|rule| rule.good <= gift.good);
                let rule = mind.learning_rules[first..last]
                    .iter()
                    .filter(|rule| {
                        rule.enabled
                            && (rule.value > mind.learning_idle
                                || (rule.value == mind.learning_idle && mind.learn_on_tie))
                            && (previous.is_none() || rule.replace)
                            && previous != Some(rule.meaning)
                    })
                    .max_by_key(|rule| (rule.value, Reverse(rule.meaning)))?;
                Some((
                    gift.observer,
                    event.form,
                    Reverse(rule.value),
                    rule.meaning,
                    gift.actor,
                    gift.good,
                    previous,
                ))
            })
            .collect();
        if proposed.is_empty() {
            return Ok(None);
        }
        if proposed.par_windows(2).any(|rows| rows[0] > rows[1]) {
            proposed.par_sort_unstable();
        }
        let chosen: Vec<_> = proposed
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|group| group[0])
            .collect();
        let events = chosen
            .into_par_iter()
            .map(|row| LearningEvent {
                tick,
                observer: row.0,
                form: row.1,
                meaning: row.3,
                actor: row.4,
                good: row.5,
                previous: row.6,
            })
            .collect();
        Ok(Some(LearningDraft { events }))
    }
}

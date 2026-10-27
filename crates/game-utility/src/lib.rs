//! Pure comparisons of explicitly supplied motives and consequences.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub need: i64,
    pub goal: i64,
    pub value: i64,
    pub effort: i64,
    pub risk: i64,
    pub survival: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: u64,
    pub score: i64,
    pub reasons: Vec<String>,
}
impl Candidate {
    /// Apply supplied percentage weights without coupling utility to a trait owner.
    pub fn reweight(&mut self, weight: impl Fn(&str) -> i64) {
        let apply = |term, value: i64| value.saturating_mul(weight(term)) / 100;
        self.need = apply("need", self.need);
        self.goal = apply("goal", self.goal);
        self.value = apply("value", self.value);
        self.effort = apply("effort", self.effort);
        self.risk = apply("risk", self.risk);
    }
    pub fn score(&self) -> i64 {
        self.need
            .saturating_add(self.goal)
            .saturating_add(self.value)
            .saturating_sub(self.effort)
            .saturating_sub(self.risk)
    }
}
pub fn choose(
    candidates: &[Candidate],
    current: Option<u64>,
    margin: i64,
    _urgent: bool,
) -> Option<Decision> {
    choose_iter(candidates.iter(), current, margin, _urgent)
}

/// Select from a reusable candidate iterator without allocating an intermediate list.
pub fn choose_iter<'a>(
    mut candidates: impl Iterator<Item = &'a Candidate> + Clone,
    current: Option<u64>,
    margin: i64,
    _urgent: bool,
) -> Option<Decision> {
    let best = candidates
        .clone()
        .max_by_key(|c| (c.score(), std::cmp::Reverse(c.id)))?;
    let chosen = candidates
        .find(|c| Some(c.id) == current && best.score() <= c.score().saturating_add(margin))
        .unwrap_or(best);
    Some(Decision {
        id: chosen.id,
        score: chosen.score(),
        reasons: vec![],
    })
}

/// Recent failure is a perceived cost, rather than a prohibition on another attempt.
pub fn failure_cost(remaining: u64, span: u64, learning: u64, caution: i64) -> i64 {
    let scale = i128::from(100 + learning.min(16000) / 10) * i128::from(caution.clamp(0, 2000));
    (scale * i128::from(remaining.min(span)) / i128::from(span.max(1)) / 1000) as i64
}

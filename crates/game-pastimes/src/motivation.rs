use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Satisfaction {
    pub at: SimTime,
    pub amount: u64,
    pub span: SimDuration,
}
impl Satisfaction {
    pub fn remaining(&self, now: SimTime) -> u64 {
        let elapsed = now.0.saturating_sub(self.at.0);
        (u128::from(self.amount) * u128::from(self.span.0)
            / u128::from(self.span.0.saturating_add(elapsed).max(1))) as u64
    }
}
pub(super) fn score(tx: &Transaction<'_>, person: PersonId, a: &Activity, index: usize) -> i64 {
    let interest = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&person))
        .and_then(|v| v.get(a.concept.as_str()))
        .copied()
        .unwrap_or(0);
    let affinity = game_vitality::disposition(tx, person, &a.affinity);
    let satisfaction = tx
        .get::<Pastimes>()
        .ok()
        .and_then(|s| s.satisfaction.get(&person)?.get(&a.concept))
        .map_or(0, |s| s.remaining(tx.now()));
    affinity
        + i64::from(interest) * 8
        + (random(tx.seed(), person.0 + 917, index as u64) % 500) as i64
        - (satisfaction.min(i64::MAX as u64) as i64)
}

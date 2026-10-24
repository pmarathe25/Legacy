use super::*;
use game_choice::{Next, Schedule};
#[derive(Clone, Serialize, Deserialize)]
pub struct Resume {
    pub person: PersonId,
}
impl Request for Resume {
    const KEY: &'static str = "characters.resume";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Recall {
    pub person: PersonId,
    pub until: SimTime,
}
impl Request for Recall {
    const KEY: &'static str = "characters.options-expired";
    type Response = ();
}
impl Recall {
    pub fn matches(&self, now: SimTime, mind: Option<&Mind>) -> bool {
        mind.and_then(|m| m.considered.as_ref())
            .is_some_and(|m| m.until == self.until && m.pending.is_some() && self.until <= now)
    }
}
impl Rules {
    /// Attention delays must not add a whole idle-review period by crossing its grid.
    pub fn next_review_at(&self, now: SimTime, schedule: &Schedule) -> Result<SimTime> {
        if self.attention.is_some() {
            schedule.grid(now.after(SimDuration(1))?, self.review)
        } else {
            now.after(self.review)
        }
    }
}
pub fn next(
    tx: &Transaction<'_>,
    person: PersonId,
    rules: &Rules,
    schedule: &Schedule,
) -> Result<Next> {
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    let pressure =
        1 + body.hunger / rules.urgent_hunger.max(1) + body.fatigue / rules.rest_fatigue.max(1);
    let horizon = MINUTE
        .0
        .saturating_mul(game_vitality::attention_horizon(tx, person, pressure));
    let interval = SimDuration(rules.review.0.max(horizon));
    let threshold = game_physiology::hunger_threshold(
        body,
        &tx.get::<game_physiology::Bodies>()?.rules,
        rules.urgent_hunger,
    );
    let base = rules.next_review_at(tx.now(), schedule)?;
    let next = if interval == rules.review {
        base
    } else {
        tx.now().after(interval)?
    };
    let next = next.min(threshold.unwrap_or(next).max(base));
    let Some(action) = tx.get::<Actions>()?.0.get(&person) else {
        return schedule.waiting(Some(next), next, rules.review);
    };
    let interruptible = action.kind != "combat";
    if interruptible && body.hunger >= rules.urgent_hunger {
        return schedule.waiting(Some(next), next, rules.review);
    }
    let mut at = action.until;
    if interruptible {
        at = match (at, threshold) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
    schedule.waiting(at, base, rules.review)
}

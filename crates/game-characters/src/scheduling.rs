use super::*;
pub(super) use game_choice::{Next, Schedule};
#[derive(Clone, Serialize, Deserialize)]
struct Resume {
    person: PersonId,
}
impl Request for Resume {
    const KEY: &'static str = "characters.resume";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_physiology::Fed>("characters.fed", |tx, e| wake(tx, e.patient, true))?;
    b.subscribe::<game_physiology::Damaged>("characters.hurt", |tx, e| wake(tx, e.person, false))?;
    b.request::<Resume>(|tx, r| {
        if !tx.get::<Minds>()?.running || !tx.get::<Minds>()?.people.contains_key(&r.person) {
            return Ok(());
        }
        review(tx, r.person)?;
        Ok(())
    })
}
pub(super) fn wake(tx: &mut Transaction<'_>, person: PersonId, hunger_only: bool) -> Result<()> {
    let state = tx.get::<Minds>()?;
    if !state.running
        || !state.people.contains_key(&person)
        || !hunger_only && state.schedule.waking.get(&person) == Some(&tx.now())
    {
        return Ok(());
    }
    let now = tx.now();
    let state = tx.edit::<Minds>()?;
    state.schedule.set(person, Next::Active);
    state.schedule.waking.insert(person, now);
    tx.schedule(now, 50, Resume { person })?;
    Ok(())
}
pub(super) fn next(tx: &Transaction<'_>, person: PersonId) -> Result<Next> {
    let state = tx.get::<Minds>()?;
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    let pressure = 1
        + body.hunger / state.rules.urgent_hunger.max(1)
        + body.fatigue / state.rules.rest_fatigue.max(1);
    let horizon = MINUTE
        .0
        .saturating_mul(game_vitality::attention_horizon(tx, person, pressure));
    let interval = SimDuration(state.rules.review.0.max(horizon));
    let threshold = game_physiology::hunger_threshold(
        body,
        &tx.get::<game_physiology::Bodies>()?.rules,
        state.rules.urgent_hunger,
    );
    let next = tx.now().after(interval)?;
    let base = tx.now().after(state.rules.review)?;
    let next = next.min(threshold.unwrap_or(next).max(base));
    let Some(action) = tx.get::<Actions>()?.0.get(&person) else {
        return state.schedule.waiting(Some(next), next, state.rules.review);
    };
    let interruptible = action.kind != "combat";
    if interruptible && body.hunger >= state.rules.urgent_hunger {
        return state.schedule.waiting(Some(next), next, state.rules.review);
    }
    let mut at = action.until;
    if interruptible {
        at = match (at, threshold) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
    state.schedule.waiting(at, base, state.rules.review)
}

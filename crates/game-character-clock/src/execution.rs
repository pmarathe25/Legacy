use super::*;
#[derive(Clone, Serialize, Deserialize)]
struct Advance {
    ready: game_choice::GoalReady,
    reviewed: SimTime,
}
impl Request for Advance {
    const KEY: &'static str = "characters.advance-goal";
    type Response = ();
}
fn reviewed(tx: &Transaction<'_>, e: &game_choice::GoalReady) -> Option<SimTime> {
    let minds = tx.get::<Minds>().ok()?;
    let mind = minds.people.get(&e.person)?;
    let floor = minds.rules.review_floor()?;
    let clock = mind.review_clock?;
    (minds.running
        && mind.engagement == Some(e.action)
        && mind.chosen.as_deref() == Some(e.key.as_str())
        && !clock.due(tx.now(), floor))
    .then_some(clock.reviewed)
    .flatten()
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.before::<game_choice::ConfigureExecution>("characters.execution-compatible", |tx, r| {
        require(
            !r.0 || tx.get::<Minds>()?.rules.review_floor().is_some(),
            "goal execution requires an attention floor",
        )
    })?;
    b.subscribe::<game_choice::GoalReady>("characters.advance-goal", |tx, e| {
        if tx.get::<game_choice::Offers>()?.goal_execution
            && let Some(reviewed) = reviewed(tx, e)
        {
            tx.schedule(
                tx.now(),
                50,
                Advance {
                    ready: e.clone(),
                    reviewed,
                },
            )?;
        }
        Ok(())
    })?;
    b.request::<Advance>(|tx, r| {
        let e = r.ready;
        if !tx.get::<game_choice::Offers>()?.goal_execution
            || reviewed(tx, &e) != Some(r.reviewed)
            || tx.get::<Actions>()?.0.contains_key(&e.person)
        {
            return Ok(());
        }
        let minds = tx.get::<Minds>()?;
        let body = &tx.get::<game_physiology::Bodies>()?.people[&e.person];
        if !body.alive
            || body.health == 0
            || body.hunger >= minds.rules.urgent_hunger
            || body.fatigue >= minds.rules.rest_fatigue
        {
            return super::wake(tx, e.person, false, true);
        }
        if tx
            .attempt(game_choice::Run(e.request))
            .is_ok_and(|o| o == game_choice::Outcome::Performed)
            && let Some(action) = tx.get::<Actions>()?.0.get(&e.person).map(|a| a.id)
        {
            tx.edit::<Minds>()?
                .people
                .get_mut(&e.person)
                .unwrap()
                .engagement = Some(action);
            let next = super::next(tx, e.person)?;
            let now = tx.now();
            let minds = tx.edit::<Minds>()?;
            minds.schedule.set(e.person, next);
            minds.clear_wake_at(e.person, now);
            return Ok(());
        }
        super::wake(tx, e.person, false, true)
    })
}

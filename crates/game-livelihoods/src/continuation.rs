use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Continue {
    pub person: PersonId,
    pub action: u64,
}
impl Request for Continue {
    const KEY: &'static str = "livelihoods.continue";
    type Response = game_choice::Outcome;
}
fn enabled(tx: &Transaction<'_>) -> bool {
    tx.get::<game_choice::Offers>()
        .is_ok_and(|o| o.goal_execution)
}
pub(super) fn started(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if enabled(tx)
        && let Some(action) = tx.get::<Actions>()?.0.get(&person).map(|a| a.id)
    {
        let state = tx.edit::<Livelihoods>()?;
        state.active_steps.insert(person, action);
        state.ready_steps.remove(&person);
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_physiology::Died>("livelihoods.forget-execution", |tx, e| {
        if !enabled(tx) {
            return Ok(());
        }
        let state = tx.edit::<Livelihoods>()?;
        state.active_steps.remove(&e.person);
        state.ready_steps.remove(&e.person);
        Ok(())
    })?;
    b.before::<game_choice::ConfigureExecution>("livelihoods.reset-execution", |tx, r| {
        if enabled(tx) != r.0 {
            let state = tx.edit::<Livelihoods>()?;
            state.active_steps = Table::default();
            state.ready_steps = Table::default();
        }
        Ok(())
    })?;
    b.subscribe::<ActionFinished>("livelihoods.step-finished", |tx, e| {
        if !enabled(tx) || tx.get::<Livelihoods>()?.active_steps.get(&e.person) != Some(&e.action) {
            return Ok(());
        }
        tx.edit::<Livelihoods>()?.active_steps.remove(&e.person);
        if e.completed
            && let Some(plan) = tx.get::<Livelihoods>()?.plans.get(&e.person)
        {
            let key = format!("work:{}", plan.goal.0);
            tx.edit::<Livelihoods>()?
                .ready_steps
                .insert(e.person, e.action);
            tx.notify(game_choice::GoalReady {
                person: e.person,
                action: e.action,
                key,
                request: Pending::immediate(Continue {
                    person: e.person,
                    action: e.action,
                }),
            })?;
        }
        Ok(())
    })?;
    b.request::<Continue>(|tx, r| {
        if !enabled(tx)
            || tx.get::<Livelihoods>()?.ready_steps.get(&r.person) != Some(&r.action)
            || !tx.get::<Livelihoods>()?.has_plan(r.person)
        {
            return Ok(game_choice::Outcome::Reconsider);
        }
        tx.edit::<Livelihoods>()?.ready_steps.remove(&r.person);
        consider(tx, r.person).map(Into::into)
    })
}

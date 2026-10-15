use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cost {
    Strain,
    Identity,
    Emotion,
    Lifespan,
    ControlLoss,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostAt {
    pub after: SimDuration,
    pub cost: Cost,
    pub amount: u64,
    pub duration: SimDuration,
}
pub fn apply(tx: &mut Transaction<'_>, warning: &Warning) -> Result<()> {
    let person = tx.get::<Invocation>()?.active.as_ref().unwrap().person;
    match warning.cost {
        Cost::Strain => {
            tx.call(game_physiology::Strain {
                person,
                amount: warning.amount,
            })?;
        }
        Cost::Identity => {
            let s = tx.edit::<Invocation>()?;
            s.identity = s
                .identity
                .checked_add(warning.amount)
                .ok_or(Error::Overflow)?;
        }
        Cost::Emotion => {
            let s = tx.edit::<Invocation>()?;
            s.instability = s
                .instability
                .checked_add(warning.amount)
                .ok_or(Error::Overflow)?;
        }
        Cost::Lifespan => {
            let d = SimDuration(warning.amount);
            tx.call(game_physiology::ShortenLife {
                person,
                duration: d,
            })?;
            let s = tx.edit::<Invocation>()?;
            s.lifespan_lost.0 = s.lifespan_lost.0.checked_add(d.0).ok_or(Error::Overflow)?;
        }
        Cost::ControlLoss => {
            require(warning.duration.0 > 0, "control loss must have duration")?;
            let end = tx.now().after(warning.duration)?;
            let until = tx
                .get::<Invocation>()?
                .control_until
                .map_or(end, |old| old.max(end));
            tx.edit::<Invocation>()?.control_until = Some(until);
            tx.schedule(until, 5, ControlEnd { until })?;
            if tx.registered::<game_conflict::RefreshTurns>() {
                tx.call(game_conflict::RefreshTurns)?;
            }
        }
    }
    Ok(())
}

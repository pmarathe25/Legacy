use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureExecution(pub bool);
impl Request for ConfigureExecution {
    const KEY: &'static str = "choice.configure-execution";
    type Response = ();
}
/// An owner-validated step finished; the scheduler may advance its current goal.
#[derive(Clone, Serialize, Deserialize)]
pub struct GoalReady {
    pub person: PersonId,
    pub action: u64,
    pub key: String,
    pub request: Pending,
}
impl Event for GoalReady {
    const KEY: &'static str = "choice.goal-ready";
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<ConfigureExecution>(|tx, r| {
        require(
            !r.0 || tx.get::<Offers>()?.frame.is_none(),
            "goal execution requires event-driven decisions",
        )?;
        if tx.get::<Offers>()?.goal_execution != r.0 {
            tx.edit::<Offers>()?.goal_execution = r.0;
        }
        Ok(())
    })
}

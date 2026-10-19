use super::*;
/// Completed owner execution, distinct from cancellation or the initial commitment.
#[derive(Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub person: PersonId,
    pub action: u64,
    pub outcome: Outcome,
}
impl Event for Resolved {
    const KEY: &'static str = "choice.resolved";
}
/// A chosen deliberation occupies attention; its owner revalidates the request on completion.
#[derive(Clone, Serialize, Deserialize)]
pub struct Undertake {
    pub person: PersonId,
    pub kind: String,
    pub duration: SimDuration,
    pub request: Pending,
}
impl Request for Undertake {
    const KEY: &'static str = "choice.undertake";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    person: PersonId,
    kind: String,
    request: Pending,
}
impl Request for Finish {
    const KEY: &'static str = "choice.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(ScheduleId);
impl Request for Cancel {
    const KEY: &'static str = "choice.cancel";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Undertake>(|tx, r| {
        require(
            r.duration.0 > 0 && !r.kind.is_empty() && r.kind.len() <= 40,
            "invalid undertaking",
        )?;
        let until = tx.now().after(r.duration)?;
        let job = tx.schedule(
            until,
            20,
            Finish {
                person: r.person,
                kind: r.kind.clone(),
                request: r.request,
            },
        )?;
        tx.start_action(r.person, &r.kind, Some(until), Cancel(job))?;
        Ok(until)
    })?;
    b.request::<Cancel>(|tx, r| {
        tx.unschedule(r.0);
        Ok(())
    })?;
    b.request::<Finish>(|tx, r| {
        let action = tx.get::<Actions>()?.0[&r.person].id;
        tx.finish_action(r.person, &r.kind)?;
        let outcome = tx.attempt(Run(r.request)).unwrap_or(Outcome::Reconsider);
        tx.notify(Resolved {
            person: r.person,
            action,
            outcome,
        })
    })
}

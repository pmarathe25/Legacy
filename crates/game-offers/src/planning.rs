//! Deferred owner requests accompany pure actor preparation, never speculative writes.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
struct Source(Option<String>);
impl Request for Source {
    const KEY: &'static str = "choice.planning-source";
    type Response = Option<String>;
}
#[derive(Clone, Serialize, Deserialize)]
struct Defer(Pending);
impl Request for Defer {
    const KEY: &'static str = "choice.defer-sensing";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Source>(|tx, r| Ok(std::mem::replace(&mut tx.edit::<Offers>()?.source, r.0)))?;
    b.request::<Defer>(|tx, r| {
        let state = tx.edit::<Offers>()?;
        require(state.planning, "not preparing a decision")?;
        state.effects.push(r.0);
        Ok(())
    })
}
pub fn planning(tx: &Transaction<'_>) -> bool {
    tx.get::<Offers>().is_ok_and(|o| o.planning)
}
pub fn defer<R: Request>(tx: &mut Transaction<'_>, request: R) -> Result<()> {
    if planning(tx) {
        tx.call(Defer(Pending::immediate(request)))
    } else {
        tx.call(request).map(|_| ())
    }
}
pub fn with_provider(
    tx: &mut Transaction<'_>,
    name: Option<&str>,
    run: impl FnOnce(&mut Transaction<'_>) -> Result<()>,
) -> Result<()> {
    if !planning(tx) {
        return run(tx);
    }
    let previous = tx.call(Source(name.map(str::to_owned)))?;
    let result = run(tx);
    tx.call(Source(previous))?;
    result
}
impl Offers {
    pub fn planning(fanout: bool) -> Self {
        Self {
            planning: true,
            fanout,
            ..Self::default()
        }
    }
    pub fn finish_planning(self) -> (Vec<Pending>, std::collections::BTreeMap<String, String>) {
        (self.effects, self.sources)
    }
}

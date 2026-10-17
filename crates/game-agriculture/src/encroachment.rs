use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Encroached {
    pub person: PersonId,
    pub steward: PersonId,
    pub plot: PlotId,
    pub purpose: String,
    pub witnesses: Vec<PersonId>,
}
impl Event for Encroached {
    const KEY: &'static str = "agriculture.encroached";
}
pub(super) fn note(
    tx: &mut Transaction<'_>,
    person: PersonId,
    plot: PlotId,
    purpose: &str,
) -> Result<()> {
    let p = &tx.get::<Farms>()?.plots[&plot];
    let steward = p.steward;
    let area = p.area;
    if steward == person {
        return Ok(());
    }
    let mut witnesses = vec![person];
    if game_transport::area(tx.get()?, steward).ok() == Some(area) {
        witnesses.push(steward);
    }
    tx.emit(
        Encroached {
            person,
            steward,
            plot,
            purpose: purpose.into(),
            witnesses: witnesses.clone(),
        },
        witnesses,
    )
}

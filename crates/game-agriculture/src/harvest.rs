use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct HarvestSome {
    pub person: PersonId,
    pub plot: PlotId,
    pub destination: InventoryId,
    pub quantity: u64,
}
impl Request for HarvestSome {
    const KEY: &'static str = "agriculture.harvest-some";
    type Response = SimTime;
}
fn begin(tx: &mut Transaction<'_>, r: Harvest, amount: Option<u64>) -> Result<SimTime> {
    local(tx, r.person, r.plot, r.destination)?;
    encroachment::note(tx, r.person, r.plot, "harvesting")?;
    let Phase::Ripe { yield_units } = tx.get::<Farms>()?.plots[&r.plot].phase else {
        return Err(Error::Rejected("crop is not ripe".into()));
    };
    require(amount.is_none_or(|n| n > 0), "empty harvest")?;
    let remaining = yield_units.saturating_sub(amount.unwrap_or(yield_units));
    let yield_units = amount.unwrap_or(yield_units).min(yield_units);
    let reservation = if yield_units > 0 {
        Some(tx.call(game_cargo::ReserveSpace {
            good: genetics::crop(tx, r.plot)?.output,
            inventory: r.destination,
            quantity: yield_units,
        })?)
    } else {
        None
    };
    let until = tx.now().after(genetics::crop(tx, r.plot)?.harvest_labor)?;
    tx.start_action(r.person, "farm", Some(until), Cancel { person: r.person })?;
    let job = tx.schedule(until, 10, Finish { person: r.person })?;
    let s = tx.edit::<Farms>()?;
    s.plots.get_mut(&r.plot).unwrap().phase = Phase::Harvesting { yield_units };
    s.work.insert(
        r.person,
        Work {
            plot: r.plot,
            job,
            destination: r.destination,
            reservation,
            remaining,
        },
    );
    Ok(until)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Harvest>(|tx, r| begin(tx, r, None))?;
    b.request::<HarvestSome>(|tx, r| {
        begin(
            tx,
            Harvest {
                person: r.person,
                plot: r.plot,
                destination: r.destination,
            },
            Some(r.quantity),
        )
    })?;
    Ok(())
}

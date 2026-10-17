use super::*;
/// Optional perception-free provider: physical storage and weather owners supply the rate.
#[derive(Clone, Serialize, Deserialize)]
pub struct Conditions {
    pub inventory: InventoryId,
}
impl Request for Conditions {
    const KEY: &'static str = "preservation.conditions";
    type Response = u32;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Refresh {
    pub inventory: InventoryId,
}
impl Request for Refresh {
    const KEY: &'static str = "preservation.refresh";
    type Response = ();
}
pub(super) fn retime(now: SimTime, expires: SimTime, from: u32, to: u32) -> Result<SimTime> {
    let remaining = u128::from(expires.0.saturating_sub(now.0)) * u128::from(to) / u128::from(from);
    now.after(SimDuration(
        remaining.try_into().map_err(|_| Error::Overflow)?,
    ))
}
fn rate(tx: &mut Transaction<'_>, inventory: InventoryId) -> Result<u32> {
    let rate = if tx.registered::<Conditions>() {
        tx.call(Conditions { inventory })?
    } else {
        1000
    };
    require((100..=16000).contains(&rate), "invalid storage aging rate")?;
    Ok(rate)
}
pub(super) fn fresh(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    days: u64,
) -> Result<SimTime> {
    let rate = rate(tx, inventory)?;
    tx.edit::<Preservation>()?.rates.insert(inventory, rate);
    // Initial production shares daily expiry buckets. Subsequent moves retain remaining life.
    let baseline = SimTime(
        (tx.now().0 / DAY.0 + days + 1)
            .checked_mul(DAY.0)
            .ok_or(Error::Overflow)?,
    );
    retime(tx.now(), baseline, 1000, rate)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Refresh>(|tx, r| {
        if !tx.get::<Preservation>()?.enabled {
            return Ok(());
        }
        let to = rate(tx, r.inventory)?;
        let from = tx
            .get::<Preservation>()?
            .rates
            .get(&r.inventory)
            .copied()
            .unwrap_or(1000);
        if from == to {
            if !tx.get::<Preservation>()?.rates.contains_key(&r.inventory) {
                tx.edit::<Preservation>()?.rates.insert(r.inventory, to);
            }
            return Ok(());
        }
        let lots = tx
            .edit::<Preservation>()?
            .batches
            .remove(&r.inventory)
            .unwrap_or_default();
        for (good, rows) in &lots {
            for lot in rows {
                let batch = Batch {
                    quantity: lot.quantity,
                    expires: retime(tx.now(), lot.expires, from, to)?,
                };
                put(tx, r.inventory, *good, batch)?;
            }
        }
        tx.edit::<Preservation>()?.rates.insert(r.inventory, to);
        Ok(())
    })?;
    Ok(())
}

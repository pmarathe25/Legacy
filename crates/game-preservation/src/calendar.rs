use super::*;
/// Migrate earlier saves once, retaining only appointments backed by physical lots.
pub(super) fn ensure(tx: &mut Transaction<'_>) -> Result<()> {
    if tx.get::<Preservation>()?.indexed {
        return Ok(());
    }
    tx.cancel_scheduled::<Expire>()?;
    let mut due: Table<SimTime, BTreeSet<(InventoryId, GoodId)>> = Table::new();
    for (inventory, goods) in &tx.get::<Preservation>()?.batches {
        for (good, lots) in goods {
            for lot in lots {
                due.entry(lot.expires)
                    .or_default()
                    .insert((*inventory, *good));
            }
        }
    }
    let dates: Vec<_> = due.keys().copied().collect();
    let state = tx.edit::<Preservation>()?;
    state.due = due;
    state.timers = Table::new();
    state.indexed = true;
    for at in dates {
        wake(tx, at)?;
    }
    Ok(())
}
pub(super) fn wake(tx: &mut Transaction<'_>, at: SimTime) -> Result<()> {
    if !tx.get::<Preservation>()?.timers.contains_key(&at) {
        let id = tx.schedule(at, 5, Expire)?;
        tx.edit::<Preservation>()?.timers.insert(at, id);
    }
    Ok(())
}
pub(super) fn remove(
    tx: &mut Transaction<'_>,
    inventory: InventoryId,
    good: GoodId,
    at: SimTime,
) -> Result<()> {
    let state = tx.edit::<Preservation>()?;
    if state.due.get_mut(&at).is_some_and(|targets| {
        targets.remove(&(inventory, good));
        targets.is_empty()
    }) {
        state.due.remove(&at);
        if let Some(id) = state.timers.remove(&at) {
            tx.unschedule(id);
        }
    }
    Ok(())
}

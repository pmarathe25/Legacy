//! Arrival positions become visible together before ordered completion listeners.
use super::*;
pub(super) fn one(tx: &mut Transaction<'_>, r: Arrive) -> Result<()> {
    tx.edit::<Positions>()?
        .0
        .insert(r.person, Location::At(r.destination));
    let action = travel_action(tx, r.person);
    tx.edit::<Riders>()?.moving.remove(&r.person);
    publish(tx, &r, action)
}
fn travel_action(tx: &Transaction<'_>, person: PersonId) -> Option<u64> {
    tx.get::<Actions>()
        .ok()?
        .0
        .get(&person)
        .filter(|a| a.kind == "travel")
        .map(|a| a.id)
}
fn publish(tx: &mut Transaction<'_>, r: &Arrive, action: Option<u64>) -> Result<()> {
    if action.is_some() && travel_action(tx, r.person) == action {
        tx.finish_action(r.person, "travel")?;
    }
    tx.emit(
        Arrived {
            person: r.person,
            area: r.destination,
        },
        vec![r.person],
    )
}
pub(super) fn wave(tx: &mut Transaction<'_>, requests: &[Arrive]) -> Result<()> {
    game_diagnostics::interaction_profile!({
        game_diagnostics::count("transport.wave.calls", 1);
        game_diagnostics::count("transport.wave.rows", requests.len() as u64);
    });
    let positions = tx.get::<Positions>()?;
    let keys: Vec<_> = requests.iter().map(|r| r.person).collect();
    let unique: std::collections::BTreeSet<_> = keys.iter().copied().collect();
    // Repeated actors and non-transit callbacks use ordered owner arbitration.
    if unique.len() != keys.len() || requests.iter().any(|r| !matches!(positions.0.get(&r.person), Some(Location::Transit { to, arrives, .. }) if *to == r.destination && *arrives == tx.now())) {
        for r in requests { one(tx, r.clone())?; }
        return Ok(());
    }
    let updated = tx.edit_with_compute::<Positions, _>(|positions, pool| {
        game_diagnostics::interaction_profile!(batch "transport-position", keys.len(), keys.len() >= 4096 && pool.current_num_threads() > 1);
        positions
            .0
            .update_selected(pool, &keys, 4096, |_, location| {
                let Location::Transit { to, .. } = location else {
                    unreachable!("validated arrival")
                };
                *location = Location::At(*to);
            })
    })?;
    require(updated, "arrival rows changed during phase")?;
    let actions: Vec<_> = requests
        .iter()
        .map(|r| travel_action(tx, r.person))
        .collect();
    for r in requests {
        tx.edit::<Riders>()?.moving.remove(&r.person);
    }
    for (r, action) in requests.iter().zip(actions) {
        publish(tx, r, action)?;
    }
    Ok(())
}

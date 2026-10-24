use super::*;
pub(super) fn capacity(tx: &Transaction<'_>, p: PersonId) -> usize {
    (32 + game_vitality::ability(tx, p, "memory_capacity").min(4000) * 96 / 1000) as usize
}
pub fn duration(tx: &Transaction<'_>, p: PersonId) -> SimDuration {
    SimDuration(
        6 * HOUR.0
            + game_vitality::ability(tx, p, "memory_durability").min(4000) * 42 * HOUR.0 / 1000,
    )
}
pub(super) fn forget_many<V: Content>(
    tx: &mut Transaction<'_>,
    p: PersonId,
    keys: Vec<Arc<str>>,
) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let state = tx.edit::<Memories<V>>()?;
    for key in &keys {
        if let Some(f) = state.0.get_mut(&p) {
            f.remove(key);
        }
        if let Some(b) = state.1.get_mut(&p) {
            b.entries.remove(key);
        }
    }
    if let Some(b) = state.1.get_mut(&p) {
        let removed: std::collections::BTreeSet<_> = keys.into_iter().collect();
        let linked: Vec<_> = b
            .entries
            .iter()
            .filter(|(_, t)| t.related.iter().any(|k| removed.contains(k)))
            .map(|(k, _)| k.clone())
            .collect();
        for k in linked {
            b.entries
                .get_mut(&k)
                .unwrap()
                .related
                .retain(|x| !removed.contains(x));
        }
    }
    Ok(())
}
pub(super) fn prune<V: Content>(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let now = tx.now();
    let keys: Vec<_> = tx
        .get::<Memories<V>>()?
        .1
        .get(&p)
        .into_iter()
        .flat_map(|b| b.entries.iter())
        .filter(|(_, t)| t.accessibility(now) == 0)
        .map(|(k, _)| k.clone())
        .collect();
    forget_many::<V>(tx, p, keys)
}
pub(super) fn schedule<V: Content>(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let Some(book) = tx.get::<Memories<V>>()?.1.get(&p) else {
        return Ok(());
    };
    if book.entries.is_empty() {
        return Ok(());
    }
    let at = book
        .entries
        .values()
        .filter_map(|t| t.expires)
        .min()
        .unwrap_or(tx.now().after(SimDuration(30 * DAY.0))?);
    let at = at
        .min(tx.now().after(SimDuration(30 * DAY.0))?)
        .max(tx.now().after(SimDuration(1))?);
    if book.due.is_some_and(|due| due <= at) {
        return Ok(());
    }
    if let Some(job) = book.sweep {
        tx.unschedule(job);
    }
    let job = tx.schedule(at, 40, Sweep { person: p })?;
    let book = tx.edit::<Memories<V>>()?.1.get_mut(&p).unwrap();
    book.sweep = Some(job);
    book.due = Some(at);
    Ok(())
}
pub(super) fn sleep<V: Content>(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    prune::<V>(tx, p)?;
    let limit = capacity(tx, p);
    let now = tx.now();
    let Some(book) = tx.edit::<Memories<V>>()?.1.get_mut(&p) else {
        return Ok(());
    };
    let candidates: Vec<_> = book
        .entries
        .iter()
        .filter(|(_, t)| {
            t.expires.is_some() && (t.importance >= 500 || t.emotion.unsigned_abs() >= 500)
        })
        .map(|(k, _)| k.clone())
        .collect();
    for key in candidates {
        let t = book.entries.get_mut(&key).unwrap();
        t.expires = None;
        t.strength = t.importance.max(t.emotion.unsigned_abs()).max(500);
        t.rehearsed = now;
    }
    let keys = book.crowded_long(now, limit);
    forget_many::<V>(tx, p, keys)?;
    schedule::<V>(tx, p)
}
pub(super) fn finish_batch<V: Content>(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let book = &tx.get::<Memories<V>>()?.1[&p];
    let keys = book.crowded_short(512);
    forget_many::<V>(tx, p, keys)?;
    schedule::<V>(tx, p)
}

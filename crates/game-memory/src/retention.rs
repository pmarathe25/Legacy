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
pub(super) fn forget<V: Content>(tx: &mut Transaction<'_>, p: PersonId, key: &str) -> Result<()> {
    let state = tx.edit::<Memories<V>>()?;
    if let Some(f) = state.0.get_mut(&p) {
        f.remove(key);
    }
    if let Some(b) = state.1.get_mut(&p) {
        b.entries.remove(key);
        let linked: Vec<_> = b
            .entries
            .iter()
            .filter(|(_, t)| t.related.iter().any(|k| k.as_ref() == key))
            .map(|(k, _)| k.clone())
            .collect();
        for k in linked {
            b.entries
                .get_mut(&k)
                .unwrap()
                .related
                .retain(|x| x.as_ref() != key);
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
        .map(|(k, _)| k.to_string())
        .collect();
    for key in keys {
        forget::<V>(tx, p, &key)?;
    }
    Ok(())
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
    let mut retained: Vec<_> = book
        .entries
        .iter()
        .filter(|(_, t)| t.expires.is_none())
        .map(|(k, t)| {
            (
                t.accessibility(now),
                t.importance.max(t.emotion.unsigned_abs()),
                t.rehearsed,
                k.clone(),
            )
        })
        .collect();
    retained.sort();
    let excess = retained.len().saturating_sub(limit);
    for (_, _, _, key) in retained.into_iter().take(excess) {
        forget::<V>(tx, p, &key)?;
    }
    schedule::<V>(tx, p)
}
pub(super) fn record<V: Content>(
    tx: &mut Transaction<'_>,
    p: PersonId,
    key: Arc<str>,
    same: bool,
) -> Result<()> {
    let now = tx.now();
    let end = now.after(duration(tx, p))?;
    let fact = tx.get::<Memories<V>>()?.0[&p].get(&key).unwrap().to_owned();
    let mut cues = fact.value.cues(&key);
    cues.retain(|s| !s.is_empty() && s.len() <= 160);
    cues.sort();
    cues.dedup();
    cues.truncate(8);
    let emotion = game_vitality::condition(tx, p).map_or(0, |(stress, satisfaction)| {
        if stress > 500 {
            -(stress as i16)
        } else {
            satisfaction.clamp(-1000, 1000) as i16
        }
    });
    let interest = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&p))
        .map_or(100, |values| {
            cues.iter()
                .filter_map(|c| values.get(c.as_str()))
                .map(|n| (n.unsigned_abs() * 10).min(1000) as u16)
                .max()
                .unwrap_or(100)
        });
    let book = tx.edit::<Memories<V>>()?.1.entry(p).or_default();
    let prior = book.entries.get(&key);
    if same
        && let Some(old) = prior
        && old.expires.is_none()
    {
        let t = book.entries.get_mut(&key).unwrap();
        t.strength = t.accessibility(now).saturating_add(20).min(1000);
        t.rehearsed = now;
    } else {
        let importance = prior.map_or(interest, |t| t.importance.max(interest));
        let related = book
            .entries
            .iter()
            .filter(|(k, t)| **k != key && t.cues.iter().any(|c| cues.contains(c)))
            .take(4)
            .map(|(k, _)| k.clone())
            .collect();
        book.entries.insert(
            key.clone(),
            Trace {
                learned: now,
                rehearsed: now,
                expires: Some(end),
                importance,
                emotion,
                strength: 1000,
                recalls: 0,
                cues,
                related,
            },
        );
    }
    let mut short: Vec<_> = book
        .entries
        .iter()
        .filter(|(_, t)| t.expires.is_some())
        .map(|(k, t)| {
            (
                t.importance.max(t.emotion.unsigned_abs()),
                t.learned,
                k.clone(),
            )
        })
        .collect();
    short.sort();
    let excess = short.len().saturating_sub(512);
    for (_, _, k) in short.into_iter().take(excess) {
        forget::<V>(tx, p, &k)?;
    }
    schedule::<V>(tx, p)
}

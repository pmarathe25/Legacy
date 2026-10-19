use super::*;
pub fn condition(tx: &Transaction<'_>, p: PersonId, w: &Garment) -> u32 {
    if !w.equipped {
        return w.condition;
    }
    let at = tx
        .get::<Equipment>()
        .ok()
        .and_then(|s| s.donned.get(&p))
        .copied()
        .unwrap_or(tx.now());
    w.condition
        .saturating_sub((tx.now().0.saturating_sub(at.0) / (7 * DAY.0)).min(100) as u32)
}

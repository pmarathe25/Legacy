use super::*;
/// Only experienced foods can promise relief from a particular deficiency or craving.
/// The contribution is bounded by the amount in one portion, rather than by hunger alone.
pub fn appetite_for(tx: &Transaction<'_>, person: PersonId, good: GoodId) -> u64 {
    let Some(n) = tx.get::<Nutrition>().ok().filter(|n| n.enabled) else {
        return 0;
    };
    let Some(d) = n.people.get(&person) else {
        return 0;
    };
    let Some(learned) = d.learned.get(&good) else {
        return 0;
    };
    let deficiency = n
        .needs
        .iter()
        .map(|(key, need)| {
            let used = (u128::from(tx.now().0.saturating_sub(d.at.0)) * u128::from(*need) * 1000
                / u128::from(DAY.0))
            .min(u128::from(u64::MAX)) as u64;
            let balance = d
                .reserves
                .get(key)
                .copied()
                .unwrap_or(0)
                .saturating_sub(used);
            let shortage = 1000 - (balance / (need * 14)).min(1000);
            shortage * learned.nutrients.get(key).copied().unwrap_or(0).min(*need) / need
        })
        .max()
        .unwrap_or(0);
    let craving = learned
        .substance
        .as_ref()
        .and_then(|s| d.habits.get(s))
        .map_or(0, |h| u64::from(h.craving(tx.now())) / 10);
    deficiency / 12 + craving
}

use super::*;
fn similarity(a: &str, b: &str) -> i64 {
    let a = first(a).to_lowercase();
    let b = first(b).to_lowercase();
    if a == b {
        return 100;
    }
    let common = a.chars().zip(b.chars()).take_while(|(a, b)| a == b).count();
    (common * 100 / a.chars().count().max(b.chars().count()).max(1)) as i64
}
fn regard(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&other))
        .map_or(0, |r| {
            i64::from(r.affection + r.respect + r.admiration - r.grievance).max(0)
        })
}
pub(super) fn suggest(tx: &mut Transaction<'_>, r: SuggestChild) -> Result<String> {
    let known: Vec<_> = tx
        .get::<Names>()?
        .heard
        .get(&r.chooser)
        .into_iter()
        .flat_map(|row| row.iter())
        .map(|(p, n)| (*p, n.clone()))
        .collect();
    let creativity = disposition(tx, r.chooser, "creativity");
    let empathy = disposition(tx, r.chooser, "empathy");
    let mut candidates: Vec<_> = known
        .iter()
        .filter(|(p, _)| *p != r.chooser)
        .map(|(p, n)| {
            (
                first(n).to_string(),
                regard(tx, r.chooser, *p) * (1000 + empathy) / 1000,
            )
        })
        .collect();
    let breadth = (4
        + game_vitality::ability(tx, r.chooser, "intelligence") / 500
        + creativity.max(0) as u64 / 250)
        .clamp(4, 64);
    for offset in 0..breadth {
        let salt = PersonId(r.child.0.wrapping_add(offset * 997));
        candidates.push((invent(tx, r.chooser, salt)?, 10 + creativity / 4));
    }
    candidates
        .into_iter()
        .map(|(name, tribute)| {
            let costs: Vec<_> = known
                .iter()
                .map(|(p, n)| similarity(&name, n) * (45 + regard(tx, r.chooser, *p) / 2) / 100)
                .filter(|cost| *cost > 0)
                .collect();
            let confusion = costs.iter().sum::<i64>() * costs.len() as i64;
            (tribute - confusion, name)
        })
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|(_, name)| name)
        .ok_or_else(|| Error::Rejected("no name considered".into()))
}

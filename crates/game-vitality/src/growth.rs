use super::*;
pub fn disposition(tx: &Transaction<'_>, person: PersonId, key: &str) -> i64 {
    projection::disposition(tx, person, key)
}
/// Physical locomotion readiness; a dependent may still be carried by another person.
pub fn can_walk(tx: &Transaction<'_>, person: PersonId) -> bool {
    projection::readiness(tx, person, "speed")
}

pub fn can_converse(tx: &Transaction<'_>, person: PersonId) -> bool {
    projection::readiness(tx, person, "language")
}

/// Coordinated deliberate manipulation, rather than reflexive infant movement.
/// Uses expressed motor development, so inherited potential and impairment matter.
pub fn can_manipulate(tx: &Transaction<'_>, person: PersonId) -> bool {
    projection::readiness(tx, person, "agility")
}

/// Attention supports a current commitment; important needs can still interrupt it.
pub fn switching_margin(tx: &Transaction<'_>, person: PersonId, base: i64) -> i64 {
    base.saturating_mul(250 + ability(tx, person, "attention").min(32000) as i64) / 1250
}

/// A personal horizon for reconsideration, shortened by the stakes of current needs.
/// External evidence remains free to interrupt; this does not exclude any action.
pub fn attention_horizon(tx: &Transaction<'_>, p: PersonId, pressure: u64) -> u64 {
    let attention = tx
        .get::<Vitality>()
        .ok()
        .filter(|v| v.enabled)
        .map_or(0, |_| ability(tx, p, "attention").min(16000));
    1 + attention / 250 / pressure.max(1)
}

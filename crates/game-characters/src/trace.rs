//! Read-only, feature-gated telemetry; no trace value controls autonomy.
use super::*;
pub(super) fn state(tx: &Transaction<'_>, person: PersonId) -> serde_json::Value {
    let mind = tx.get::<Minds>().ok().and_then(|s| s.people.get(&person));
    let body = tx
        .get::<game_physiology::Bodies>()
        .ok()
        .and_then(|b| b.people.get(&person));
    serde_json::json!({
        "body":body.map(|b| (b.alive,b.hunger,b.fatigue,b.health,b.born,b.last)),
        "action":tx.get::<Actions>().ok().and_then(|a| a.0.get(&person)),
        "chosen":mind.and_then(|m| m.chosen.as_deref()),
        "decision":mind.and_then(|m| m.last.as_ref()),
        "failures":mind.map(|m| m.failures)
    })
}
pub(super) fn begin(tx: &Transaction<'_>, person: PersonId) -> game_contracts::causal::Review {
    game_contracts::causal::Review::begin(person.0, tx.now().0, tx.revision_after(), || {
        state(tx, person)
    })
}

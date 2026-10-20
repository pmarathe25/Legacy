//! Dispositions and learned moral responses to personally experienced violence.
use game_core::*;
use game_sim::*;
/// Personal convictions can outweigh inherited aggression in either direction.
pub fn ethics(tx: &Transaction<'_>, person: PersonId) -> i64 {
    tx.get::<game_relationships::Social>().map_or(0, |s| {
        i64::from(s.opinion(person, "violence") - s.opinion(person, "peace")) * 4
            - i64::from(s.opinion(person, "care")) * 2
    })
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("violence", 1)?;
    b.subscribe::<game_conflict::Struck>("violence.moral-experience", |tx, e| {
        for person in [e.actor, e.target] {
            if game_physiology::capable(tx, person).is_err() {
                continue;
            }
            let rigidity = game_vitality::disposition(tx, person, "rigidity");
            if (random(tx.seed(), person.0, tx.now().0) % 1000) < rigidity as u64 {
                continue;
            }
            let aggression = game_vitality::disposition(tx, person, "aggression");
            let empathy = game_vitality::disposition(tx, person, "empathy");
            let compassion = game_vitality::disposition(tx, person, "compassion");
            let response = ((empathy + compassion - aggression) / 300).clamp(-3, 3) as i32;
            if response != 0 {
                tx.call(game_relationships::LearnValue {
                    person,
                    concept: "peace".into(),
                    outcome: response,
                })?;
                tx.call(game_relationships::LearnValue {
                    person,
                    concept: "violence".into(),
                    outcome: -response,
                })?;
            }
        }
        Ok(())
    })?;
    Ok(())
}

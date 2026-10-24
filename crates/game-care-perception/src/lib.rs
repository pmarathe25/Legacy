//! Personal care candidates, perceived food need and concern; no resource ownership.
use game_core::*;
use game_households::Households;
use game_sim::*;
mod signals;
pub use signals::{SignalReaction, notice_prioritized};
pub fn concern(tx: &Transaction<'_>, p: PersonId, other: PersonId, need: u64) -> i64 {
    let altruism = game_vitality::disposition(tx, p, "altruism");
    let empathy = game_vitality::disposition(tx, p, "empathy");
    let social = tx.get::<game_relationships::Social>().ok();
    let tie = social.and_then(|s| s.relationships.get(&p)?.get(&other));
    let attachment = tie.map_or(0, |r| {
        i64::from(r.affection) * 4 - i64::from(r.grievance) * 4 + i64::from(r.fear) * 2
    });
    let moral = social
        .and_then(|s| s.values.get(&p)?.get("care"))
        .copied()
        .unwrap_or(0);
    let body = &tx.get::<game_physiology::Bodies>().unwrap().people[&p];
    let compassion = game_vitality::disposition(tx, p, "compassion");
    let need = need.min(1000) as i64;
    let awareness = game_vitality::ability(tx, p, "foresight")
        .min(game_vitality::ability(tx, p, "intelligence"))
        .min(16000) as i64;
    let urgent = tx
        .get::<game_character_state::Minds>()
        .map_or(100, |m| m.rules.urgent_hunger.clamp(1, 10000)) as i64;
    // Concern includes anticipated consequences of the perceived need, not remote bodily truth.
    let visible_immaturity = if local(tx, p, other) {
        tx.get::<game_physiology::Bodies>()
            .ok()
            .and_then(|b| b.people.get(&other))
            .map_or(0, |b| {
                1000 - game_vitality::maturation(b.born, tx.now(), "constitution")
            }) as i64
    } else {
        0
    };
    let fragility = visible_immaturity * awareness / (1000 + awareness);
    let severity = (need + need * need * awareness / (urgent * (1000 + awareness)))
        * (1000 + fragility)
        / 1000;
    let perceived = severity * (500 + empathy / 4 + compassion / 4) / 1000;
    perceived * altruism / 200 + attachment + i64::from(moral) * 3
        - body.hunger.min(1000) as i64 * 2
        - body.fatigue.min(1000) as i64
        - 30
}
pub fn candidates(tx: &Transaction<'_>, p: PersonId) -> Vec<PersonId> {
    let Ok(h) = tx.get::<Households>() else {
        return vec![];
    };
    let mut known = std::collections::BTreeSet::new();
    known.extend(h.beneficiaries_of(p));
    known.extend(h.children.get(&p).into_iter().flatten().copied());
    // A perceived need can motivate help without kinship or a prior commitment.
    if let Ok(area) = game_transport::area(tx.get().unwrap(), p)
        && let Ok(social) = tx.get::<game_social_life::SocialLife>()
        && let Some(peers) = social.residents.get(&area)
    {
        let mut peers: Vec<_> = peers.keys().copied().collect();
        if !peers.is_empty() {
            let encounters = social.people.get(&p).map_or(0, |s| s.encounters);
            let start = p.0.wrapping_add(encounters) as usize % peers.len();
            peers.rotate_left(start);
            known.extend(peers.into_iter().take(32));
        }
    }
    known.remove(&p);
    known
        .into_iter()
        .filter(|other| {
            tx.get::<game_physiology::Bodies>()
                .is_ok_and(|b| b.people.get(other).is_some_and(|b| b.alive))
        })
        .collect()
}
pub fn local(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> bool {
    game_transport::area(tx.get().unwrap(), p)
        .ok()
        .is_some_and(|area| game_transport::area(tx.get().unwrap(), other).ok() == Some(area))
}
pub fn observed_need(
    tx: &mut Transaction<'_>,
    observer: PersonId,
    person: PersonId,
) -> Result<u64> {
    if game_communication::active(tx) {
        if !local(tx, observer, person) {
            return Ok(game_communication::estimated_hunger(tx, observer, person).unwrap_or(0));
        }
        if game_choice::planning(tx) {
            require(
                !tx.has_before::<game_signals::Notice>(),
                "signal observation has hooks",
            )?;
            let assessment = game_signals::assess(tx, game_signals::Notice { observer, person })?;
            let need = assessment.food_need(tx);
            game_choice::defer(tx, game_signals::ApplyAssessment(assessment))?;
            return Ok(need);
        }
        let impression = tx.call(game_communication::Notice { observer, person })?;
        return Ok(if impression.guessed_need.as_deref() == Some("food") {
            impression.estimated_need
        } else {
            game_communication::estimated_hunger(tx, observer, person).unwrap_or(0)
        });
    }
    Ok(tx.get::<game_physiology::Bodies>()?.people[&person].hunger)
}
/// Record every signal through its owner. Filtering changes only the decision wake.
/// Failed observations retain the ordinary wake rather than hiding an error path.
pub fn notice_signal(
    tx: &mut Transaction<'_>,
    observer: PersonId,
    person: PersonId,
    changes_only: bool,
) -> bool {
    let before = changes_only
        .then(|| {
            tx.get::<game_signals::Signals>()
                .ok()
                .and_then(|s| s.perceptions.get(&observer)?.get(&person))
                .cloned()
        })
        .flatten();
    let result = tx.attempt(game_communication::Notice { observer, person });
    match result {
        Ok(after) if changes_only => food_changed(before.as_ref(), &after),
        _ => true,
    }
}
fn food_changed(
    before: Option<&game_signals::Perception>,
    after: &game_signals::Perception,
) -> bool {
    after.guessed_need.as_deref() == Some("food")
        && before.is_none_or(|before| {
            before.guessed_need != after.guessed_need
                || before.estimated_need != after.estimated_need
                || before.cue != after.cue
                || before.appetite != after.appetite
                || before.last_fed != after.last_fed
                || before.area != after.area
        })
}

//! Personal attraction combines inherited tastes, acquired ideals and perceived compatibility.
use game_core::*;
use game_sim::*;
/// Subjective attraction uses visible build and personally heard interests/convictions.
/// Another person's private values or genome are never consulted for compatibility.
pub fn attraction(tx: &Transaction<'_>, observer: PersonId, other: PersonId) -> i64 {
    let Ok(social) = tx.get::<game_relationships::Social>() else {
        return 0;
    };
    let appearance = if enabled() {
        physical::appearance(tx, observer, other, social)
    } else {
        physical::uncached(tx, observer, other, social)
    };
    companionship(tx, observer, other) + appearance
}

/// Affection and shared interests without romantic physical preferences.
pub fn companionship(tx: &Transaction<'_>, observer: PersonId, other: PersonId) -> i64 {
    let Ok(social) = tx.get::<game_relationships::Social>() else {
        return 0;
    };
    appraisal::emotional(social, observer, other)
        + if enabled() {
            appraisal::compatibility(social, observer, other)
        } else {
            appraisal::uncached(social, observer, other)
        }
}
mod appraisal;
mod physical;
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("LEGACY_APPRAISAL_CACHE").is_some_and(|v| v == "1"))
}

pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("attraction", 1)?;
    b.subscribe::<game_social_life::Encounter>("attraction.interests", |tx, e| {
        for (speaker, listener) in [(e.first, e.second), (e.second, e.first)] {
            if !game_vitality::can_converse(tx, speaker)
                || !game_vitality::can_converse(tx, listener)
            {
                continue;
            }
            let social = tx.get::<game_relationships::Social>()?;
            let privacy = social.opinion(speaker, "privacy");
            if game_vitality::disposition(tx, speaker, "sociability") / 10 < i64::from(privacy) {
                continue;
            }
            let regard = social.regard(speaker, listener);
            if regard > 20
                && (random(tx.seed(), speaker.0, tx.now().0) % 1000)
                    >= game_vitality::disposition(tx, speaker, "rigidity") as u64
            {
                for feature in ["size", "strength"] {
                    let apparent = (game_vitality::ability(tx, listener, feature) / 200 * 100)
                        .min(1000) as i32;
                    let outcome = ((apparent - 500) / 200).clamp(-2, 2);
                    if outcome != 0 {
                        tx.call(game_relationships::LearnValue {
                            person: speaker,
                            concept: format!("beauty:{feature}"),
                            outcome,
                        })?;
                    }
                }
            }
            let topic = tx
                .get::<game_relationships::Social>()?
                .values
                .get(&speaker)
                .and_then(|r| {
                    r.iter()
                        .filter(|(k, v)| **v != 0 && !k.starts_with("claim:"))
                        .max_by_key(|(k, v)| {
                            (
                                v.abs(),
                                game_core::random(tx.seed(), listener.0, game_choice::identity(k)),
                            )
                        })
                })
                .map(|(k, v)| (k.to_string(), *v));
            if let Some((concept, stance)) = topic {
                if game_communication::active(tx)
                    && !game_communication::understood(tx, speaker, listener, &concept, true)?
                {
                    continue;
                }
                tx.call(game_relationships::HearOpinion {
                    listener,
                    speaker,
                    via: speaker,
                    concept,
                    stance,
                    coerced: false,
                })?;
            }
        }
        Ok(())
    })?;
    Ok(())
}

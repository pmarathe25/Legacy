//! Personal attraction combines inherited tastes, acquired ideals and perceived compatibility.
use game_core::*;
use game_sim::*;
/// Subjective attraction uses visible build and personally heard interests/convictions.
/// Another person's private values or genome are never consulted for compatibility.
pub fn attraction(tx: &Transaction<'_>, observer: PersonId, other: PersonId) -> i64 {
    let Ok(social) = tx.get::<game_relationships::Social>() else {
        return 0;
    };
    let mut appearance = 0;
    if game_transport::area(tx.get().unwrap(), observer)
        .ok()
        .is_some_and(|a| game_transport::area(tx.get().unwrap(), other).ok() == Some(a))
    {
        for (feature, locus) in [("size", "taste-size"), ("strength", "taste-build")] {
            let ideal = (game_vitality::disposition(tx, observer, locus)
                + i64::from(social.opinion(observer, &format!("beauty:{feature}"))) * 5)
                .clamp(0, 1000);
            // Coarse apparent build, rather than exact capabilities or genetic potential.
            let apparent =
                (game_vitality::ability(tx, other, feature) / 200 * 100).min(1000) as i64;
            appearance += 50 - (ideal - apparent).abs() / 5;
        }
        appearance =
            appearance * game_vitality::disposition(tx, observer, "aesthetic-sensitivity") / 1000;
    }
    companionship(tx, observer, other) + appearance
}

/// Affection and shared interests without romantic physical preferences.
pub fn companionship(tx: &Transaction<'_>, observer: PersonId, other: PersonId) -> i64 {
    let Ok(social) = tx.get::<game_relationships::Social>() else {
        return 0;
    };
    let emotional = social
        .relationships
        .get(&observer)
        .and_then(|r| r.get(&other))
        .map_or(0, |r| {
            i64::from(r.affection) * 2 + i64::from(r.trust) + i64::from(r.admiration) / 2
                - i64::from(r.fear) * 3
                - i64::from(r.grievance) * 3
        });
    let mut compatibility = 0;
    if let Some(heard) = social.heard.get(&observer).and_then(|r| r.get(&other)) {
        let mut count = 0;
        for (concept, opinion) in heard {
            if opinion.coerced || concept.starts_with("claim:") {
                continue;
            }
            let own = social.opinion(observer, concept);
            if own == 0 {
                continue;
            }
            compatibility +=
                i64::from(own.abs() + opinion.stance.abs() - (own - opinion.stance).abs()) / 2;
            count += 1;
            if count == 8 {
                break;
            }
        }
    }
    emotional + compatibility.clamp(-200, 200)
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

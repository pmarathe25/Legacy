//! Personal advocacy composes heard testimony, private judgment, and physical pressure.
use game_core::*;
use game_relationships::{HearOpinion, Regard, ReviseOpinion, Social};
use game_sim::*;
use serde::{Deserialize, Serialize};
mod judgment;
mod policy;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Approach {
    Explain,
    Persuade,
    Cite { source: PersonId },
    Threaten,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Advocate {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub concept: String,
    pub stance: i32,
    pub approach: Approach,
}
impl Request for Advocate {
    const KEY: &'static str = "discourse.advocate";
    type Response = ();
}
/// Observable speech contains neither a hidden truth flag nor the listener's private response.
#[derive(Clone, Serialize, Deserialize)]
pub struct Spoken {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub concept: String,
    pub stance: i32,
    pub approach: Approach,
    pub quality: u32,
    pub coerced: bool,
}
impl Event for Spoken {
    const KEY: &'static str = "discourse.spoken";
}
fn local(tx: &Transaction<'_>, speaker: PersonId, listener: PersonId) -> bool {
    speaker != listener
        && [speaker, listener]
            .iter()
            .all(|p| game_physiology::capable(tx, *p).is_ok())
        && game_transport::area(tx.get().unwrap(), speaker)
            .ok()
            .is_some_and(|area| {
                game_transport::area(tx.get().unwrap(), listener).ok() == Some(area)
            })
}
fn speak(tx: &mut Transaction<'_>, r: Advocate) -> Result<()> {
    require(
        local(tx, r.speaker, r.listener),
        "advocacy requires contact",
    )?;
    require(
        game_vitality::can_converse(tx, r.speaker) && game_vitality::can_converse(tx, r.listener),
        "statement is not understood",
    )?;
    require(
        !r.concept.trim().is_empty() && (-100..=100).contains(&r.stance),
        "invalid statement",
    )?;
    if let Approach::Cite { source } = r.approach {
        let social = tx.get::<Social>()?;
        require(
            source != r.listener
                && source != r.speaker
                && (social
                    .relationships
                    .get(&r.speaker)
                    .is_some_and(|row| row.contains_key(&source))
                    || social.heard(r.speaker, source, &r.concept).is_some()),
            "source is unfamiliar",
        )?;
    }
    let intelligence = game_vitality::ability(tx, r.speaker, "intelligence");
    let experience = tx
        .get::<game_social_life::SocialLife>()
        .ok()
        .and_then(|s| s.people.get(&r.speaker))
        .map_or(0, |p| p.encounters.min(1000) / 100);
    let quality = ((intelligence / 80 + experience + 12).min(100)) as u32;
    let event = Spoken {
        speaker: r.speaker,
        listener: r.listener,
        concept: r.concept,
        stance: r.stance,
        approach: r.approach,
        quality,
        coerced: false,
    };
    if game_communication::active(tx) {
        let concept = format!("idea:{}", event.concept);
        let reception = game_communication::understood(
            tx,
            event.speaker,
            event.listener,
            &concept,
            matches!(event.approach, Approach::Explain),
        )?;
        if !reception {
            return Ok(());
        }
    }
    judgment::hear(tx, &event)?;
    tx.emit(event.clone(), vec![event.speaker, event.listener])?;
    if matches!(event.approach, Approach::Threaten) {
        let _ = tx.attempt(game_pressure::Press {
            leader: event.speaker,
            subject: event.listener,
            purpose: game_pressure::Purpose::Endorse {
                concept: event.concept,
                stance: event.stance,
            },
        });
    } else {
        let change = judgment::revision(tx, &event);
        if change != 0 {
            tx.call(ReviseOpinion {
                person: event.listener,
                concept: event.concept.clone(),
                change,
            })?;
        }
        if event.quality >= 60 {
            tx.call(Regard {
                observer: event.listener,
                other: event.speaker,
                trust: 0,
                respect: 1,
                admiration: i32::from(
                    event.quality >= 90
                        && tx.get::<Social>()?.opinion(event.listener, &event.concept)
                            * event.stance
                            > 0,
                ),
            })?;
        }
    }
    Ok(())
}
/// Persistent impressions belong to relationships; advocacy owns its requests and policy.
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("discourse", 1)?;
    b.request::<Advocate>(speak)?;
    b.subscribe::<game_social_life::Encounter>("zz.discourse.advocacy", |tx, e| {
        for (speaker, listener) in [(e.first, e.second), (e.second, e.first)] {
            policy::consider(tx, speaker, listener)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_pressure::Submitted>("discourse.endorsement", |tx, e| {
        let game_pressure::Purpose::Endorse { concept, stance } = &e.purpose else {
            return Ok(());
        };
        if game_communication::active(tx)
            && (game_communication::ability(tx, e.subject, "vocalization") < 80
                || game_communication::ability(tx, e.subject, "hearing") < 80
                || game_communication::ability(tx, e.subject, "interpretation") < 80
                || !game_communication::understood(
                    tx,
                    e.leader,
                    e.subject,
                    &format!("idea:{concept}"),
                    true,
                )?)
        {
            return Ok(());
        }
        if game_communication::active(tx)
            && !game_communication::understood(
                tx,
                e.subject,
                e.leader,
                &format!("idea:{concept}"),
                false,
            )?
        {
            return Ok(());
        }
        let event = Spoken {
            speaker: e.subject,
            listener: e.leader,
            concept: concept.clone(),
            stance: *stance,
            approach: Approach::Threaten,
            quality: 0,
            coerced: true,
        };
        judgment::hear(tx, &event)?;
        tx.call(game_relationships::LearnValue {
            person: e.leader,
            concept: "coercion".into(),
            outcome: 1,
        })?;
        tx.emit(event, vec![e.leader, e.subject])
    })?;
    b.subscribe::<game_physiology::Fed>("discourse.received-care", |tx, e| {
        if e.caregiver == e.patient {
            return Ok(());
        }
        let harmed = tx
            .get::<game_nutrition::Nutrition>()
            .ok()
            .and_then(|n| n.people.get(&e.patient))
            .and_then(|d| d.learned.get(&e.good))
            .is_some_and(|l| l.effects.health < 0);
        let warmth = if harmed {
            -3
        } else {
            let empathy = game_vitality::disposition(tx, e.patient, "empathy");
            let care = tx.get::<Social>()?.opinion(e.patient, "care");
            (empathy / 250 + i64::from(care) / 25).clamp(-5, 5) as i32
        };
        tx.call(game_relationships::Converse {
            observer: e.patient,
            other: e.caregiver,
            warmth,
        })?;
        tx.call(Regard {
            observer: e.patient,
            other: e.caregiver,
            trust: 0,
            respect: warmth.signum(),
            admiration: i32::from(warmth >= 4) - i32::from(harmed),
        })?;
        let caregiver_expects_harm = tx
            .get::<game_nutrition::Nutrition>()
            .ok()
            .and_then(|n| n.people.get(&e.caregiver))
            .and_then(|d| d.learned.get(&e.good))
            .is_some_and(|l| l.effects.health < 0);
        if !caregiver_expects_harm {
            tx.call(game_relationships::LearnValue {
                person: e.caregiver,
                concept: "care".into(),
                outcome: 1,
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_health::Treated>("discourse.treatment-regard", |tx, e| {
        if e.caregiver == e.patient {
            return Ok(());
        }
        tx.call(Regard {
            observer: e.patient,
            other: e.caregiver,
            trust: 0,
            respect: if e.successful { 2 } else { -1 },
            admiration: i32::from(e.successful),
        })
    })
}

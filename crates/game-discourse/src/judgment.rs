use super::*;
pub(super) fn hear(tx: &mut Transaction<'_>, e: &Spoken) -> Result<()> {
    let source = match e.approach {
        Approach::Cite { source } => source,
        _ => e.speaker,
    };
    let old = tx.call(HearOpinion {
        listener: e.listener,
        speaker: source,
        via: e.speaker,
        concept: e.concept.clone(),
        stance: e.stance,
        coerced: e.coerced,
    })?;
    // A later direct statement can contradict reported testimony. This is the
    // listener's inference about reliability, not an oracle of anyone's true belief.
    if let Some(old) = old.filter(|old| {
        !e.coerced
            && !old.coerced
            && old.via != source
            && source == e.speaker
            && (old.stance - e.stance).abs() >= 40
            && tx.now().0.saturating_sub(old.at.0) <= 30 * DAY.0
    }) {
        let social = tx.get::<Social>()?;
        let direct = social
            .relationships
            .get(&e.listener)
            .and_then(|r| r.get(&source));
        let reporter = social
            .relationships
            .get(&e.listener)
            .and_then(|r| r.get(&old.via));
        let regard =
            |r: Option<&game_relationships::Relationship>| r.map_or(0, |r| r.trust + r.respect);
        if regard(direct) >= regard(reporter) {
            tx.call(Regard {
                observer: e.listener,
                other: old.via,
                trust: -3,
                respect: -1,
                admiration: -1,
            })?;
        }
    }
    Ok(())
}
pub(super) fn revision(tx: &Transaction<'_>, e: &Spoken) -> i32 {
    let social = tx.get::<Social>().unwrap();
    let own = social.opinion(e.listener, &e.concept);
    if own == e.stance {
        return 0;
    }
    let tie = social
        .relationships
        .get(&e.listener)
        .and_then(|r| r.get(&e.speaker));
    let appeal = tie.map_or(0, |r| {
        i64::from(r.trust) / 3
            + i64::from(r.respect) / 3
            + i64::from(r.admiration) / 2
            + i64::from(r.affection) / 8
            - i64::from(r.grievance)
    });
    let openness = game_vitality::disposition(tx, e.listener, "openness");
    let conviction = game_vitality::disposition(tx, e.listener, "conviction");
    let foresight = game_vitality::ability(tx, e.listener, "foresight").min(16000);
    let argument = (u64::from(e.quality) * foresight / (1000 + foresight)) as i64;
    let independent = social.opinion(e.listener, "independence").max(0);
    let resistance = (i64::from(e.stance) - i64::from(own)).abs() / 2
        + conviction * i64::from(own).abs() / 1000
        + i64::from(independent) / 2;
    let strength = openness / 10
        + match e.approach {
            Approach::Explain => argument + appeal / 4,
            Approach::Persuade => appeal + argument / 4,
            Approach::Cite { source } => {
                let source = social
                    .relationships
                    .get(&e.listener)
                    .and_then(|r| r.get(&source));
                argument / 2
                    + appeal / 2
                    + source.map_or(0, |r| i64::from(r.trust + r.respect + r.admiration) / 3)
            }
            Approach::Threaten => return 0,
        };
    if strength <= resistance {
        return 0;
    }
    let amount = ((strength - resistance) / 30 + 1).clamp(1, 5) as i32;
    (i64::from(e.stance) - i64::from(own)).clamp(-i64::from(amount), i64::from(amount)) as i32
}

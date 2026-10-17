use super::*;
pub(super) fn consider(
    tx: &mut Transaction<'_>,
    speaker: PersonId,
    listener: PersonId,
) -> Result<()> {
    if Some(speaker) == tx.get::<game_lineage::Lineage>()?.current
        || !local(tx, speaker, listener)
        || !game_vitality::can_converse(tx, speaker)
        || !game_vitality::can_converse(tx, listener)
    {
        return Ok(());
    }
    let social = tx.get::<Social>()?;
    let budget = (1 + game_vitality::ability(tx, speaker, "intelligence") / 250).min(64) as usize;
    let mut topics: Vec<_> = ["sharing", "authority", "honesty"]
        .into_iter()
        .map(|key| (key.to_owned(), social.opinion(speaker, key)))
        .chain(
            social
                .values
                .get(&speaker)
                .into_iter()
                .flat_map(|v| v.iter())
                .take(budget)
                .map(|(key, value)| (key.to_string(), *value)),
        )
        .collect();
    for (_, stance) in &mut topics {
        *stance = (*stance).clamp(-100, 100);
    }
    topics.retain(|(_, stance)| stance.abs() >= 10);
    let Some((concept, stance)) = topics.into_iter().max_by_key(|(_, stance)| stance.abs()) else {
        return Ok(());
    };
    let assertiveness = game_vitality::disposition(tx, speaker, "assertiveness");
    let conviction = game_vitality::disposition(tx, speaker, "conviction");
    let body = &tx.get::<game_physiology::Bodies>()?.people[&speaker];
    let desire = assertiveness + conviction * i64::from(stance).abs() / 200
        - body.hunger.min(1000) as i64
        - body.fatigue.min(1000) as i64;
    let cue = random(tx.seed(), speaker.0 ^ listener.0, tx.now().0);
    if desire <= (cue % 1000) as i64 {
        return Ok(());
    }
    let foresight = game_vitality::ability(tx, speaker, "foresight").min(16000) as i64;
    let honesty = game_vitality::disposition(tx, speaker, "honesty");
    let aggression = game_vitality::disposition(tx, speaker, "aggression");
    let sociability = game_vitality::disposition(tx, speaker, "sociability");
    let tie = social
        .relationships
        .get(&speaker)
        .and_then(|r| r.get(&listener));
    let fear = tie.map_or(0, |r| i64::from(r.fear));
    let mut methods = vec![
        (
            foresight / 10 + game_vitality::disposition(tx, speaker, "openness") / 2,
            Approach::Explain,
        ),
        (
            sociability + tie.map_or(0, |r| i64::from(r.affection)),
            Approach::Persuade,
        ),
        (
            aggression
                + assertiveness / 2
                + conviction / 4
                + i64::from(social.opinion(speaker, "coercion")) * 4
                - fear * 4
                - i64::from(tie.map_or(0, |r| r.affection.max(0))),
            Approach::Threaten,
        ),
    ];
    let sources: Vec<_> = social
        .relationships
        .get(&speaker)
        .into_iter()
        .flat_map(|r| r.keys())
        .filter(|p| **p != listener && **p != speaker)
        .take(budget)
        .copied()
        .collect();
    if let Some(source) = sources
        .get((cue as usize / 1000) % sources.len().max(1))
        .copied()
    {
        let supported = social
            .heard(speaker, source, &concept)
            .is_some_and(|o| !o.coerced && o.stance == stance);
        let cost = if supported {
            0
        } else {
            honesty * 2 + i64::from(social.opinion(speaker, "honesty")) * 5
        };
        methods.push((
            1000 - honesty + assertiveness / 2 - cost,
            Approach::Cite { source },
        ));
    }
    let (_, approach) = methods
        .into_iter()
        .enumerate()
        .max_by_key(|(i, (score, _))| {
            (
                *score + (random(tx.seed(), speaker.0, cue.wrapping_add(*i as u64)) % 200) as i64,
                std::cmp::Reverse(*i),
            )
        })
        .unwrap()
        .1;
    let _ = tx.attempt(Advocate {
        speaker,
        listener,
        concept,
        stance: stance.clamp(-100, 100),
        approach,
    });
    Ok(())
}

use super::*;
fn offer<R: Request>(
    tx: &mut Transaction<'_>,
    person: PersonId,
    key: String,
    motive: game_choice::Candidate,
    request: R,
) -> Result<()> {
    game_choice::offer(
        tx,
        person,
        key,
        motive,
        game_choice::Undertake {
            person,
            kind: "exchange-information".into(),
            duration: SimDuration(2 * MINUTE.0),
            request: Pending::immediate(request),
        },
    )?;
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Consult {
    person: PersonId,
    document: u64,
    topic: Topic,
}
impl Request for Consult {
    const KEY: &'static str = "information.consult";
    type Response = game_choice::Outcome;
}
fn reading_ability(tx: &Transaction<'_>, person: PersonId, document: u64) -> u64 {
    let script = tx
        .get::<game_literacy::Literacy>()
        .ok()
        .and_then(|r| {
            let script = r.documents.get(&document)?.script;
            r.known.get(&person)?.get(&script).copied()
        })
        .unwrap_or(0);
    game_choice::identity(&format!(
        "{}:{}:{}:{}",
        script,
        game_vitality::ability(tx, person, "intelligence") / 250,
        game_vitality::ability(tx, person, "interpretation") >= 80,
        game_vitality::ability(tx, person, "learning") / 250
    ))
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_inquiry::Uncertain>("information.curiosity", |tx, e| {
        let importance = (game_vitality::disposition(tx, e.person, "openness")
            + game_vitality::disposition(tx, e.person, "contemplation"))
            / 40;
        if importance > 0 {
            tx.call(Want {
                person: e.person,
                topic: Topic::Category(format!("theory:{}:", e.subject)),
                importance: importance.min(10000),
            })?;
        }
        Ok(())
    })?;
    b.request::<Consult>(|tx, r| {
        let ability = reading_ability(tx, r.person, r.document);
        let result = tx.attempt(game_literacy::Read {
            person: r.person,
            document: r.document,
        });
        // Cache comprehension attempts, not temporary physical unavailability.
        if result.is_ok()
            && let Some(i) = tx
                .edit::<Information>()?
                .interests
                .get_mut(&r.person)
                .and_then(|row| row.iter_mut().find(|i| i.topic == r.topic))
            && i.read.len() < 64
        {
            i.read.insert(r.document, ability);
        }
        let learned = result.unwrap_or(false);
        if learned {
            tx.notify(game_choice::Changed { person: r.person })?;
        }
        Ok(learned.into())
    })?;
    game_choice::provider(b, "information.opportunities", |tx, e| {
        let p = e.person;
        if !game_choice::collecting(tx, p) || !game_vitality::can_converse(tx, p) {
            return Ok(());
        }
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let inbox = tx.get::<Information>()?.inboxes.shared(&p);
        for q in inbox.iter().flat_map(|row| row.iter()) {
            if !game_transport::together(tx.get()?, p, q.asker) {
                continue;
            }
            let regard = game_social_life::affinity(tx, p, q.asker);
            let generosity = game_vitality::disposition(tx, p, "altruism") / 20
                + game_vitality::disposition(tx, p, "sociability") / 40
                + regard;
            let privacy = tx
                .get::<game_relationships::Social>()
                .ok()
                .and_then(|s| s.values.get(&p)?.get("privacy"))
                .copied()
                .unwrap_or(0);
            let key = tx
                .get::<Knowledge>()?
                .0
                .get(&p)
                .and_then(|facts| q.topic.answer(facts));
            if let Some(key) = key {
                offer(
                    tx,
                    p,
                    format!("information:answer:{}", q.id),
                    game_choice::motive(0, 0, generosity - i64::from(privacy), 3, 0),
                    Answer {
                        person: p,
                        question: q.id,
                        key: Some(key),
                    },
                )?;
            }
            offer(
                tx,
                p,
                format!("information:decline:{}", q.id),
                game_choice::motive(0, 0, i64::from(privacy) - generosity, 1, 0),
                Answer {
                    person: p,
                    question: q.id,
                    key: None,
                },
            )?;
        }
        let Some(interests) = tx
            .get::<Information>()?
            .interests
            .shared(&p)
            .filter(|row| !row.is_empty())
        else {
            return Ok(());
        };
        let peers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()
            .ok()
            .and_then(|s| s.residents.get(&area))
            .into_iter()
            .flat_map(|row| row.keys())
            .filter(|other| **other != p)
            .take(16)
            .copied()
            .collect();
        for i in interests.iter() {
            let topic_id = game_choice::identity(&format!("{:?}", i.topic));
            for other in &peers {
                if i.asked.contains(other) || i.asked.len() >= 64 {
                    continue;
                }
                let regard = game_social_life::affinity(tx, p, *other);
                // We do not inspect prospective respondents' memories to choose whom to ask.
                offer(
                    tx,
                    p,
                    format!("information:ask:{}:{topic_id}", other.0),
                    game_choice::motive(0, i.importance, regard / 4, 3, 0),
                    Ask {
                        person: p,
                        other: *other,
                        topic: i.topic.clone(),
                    },
                )?;
            }
            if i.read.len() < 64
                && let Ok(records) = tx.get::<game_literacy::Literacy>()
            {
                let documents: Vec<_> = records
                    .seen
                    .get(&p)
                    .into_iter()
                    .flat_map(|seen| seen.keys())
                    .filter_map(|id| Some((*id, records.documents.get(id)?)))
                    .filter(|(id, d)| {
                        i.read
                            .get(id)
                            .is_none_or(|old| *old != reading_ability(tx, p, *id))
                            && tx
                                .get::<game_communication::Communication>()
                                .ok()
                                .and_then(|c| game_communication::shared_meaning(c, p, &d.sounds))
                                .is_some_and(|key| i.topic.relevant_title(&key))
                    })
                    .map(|(id, _)| id)
                    .take(8)
                    .collect();
                for document in documents {
                    offer(
                        tx,
                        p,
                        format!("information:read:{document}:{topic_id}"),
                        game_choice::motive(0, i.importance, 0, 5, 0),
                        Consult {
                            person: p,
                            document,
                            topic: i.topic.clone(),
                        },
                    )?;
                }
            }
        }
        Ok(())
    })
}

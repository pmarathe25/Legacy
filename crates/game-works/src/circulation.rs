use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Shared {
    pub work: u64,
    pub speaker: PersonId,
    pub listener: PersonId,
    pub written: bool,
}
impl Event for Shared {
    const KEY: &'static str = "works.shared";
}
impl Works {
    pub(super) fn remember(&mut self, person: PersonId, id: u64) {
        self.index().entry(person).or_default().insert(id, ());
    }
    fn index(&mut self) -> &mut Table<PersonId, Table<u64, ()>> {
        self.repertoire.get_or_insert_with(|| {
            let mut index: Table<PersonId, Table<u64, ()>> = Table::new();
            for (id, w) in &self.pieces {
                for p in std::iter::once(&w.creator)
                    .chain(w.audience.keys())
                    .chain(w.presented.keys())
                {
                    index.entry(*p).or_default().insert(*id, ());
                }
            }
            index
        })
    }
}
fn share(tx: &mut Transaction<'_>, speaker: PersonId, listener: PersonId) -> Result<()> {
    let state = tx.get::<Works>()?;
    let Some(known) = state.repertoire.as_ref().and_then(|r| r.get(&speaker)) else {
        return Ok(());
    };
    let count = tx.get::<game_social_life::SocialLife>()?.people[&speaker].encounters;
    let choices: Vec<_> = known
        .iter()
        .skip((count % known.len() as u64) as usize)
        .chain(known.iter())
        .take(known.len().min(8))
        .map(|(id, _)| *id)
        .collect();
    for id in choices {
        let w = &tx.get::<Works>()?.pieces[&id];
        let Some(c) = &w.composition else { continue };
        let a = &tx.get::<game_pastimes::Pastimes>()?.activities[&w.activity];
        let interest = tx
            .get::<game_relationships::Social>()?
            .opinion(speaker, &a.concept);
        if game_vitality::disposition(tx, speaker, &a.affinity) + i64::from(interest) * 8 <= 400 {
            continue;
        }
        let regard = tx
            .get::<game_relationships::Social>()?
            .regard(listener, speaker);
        if game_vitality::disposition(tx, listener, "contemplation") + regard * 4 <= 350 {
            continue;
        }
        let written = c.item.is_some();
        if c.item.is_some() {
            if w.presented.contains_key(&listener) {
                continue;
            }
            if tx
                .attempt(Present {
                    holder: speaker,
                    listener,
                    work: id,
                })
                .is_err()
            {
                continue;
            }
        } else {
            // The audience stores a recollection, not a copy of the creator's meanings.
            if (w.creator != speaker && !w.audience.contains_key(&speaker))
                || w.audience.contains_key(&listener)
                || w.creator == listener
                || !literature::understands(tx, speaker, listener, c)
            {
                continue;
            }
            let work = w.clone();
            let response = (game_vitality::disposition(tx, listener, "contemplation") / 100 - 4)
                .clamp(-10, 10) as i32;
            let concept = a.concept.clone();
            let words = game_communication::decode_many(
                tx.get()?,
                listener,
                &work.composition.as_ref().unwrap().sounds,
                true,
            );
            game_testimony::absorb(
                tx,
                listener,
                &work.evidence,
                words.iter().filter_map(|w| w.meaning.as_deref()),
            )?;
            tx.edit::<Works>()?
                .pieces
                .get_mut(&id)
                .unwrap()
                .audience
                .insert(listener, response);
            tx.call(game_relationships::LearnValue {
                person: listener,
                concept,
                outcome: response,
            })?;
            tx.edit::<Works>()?.remember(listener, id);
        }
        return tx.emit(
            Shared {
                work: id,
                speaker,
                listener,
                written,
            },
            vec![speaker, listener],
        );
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_social_life::Encounter>("works.circulate", |tx, e| {
        if !tx.get::<Works>()?.enabled || !game_communication::active(tx) {
            return Ok(());
        }
        // A conversation has already paid its time and obtained both participants' assent.
        tx.edit::<Works>()?.index();
        for (speaker, listener) in [(e.first, e.second), (e.second, e.first)] {
            share(tx, speaker, listener)?;
        }
        Ok(())
    })
}

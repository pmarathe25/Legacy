use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Genre {
    Poem,
    Song,
    Book,
    Story,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composition {
    pub genre: Genre,
    pub sounds: Vec<Vec<u8>>,
    /// Creator's intended readings at composition time, not a reader's knowledge.
    pub gloss: Vec<String>,
    pub line_ends: Vec<usize>,
    pub notes: Vec<i32>,
    pub item: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Read {
    pub reader: PersonId,
    pub work: u64,
}
impl Request for Read {
    const KEY: &'static str = "works.read";
    type Response = Vec<game_communication::Translation>;
}
pub(super) fn compose(
    tx: &mut Transaction<'_>,
    creator: PersonId,
    activity: &game_pastimes::Activity,
    evidence: &[(String, game_knowledge::Fact)],
) -> Result<Option<Composition>> {
    if !game_communication::active(tx) {
        return Ok(None);
    }
    let paper = tx.get::<game_objects::Objects>().ok().map_or(0, |objects| {
        activity
            .inputs
            .iter()
            .filter(|(g, _)| objects.by_id(**g).is_ok_and(|o| o.tags.contains("paper")))
            .map(|(_, q)| *q)
            .sum::<u64>()
    });
    let genre = match activity.skill.as_str() {
        "music" => Genre::Song,
        "writing" if paper >= 4 => Genre::Book,
        "writing" => Genre::Poem,
        _ => Genre::Story,
    };
    let mut concepts = vec![
        format!("person:{}", creator.0),
        format!("idea:{}", activity.concept),
    ];
    concepts.extend(evidence.iter().take(6).map(|(key, _)| key.clone()));
    let mut gloss = vec![];
    let mut sounds = vec![];
    for key in &concepts {
        sounds.push(tx.call(game_communication::WordFor {
            person: creator,
            concept: key.clone(),
        })?);
        gloss.push(key.clone());
    }
    let width = sounds.len();
    let line_ends = match genre {
        Genre::Poem | Genre::Song => {
            sounds.extend([sounds[0].clone(), sounds[1].clone(), sounds[0].clone()]);
            gloss.extend([
                concepts[0].clone(),
                concepts[1].clone(),
                concepts[0].clone(),
            ]);
            vec![width, sounds.len()]
        }
        _ => vec![width],
    };
    let notes = if genre == Genre::Song {
        sounds
            .iter()
            .map(|w| (w.iter().map(|p| i32::from(*p)).sum::<i32>() % 12) + 48)
            .collect()
    } else {
        vec![]
    };
    let item = if paper > 0 && tx.registered::<game_items::Mark>() {
        let key = if genre == Genre::Book {
            "bound-book"
        } else {
            "written-page"
        };
        let good = tx.get::<game_objects::Objects>()?.resolve(key)?;
        let inventory = tx.get::<game_cargo::Cargo>()?.carried(creator).next();
        if let Some(inventory) = inventory {
            if tx
                .attempt(game_cargo::Deposit {
                    inventory,
                    good,
                    quantity: 1,
                })
                .is_ok()
            {
                Some(tx.call(game_items::Mark {
                    person: creator,
                    inventory,
                    good,
                    label: game_communication::render(&sounds[0]),
                })?)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(Composition {
        genre,
        sounds,
        gloss,
        line_ends,
        notes,
        item,
    }))
}
pub(super) fn understands(
    tx: &Transaction<'_>,
    performer: PersonId,
    p: PersonId,
    c: &Composition,
) -> bool {
    if game_communication::ability(tx, performer, "vocalization") < 80
        || game_communication::ability(tx, p, "hearing") < 80
        || game_communication::ability(tx, p, "interpretation") < 80
    {
        return false;
    }
    c.sounds.iter().any(|word| {
        game_communication::decode(tx.get().unwrap(), p, word)
            .meaning
            .is_some()
    })
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Present {
    pub holder: PersonId,
    pub listener: PersonId,
    pub work: u64,
}
impl Request for Present {
    const KEY: &'static str = "works.present";
    type Response = ();
}
fn accessible(
    cargo: &game_cargo::Cargo,
    positions: &game_transport::Positions,
    reader: PersonId,
    inventory: InventoryId,
    shown: bool,
) -> bool {
    let Some(inventory) = cargo.0.get(&inventory) else {
        return false;
    };
    game_transport::area(positions, reader)
        .ok()
        .is_some_and(|a| inventory.area(positions) == Some(a))
        && (inventory.custody == game_cargo::Custody::Carried(reader)
            || matches!(inventory.custody, game_cargo::Custody::Area(_))
            || shown)
}
fn translations(
    c: &game_communication::Communication,
    reader: PersonId,
    sounds: &[Vec<u8>],
    able: bool,
) -> Vec<game_communication::Translation> {
    sounds
        .iter()
        .map(|w| {
            if able {
                game_communication::decode(c, reader, w)
            } else {
                game_communication::Translation {
                    surface: game_communication::render(w),
                    meaning: None,
                    confidence: 0,
                }
            }
        })
        .collect()
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Present>(|tx, r| {
        game_physiology::capable(tx, r.holder)?;
        game_physiology::capable(tx, r.listener)?;
        require(
            game_transport::area(tx.get()?, r.holder)?
                == game_transport::area(tx.get()?, r.listener)?,
            "listener is elsewhere",
        )?;
        let item = tx
            .get::<Works>()?
            .pieces
            .get(&r.work)
            .and_then(|w| w.composition.as_ref())
            .and_then(|c| c.item)
            .ok_or_else(|| Error::Rejected("no physical text".into()))?;
        let inventory = tx
            .get::<game_items::Items>()?
            .objects
            .get(&item)
            .and_then(|i| i.inventory)
            .ok_or_else(|| Error::Rejected("text was destroyed".into()))?;
        require(
            accessible(tx.get()?, tx.get()?, r.holder, inventory, false),
            "text is not in hand or exposed",
        )?;
        tx.call(game_property::Use {
            actor: r.holder,
            inventory,
            purpose: "showing a text".into(),
        })?;
        tx.edit::<Works>()?
            .pieces
            .get_mut(&r.work)
            .unwrap()
            .presented
            .insert(r.listener, ());
        Ok(())
    })?;
    b.request::<Read>(|tx, r| {
        game_physiology::capable(tx, r.reader)?;
        let work = tx
            .get::<Works>()?
            .pieces
            .get(&r.work)
            .ok_or_else(|| Error::Missing("work".into()))?
            .clone();
        let c = work
            .composition
            .ok_or_else(|| Error::Rejected("work has no transcription".into()))?;
        if let Some(item) = c.item {
            let inventory = tx
                .get::<game_items::Items>()?
                .objects
                .get(&item)
                .and_then(|i| i.inventory)
                .ok_or_else(|| Error::Rejected("text was destroyed".into()))?;
            require(
                accessible(
                    tx.get()?,
                    tx.get()?,
                    r.reader,
                    inventory,
                    work.presented.contains_key(&r.reader),
                ),
                "text is not physically exposed",
            )?;
            tx.call(game_property::Use {
                actor: r.reader,
                inventory,
                purpose: "reading".into(),
            })?;
        } else {
            require(
                r.reader == work.creator || work.audience.contains_key(&r.reader),
                "work was never experienced",
            )?;
        }
        Ok(translations(
            tx.get()?,
            r.reader,
            &c.sounds,
            game_communication::ability(tx, r.reader, "interpretation") >= 80,
        ))
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkView {
    pub id: u64,
    pub title: String,
    pub genre: Genre,
    pub translation: Vec<game_communication::Translation>,
    pub line_ends: Vec<usize>,
    pub notes: Vec<i32>,
}
pub fn view(h: &Host, reader: PersonId) -> Result<Vec<WorkView>> {
    let Ok(_) = game_transport::area(h.get()?, reader) else {
        return Ok(vec![]);
    };
    let mut seen = vec![];
    for (id, work) in &h.get::<Works>()?.pieces {
        let Some(c) = &work.composition else {
            continue;
        };
        let visible = if let Some(item) = c.item {
            h.get::<game_items::Items>()?
                .objects
                .get(&item)
                .and_then(|i| i.inventory)
                .is_some_and(|i| {
                    accessible(
                        h.get::<game_cargo::Cargo>().unwrap(),
                        h.get::<game_transport::Positions>().unwrap(),
                        reader,
                        i,
                        work.presented.contains_key(&reader),
                    )
                })
        } else {
            work.creator == reader || work.audience.contains_key(&reader)
        };
        if visible {
            seen.push(WorkView {
                id: *id,
                title: work.title.clone(),
                genre: c.genre.clone(),
                translation: translations(
                    h.get()?,
                    reader,
                    &c.sounds,
                    game_vitality::expressed(
                        h.get::<game_traits::Traits>().ok(),
                        h.get::<game_vitality::Vitality>()
                            .ok()
                            .and_then(|v| v.people.get(&reader)),
                        reader,
                        "interpretation",
                        h.now(),
                    ) >= 80,
                ),
                line_ends: c.line_ends.clone(),
                notes: c.notes.clone(),
            });
            if seen.len() == 32 {
                break;
            }
        }
    }
    Ok(seen)
}

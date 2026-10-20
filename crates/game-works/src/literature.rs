use super::*;
use game_cargo::Cargo;
use game_communication as language;
use game_items::Items;
pub use game_poetics::Genre;
use game_transport::Positions;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composition {
    pub genre: Genre,
    pub sounds: Vec<Vec<u8>>,
    /// Creator's intended readings at composition time, not a reader's knowledge.
    pub gloss: Vec<String>,
    pub line_ends: Vec<usize>,
    pub notes: Vec<i32>,
    pub item: Option<u64>,
    #[serde(default)]
    pub script: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Read {
    pub reader: PersonId,
    pub work: u64,
}
impl Request for Read {
    const KEY: &'static str = "works.read";
    type Response = Vec<language::Translation>;
}
pub(super) fn compose(
    tx: &mut Transaction<'_>,
    creator: PersonId,
    activity: &game_pastimes::Activity,
    evidence: &[(String, game_knowledge::Fact)],
    inscription: Option<&game_inscription::Imprint>,
) -> Result<Option<Composition>> {
    if !language::active(tx) {
        return Ok(None);
    }
    let script = if inscription.is_some() {
        tx.call(game_literacy::Practice { person: creator })?
    } else {
        None
    };
    if inscription.is_some() && script.is_none() {
        return Ok(None);
    }
    let genre = match activity.skill.as_str() {
        "music" => Genre::Song,
        "writing" if activity.duration.0 >= 8 * HOUR.0 => Genre::Book,
        "writing" => Genre::Poem,
        _ => Genre::Story,
    };
    let mut gloss = vec![
        format!("person:{}", creator.0),
        format!("idea:{}", activity.concept),
    ];
    gloss.extend(evidence.iter().take(6).map(|(key, _)| key.clone()));
    let mut sounds = vec![];
    for key in &gloss {
        sounds.push(tx.call(language::WordFor {
            person: creator,
            concept: key.clone(),
        })?);
    }
    for (_, fact) in evidence.iter().take(2) {
        if let Some(scene) = fact
            .value
            .decode::<std::collections::BTreeMap<String, String>>()
            && scene.contains_key("@predicate")
        {
            for concept in scene.values() {
                sounds.push(tx.call(language::WordFor {
                    person: creator,
                    concept: concept.clone(),
                })?);
                gloss.push(concept.clone());
            }
        }
    }
    let phrasing = game_poetics::arrange(&genre, &mut sounds, &mut gloss);
    let item = inscription.map(|i| i.item);
    Ok(Some(Composition {
        genre,
        script,
        sounds,
        gloss,
        line_ends: phrasing.line_ends,
        notes: phrasing.notes,
        item,
    }))
}
pub(super) fn understands(
    tx: &Transaction<'_>,
    performer: PersonId,
    p: PersonId,
    c: &Composition,
) -> bool {
    language::ability(tx, performer, "vocalization") >= 80
        && language::ability(tx, p, "hearing") >= 80
        && language::ability(tx, p, "interpretation") >= 80
        && c.sounds
            .iter()
            .any(|w| language::decode(tx.get().unwrap(), p, w).meaning.is_some())
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
enum Access {
    Text(InventoryId),
    Memory,
}
fn exposure(
    items: &Items,
    cargo: &Cargo,
    positions: &Positions,
    p: PersonId,
    w: &Work,
) -> Option<Access> {
    let c = w.composition.as_ref()?;
    if let Some(id) = c.item {
        let i = items.objects.get(&id)?.inventory?;
        game_items::exposed_in(
            items,
            cargo,
            positions,
            p,
            id,
            w.presenter.filter(|_| w.presented.contains_key(&p)),
        )
        .then_some(Access::Text(i))
    } else {
        (w.creator == p || w.audience.contains_key(&p)).then_some(Access::Memory)
    }
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<game_literacy::WritingModel>(|tx, r| {
        Ok(tx.get::<Works>()?.pieces.values().any(|w| {
            w.composition.as_ref().is_some_and(|c| {
                c.script == Some(r.script)
                    && c.item
                        .is_some_and(|id| game_items::exposed(tx, r.person, id, None))
            })
        }))
    })?;
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
            .get::<Items>()?
            .objects
            .get(&item)
            .and_then(|i| i.inventory)
            .ok_or_else(|| Error::Rejected("text was destroyed".into()))?;
        require(
            game_items::exposed(tx, r.holder, item, None),
            "text is not in hand or exposed",
        )?;
        tx.call(game_property::Use {
            actor: r.holder,
            inventory,
            purpose: "showing a text".into(),
        })?;
        let work = tx.edit::<Works>()?.pieces.get_mut(&r.work).unwrap();
        work.presented.insert(r.listener, ());
        work.presenter = Some(r.holder);
        tx.edit::<Works>()?.remember(r.listener, r.work);
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
        let access = exposure(tx.get()?, tx.get()?, tx.get()?, r.reader, &work)
            .ok_or_else(|| Error::Rejected("work is not exposed or remembered".into()))?;
        if let Access::Text(inventory) = access {
            tx.call(game_property::Use {
                actor: r.reader,
                inventory,
                purpose: "reading".into(),
            })?;
        }
        let composition = work.composition.as_ref().unwrap();
        let translated = language::decode_many(
            tx.get()?,
            r.reader,
            &composition.sounds,
            language::ability(tx, r.reader, "interpretation") >= 80
                && game_literacy::understands(tx.get().ok(), r.reader, composition.script),
        );
        game_testimony::absorb(
            tx,
            r.reader,
            &work.evidence,
            translated.iter().filter_map(|w| w.meaning.as_deref()),
        )?;
        Ok(translated)
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkView {
    pub id: u64,
    pub title: String,
    pub genre: Genre,
    pub translation: Vec<language::Translation>,
    pub line_ends: Vec<usize>,
    pub notes: Vec<i32>,
}
pub fn view(h: &Host, reader: PersonId) -> Result<Vec<WorkView>> {
    let Ok(_) = game_transport::area(h.get()?, reader) else {
        return Ok(vec![]);
    };
    let life = h
        .get::<game_vitality::Vitality>()
        .ok()
        .and_then(|v| v.people.get(&reader));
    let able = game_vitality::expressed(
        h.get::<game_traits::Traits>().ok(),
        life,
        reader,
        "interpretation",
        h.now(),
    ) >= 80;
    let mut seen = vec![];
    for (id, work) in &h.get::<Works>()?.pieces {
        let Some(c) = &work.composition else {
            continue;
        };
        if exposure(h.get()?, h.get()?, h.get()?, reader, work).is_some() {
            seen.push(WorkView {
                id: *id,
                title: work.title.clone(),
                genre: c.genre.clone(),
                translation: language::decode_many(
                    h.get()?,
                    reader,
                    &c.sounds,
                    able && game_literacy::understands(h.get().ok(), reader, c.script),
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

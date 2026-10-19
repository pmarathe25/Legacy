//! Invented symbol systems and physical notes preserve personally held claims.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod choices;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub creator: PersonId,
    pub glyphs: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub author: PersonId,
    pub script: u64,
    pub item: u64,
    pub key: String,
    pub sounds: Vec<u8>,
    pub glyphs: Vec<u64>,
    pub fact: game_knowledge::Fact,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Literacy {
    pub scripts: Table<u64, Script>,
    pub known: Table<PersonId, Table<u64, u8>>,
    pub documents: Table<u64, Document>,
    pub seen: Table<PersonId, Table<u64, ()>>,
    #[serde(default)]
    pub shown: Table<u64, PersonId>,
}
impl Literacy {
    fn notice(&mut self, person: PersonId, item: u64) {
        self.seen.entry(person).or_default().insert(item, ());
    }
}
impl Resource for Literacy {
    const KEY: &'static str = "literacy.records";
    fn allocated_ids(&self) -> Vec<u64> {
        self.scripts.keys().copied().collect()
    }
    fn validate(&self) -> Result<()> {
        require(
            self.scripts.values().all(|s| s.glyphs.len() == 256)
                && self.known.values().all(|row| {
                    row.iter()
                        .all(|(s, p)| self.scripts.contains_key(s) && *p > 0 && *p <= 100)
                }),
            "unknown script proficiency",
        )?;
        require(
            self.documents.values().all(|d| {
                self.scripts.contains_key(&d.script) && !d.glyphs.is_empty() && !d.key.is_empty()
            }),
            "invalid written record",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Develop {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for Develop {
    const KEY: &'static str = "literacy.develop";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Invent {
    person: PersonId,
}
impl Request for Invent {
    const KEY: &'static str = "literacy.invent";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Inscribe {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub key: String,
    pub script: u64,
}
impl Request for Inscribe {
    const KEY: &'static str = "literacy.inscribe";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Write {
    person: PersonId,
    inventory: InventoryId,
    key: String,
    script: u64,
    fact: game_knowledge::Fact,
}
impl Request for Write {
    const KEY: &'static str = "literacy.write";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Read {
    pub person: PersonId,
    pub document: u64,
}
impl Request for Read {
    const KEY: &'static str = "literacy.read";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Show {
    pub person: PersonId,
    pub listener: PersonId,
    pub document: u64,
}
impl Request for Show {
    const KEY: &'static str = "literacy.show";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Teach {
    pub teacher: PersonId,
    pub student: PersonId,
    pub script: u64,
}
impl Request for Teach {
    const KEY: &'static str = "literacy.teach";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Recorded {
    pub person: PersonId,
    pub document: u64,
    pub key: String,
}
impl Event for Recorded {
    const KEY: &'static str = "literacy.recorded";
}
fn local(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    require(
        game_transport::area(tx.get()?, a)? == game_transport::area(tx.get()?, b)?,
        "instruction needs contact",
    )
}
fn paper(tx: &mut Transaction<'_>, p: PersonId, inventory: InventoryId) -> Result<()> {
    game_physiology::capable(tx, p)?;
    require(
        game_vitality::can_converse(tx, p),
        "cannot formulate a written statement",
    )?;
    require(
        game_cargo::locality(tx, inventory)? == game_transport::area(tx.get()?, p)?,
        "writing materials are elsewhere",
    )?;
    tx.call(game_property::Use {
        actor: p,
        inventory,
        purpose: "writing".into(),
    })?;
    for key in ["paper", "ink"] {
        let good = tx.get::<game_objects::Objects>()?.resolve(key)?;
        tx.call(game_cargo::Consume {
            inventory,
            good,
            quantity: 1,
        })?;
    }
    Ok(())
}
fn exposed(tx: &Transaction<'_>, p: PersonId, d: &Document) -> bool {
    let literacy = tx.get::<Literacy>().unwrap();
    literacy
        .seen
        .get(&p)
        .is_some_and(|r| r.contains_key(&d.item))
        && game_items::exposed(tx, p, d.item, literacy.shown.get(&d.item).copied())
}
fn first_script(tx: &Transaction<'_>, p: PersonId) -> Option<u64> {
    let known = tx.get::<Literacy>().ok()?.known.get(&p)?;
    known
        .iter()
        .find_map(|(s, level)| (*level >= 40).then_some(*s))
}
/// Used after paid writing practice by literary and empirical composition owners.
#[derive(Clone, Serialize, Deserialize)]
pub struct Practice {
    pub person: PersonId,
}
impl Request for Practice {
    const KEY: &'static str = "literacy.practice";
    type Response = Option<u64>;
}
fn proficiency(literacy: &Literacy, person: PersonId, script: u64) -> u8 {
    let row = literacy.known.get(&person);
    row.and_then(|r| r.get(&script)).copied().unwrap_or(0)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WritingModel {
    pub person: PersonId,
    pub script: u64,
}
impl Request for WritingModel {
    const KEY: &'static str = "literacy.writing-model";
    type Response = bool;
}
pub fn understands(literacy: Option<&Literacy>, person: PersonId, script: Option<u64>) -> bool {
    script.is_none_or(|s| literacy.is_some_and(|l| proficiency(l, person, s) >= 40))
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("literacy", 1)?;
    b.resource::<Literacy>()?;
    b.request::<Practice>(|tx, r| {
        if let Some(script) = first_script(tx, r.person) {
            return Ok(Some(script));
        }
        tx.call(Invent { person: r.person })?;
        Ok(first_script(tx, r.person))
    })?;
    choices::register(b)?;
    b.request::<Develop>(|tx, r| {
        paper(tx, r.person, r.inventory)?;
        tx.call(game_choice::Undertake {
            person: r.person,
            kind: "develop-writing".into(),
            duration: SimDuration(4 * HOUR.0),
            request: Pending::new(Invent { person: r.person })?,
        })
    })?;
    b.request::<Invent>(|tx, r| {
        let capacity = game_vitality::ability(tx, r.person, "intelligence")
            .saturating_add(game_vitality::ability(tx, r.person, "learning"));
        if random(tx.seed(), r.person.0, tx.now().0) % 4000 >= capacity.min(4000) {
            return Ok(false);
        }
        let script = tx.allocate()?;
        let glyphs = (0..256)
            .map(|n| {
                random(tx.seed(), script, 0)
                    .wrapping_add((n as u64).wrapping_mul(0x9e3779b97f4a7c15))
            })
            .collect();
        tx.edit::<Literacy>()?.scripts.insert(
            script,
            Script {
                creator: r.person,
                glyphs,
            },
        );
        tx.edit::<Literacy>()?
            .known
            .entry(r.person)
            .or_default()
            .insert(script, 100);
        Ok(true)
    })?;
    b.request::<Inscribe>(|tx, r| {
        require(
            proficiency(tx.get()?, r.person, r.script) >= 40,
            "writing system is unfamiliar",
        )?;
        let fact = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&r.person)
            .and_then(|k| k.get(r.key.as_str()))
            .map(|f| f.to_owned())
            .ok_or_else(|| Error::Rejected("writer does not know this claim".into()))?;
        paper(tx, r.person, r.inventory)?;
        tx.call(game_choice::Undertake {
            person: r.person,
            kind: "write-note".into(),
            duration: HOUR,
            request: Pending::new(Write {
                person: r.person,
                inventory: r.inventory,
                key: r.key,
                script: r.script,
                fact,
            })?,
        })
    })?;
    b.request::<Write>(|tx, r| {
        require(
            game_cargo::locality(tx, r.inventory)? == game_transport::area(tx.get()?, r.person)?,
            "writer left the materials",
        )?;
        let sounds = tx.call(game_communication::WordFor {
            person: r.person,
            concept: r.key.clone(),
        })?;
        let alphabet = &tx.get::<Literacy>()?.scripts[&r.script].glyphs;
        let encoded = serde_json::to_vec(&(r.person, r.key.clone(), r.fact.clone())).unwrap();
        let glyphs = encoded
            .iter()
            .map(|byte| alphabet[usize::from(*byte)])
            .collect();
        let good = tx.get::<game_objects::Objects>()?.resolve("written-page")?;
        tx.call(game_cargo::Deposit {
            inventory: r.inventory,
            good,
            quantity: 1,
        })?;
        let item = tx.call(game_items::Mark {
            person: r.person,
            inventory: r.inventory,
            good,
            label: game_communication::render(&sounds),
        })?;
        tx.edit::<Literacy>()?.documents.insert(
            item,
            Document {
                author: r.person,
                script: r.script,
                item,
                key: r.key.clone(),
                sounds,
                glyphs,
                fact: r.fact,
            },
        );
        tx.edit::<Literacy>()?.notice(r.person, item);
        tx.emit(
            Recorded {
                person: r.person,
                document: item,
                key: r.key,
            },
            vec![r.person],
        )?;
        Ok(item)
    })?;
    b.request::<Show>(|tx, r| {
        local(tx, r.person, r.listener)?;
        let document = tx
            .get::<Literacy>()?
            .documents
            .get(&r.document)
            .ok_or_else(|| Error::Missing("document".into()))?;
        require(exposed(tx, r.person, document), "text is unavailable")?;
        let holder = game_items::holder(tx, r.document).unwrap_or(r.person);
        tx.edit::<Literacy>()?.shown.insert(r.document, holder);
        tx.edit::<Literacy>()?.notice(r.listener, r.document);
        Ok(())
    })?;
    b.request::<Teach>(|tx, r| {
        let model = tx
            .get::<Literacy>()?
            .documents
            .values()
            .any(|d| d.script == r.script && exposed(tx, r.teacher, d))
            || tx.registered::<WritingModel>()
                && tx.call(WritingModel {
                    person: r.teacher,
                    script: r.script,
                })?;
        require(
            game_communication::instruction(
                tx,
                r.teacher,
                r.student,
                &format!("script:{}", r.script),
                model,
            )?,
            "no usable teaching channel",
        )?;
        require(
            proficiency(tx.get()?, r.teacher, r.script) >= 40,
            "teacher does not know the script",
        )?;
        let regard = tx
            .get::<game_relationships::Social>()
            .ok()
            .and_then(|s| s.relationships.get(&r.student)?.get(&r.teacher))
            .map_or(0, |r| i64::from(r.trust + r.respect - r.grievance));
        if !game_vitality::can_converse(tx, r.student)
            || game_vitality::disposition(tx, r.student, "openness") / 10 + regard < 30
        {
            return Ok(false);
        }
        let amount = (game_vitality::ability(tx, r.student, "learning") / 50).clamp(1, 50) as u8;
        let level = tx
            .edit::<Literacy>()?
            .known
            .entry(r.student)
            .or_default()
            .entry(r.script)
            .or_default();
        *level = level.saturating_add(amount).min(100);
        Ok(true)
    })?;
    b.request::<Read>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            game_vitality::ability(tx, r.person, "interpretation") >= 80,
            "cannot interpret the marks",
        )?;
        let d = tx
            .get::<Literacy>()?
            .documents
            .get(&r.document)
            .cloned()
            .ok_or_else(|| Error::Missing("document".into()))?;
        require(exposed(tx, r.person, &d), "text is unavailable")?;
        let proficiency = proficiency(tx.get()?, r.person, d.script);
        let word = game_communication::decode(tx.get()?, r.person, &d.sounds);
        if proficiency < 40 || word.meaning.as_deref() != Some(d.key.as_str()) {
            return Ok(false);
        }
        let alphabet = &tx.get::<Literacy>()?.scripts[&d.script].glyphs;
        let bytes = d
            .glyphs
            .iter()
            .map(|glyph| {
                alphabet
                    .iter()
                    .position(|g| g == glyph)
                    .and_then(|i| u8::try_from(i).ok())
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| Error::Rejected("unreadable glyphs".into()))?;
        let (author, key, mut fact): (PersonId, String, game_knowledge::Fact) =
            serde_json::from_slice(&bytes)
                .map_err(|_| Error::Rejected("damaged assertion".into()))?;
        if let Some(claim) = fact.value.decode::<game_reasoning::Claim>() {
            let understanding = game_vitality::ability(tx, r.person, "intelligence")
                .saturating_mul(1000 + game_vitality::ability(tx, r.person, "learning") / 8)
                / 1000;
            if understanding < 100 + u64::from(claim.complexity) * 75 {
                return Ok(false);
            }
        }
        fact.source = author;
        fact.confidence = fact
            .confidence
            .min(proficiency)
            .min(word.confidence.min(100) as u8);
        tx.call(game_knowledge::Receive {
            person: r.person,
            key,
            fact,
        })?;
        Ok(true)
    })
}

//! Distinct works preserve the creator's actual personal evidence and local interpretations.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod circulation;
mod literature;
pub use circulation::Shared;
pub use literature::{Composition, Genre, Present, Read, WorkView, view};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub creator: PersonId,
    pub area: AreaId,
    pub at: SimTime,
    pub title: String,
    pub activity: String,
    pub evidence: Vec<(String, game_knowledge::Fact)>,
    pub audience: Table<PersonId, i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<Composition>,
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub presented: Table<PersonId, ()>,
    #[serde(default)]
    pub presenter: Option<PersonId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Works {
    pub enabled: bool,
    pub pieces: Table<u64, Work>,
    /// Legacy archive deadlines retained for save compatibility; no longer gate creation.
    pub next: Table<PersonId, SimTime>,
    /// Rebuilt from personal experience after loading; never a source of knowledge.
    #[serde(skip)]
    repertoire: Option<Table<PersonId, Table<u64, ()>>>,
}
impl Resource for Works {
    const KEY: &'static str = "works.creations";
    fn validate_changes(&self, old: &Self) -> Result<()> {
        validate(self.pieces.changed(&old.pieces).map(|(_, w)| w))
    }
    fn validate(&self) -> Result<()> {
        validate(self.pieces.values())
    }

    fn allocated_ids(&self) -> Vec<u64> {
        self.pieces.keys().copied().collect()
    }
}
fn validate<'a>(mut pieces: impl Iterator<Item = &'a Work>) -> Result<()> {
    require(
        pieces.all(|w| {
            !w.title.is_empty()
                && w.composition.as_ref().is_none_or(|c| {
                    !c.sounds.is_empty()
                        && c.gloss.len() == c.sounds.len()
                        && c.sounds.iter().all(|s| !s.is_empty())
                        && c.sounds
                            .iter()
                            .flatten()
                            .all(|p| usize::from(*p) < game_communication::PHONEMES.len())
                        && c.line_ends.last() == Some(&c.sounds.len())
                        && c.line_ends.windows(2).all(|p| p[0] < p[1])
                        && (c.notes.is_empty() || c.notes.len() == c.sounds.len())
                })
        }),
        "invalid composition",
    )
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "works.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Created {
    pub work: u64,
    pub creator: PersonId,
    pub title: String,
    pub audience: usize,
}
impl Event for Created {
    const KEY: &'static str = "works.created";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_literacy::plugin(b)?;
    game_recollection::plugin(b)?;
    b.plugin("works", 1)?;
    b.resource::<Works>()?;
    literature::register(b)?;
    circulation::register(b)?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Works>()?.enabled = true;
        Ok(())
    })?;
    b.subscribe::<game_pastimes::Experienced>("works.testimony", |tx, e| {
        if !e.completed || !tx.get::<Works>()?.enabled {
            return Ok(());
        }
        let activity = tx.get::<game_pastimes::Pastimes>()?.activities[&e.activity].clone();
        if !["writing", "music", "art", "storytelling"].contains(&activity.skill.as_str()) {
            return Ok(());
        }
        let Ok(area) = game_transport::area(tx.get()?, e.person) else {
            return Ok(());
        };
        let evidence = game_recollection::inspiration(tx, e.person);
        if evidence.is_empty() {
            return Ok(());
        }
        let composition =
            literature::compose(tx, e.person, &activity, &evidence, e.inscription.as_ref())?;
        if composition.as_ref().is_some_and(|c| c.item.is_none())
            && tx
                .get::<Works>()?
                .pieces
                .values()
                .any(|w| w.creator == e.person && w.composition == composition)
        {
            return Ok(());
        }
        let title = composition
            .as_ref()
            .map(|c| game_communication::render(&c.sounds[0]))
            .unwrap_or_else(|| format!("{}: {}", activity.name, evidence[0].0));
        let id = tx.allocate()?;
        let now = tx.now();
        tx.edit::<Works>()?.pieces.insert(
            id,
            Work {
                creator: e.person,
                area,
                at: now,
                title: title.clone(),
                activity: e.activity.clone(),
                evidence,
                audience: Table::new(),
                composition,
                presented: Table::new(),
                presenter: None,
            },
        );
        tx.edit::<Works>()?.remember(e.person, id);
        tx.emit(
            Created {
                work: id,
                creator: e.person,
                title,
                audience: 0,
            },
            vec![e.person],
        )
    })?;
    Ok(())
}

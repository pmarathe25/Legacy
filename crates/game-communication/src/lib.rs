//! Personally learned sound–meaning associations and perceived nonverbal signals.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod signals;
mod sounds;
pub use signals::{Notice, Perception};
pub use sounds::{Coin, Convey, Reception, Translation, WordFor, decode, phonetic, render};
pub const PHONEMES: &[&str] = &[
    "a", "e", "i", "o", "u", "m", "n", "p", "t", "k", "s", "l", "r", "v", "h", "sh", "f", "b", "d",
    "g", "w", "y",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Association {
    pub sounds: Vec<u8>,
    pub strength: u16,
    pub source: PersonId,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Communication {
    pub enabled: bool,
    pub lexicons: Table<PersonId, Table<String, Vec<Association>>>,
    pub perceptions: Table<PersonId, Table<PersonId, Perception>>,
}
impl Resource for Communication {
    const KEY: &'static str = "communication.experience";
    fn validate(&self) -> Result<()> {
        for row in self.lexicons.values() {
            validate_words(row)?;
        }
        Ok(())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (_, row) in self.lexicons.changed(&old.lexicons) {
            validate_words(row)?;
        }
        Ok(())
    }
}
fn validate_words(row: &Table<String, Vec<Association>>) -> Result<()> {
    require(
        row.iter().all(|(key, variants)| {
            !key.is_empty()
                && key.len() <= 160
                && !variants.is_empty()
                && variants.len() <= 8
                && variants.iter().all(|w| {
                    w.strength > 0
                        && !w.sounds.is_empty()
                        && w.sounds.len() <= 16
                        && w.sounds.iter().all(|p| usize::from(*p) < PHONEMES.len())
                })
        }),
        "invalid lexical evidence",
    )
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "communication.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Practice {
    pub first: PersonId,
    pub second: PersonId,
}
impl Request for Practice {
    const KEY: &'static str = "communication.practice";
    type Response = ();
}
pub use game_vitality::ability;
fn local(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<AreaId> {
    game_physiology::capable(tx, a)?;
    game_physiology::capable(tx, b)?;
    let area = game_transport::area(tx.get()?, a)?;
    require(
        game_transport::area(tx.get()?, b)? == area,
        "communication requires contact",
    )?;
    Ok(area)
}
pub fn active(tx: &Transaction<'_>) -> bool {
    tx.get::<Communication>().is_ok_and(|c| c.enabled)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("communication", 1)?;
    b.resource::<Communication>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Communication>()?.enabled = true;
        Ok(())
    })?;
    sounds::register(b)?;
    signals::register(b)?;
    b.request::<Practice>(|tx, r| {
        if !active(tx) {
            return Ok(());
        }
        let area = local(tx, r.first, r.second)?;
        for (speaker, listener) in [(r.first, r.second), (r.second, r.first)] {
            let mut concepts = vec![
                format!("person:{}", speaker.0),
                format!("person:{}", listener.0),
            ];
            if let Some(row) = tx.get::<Communication>()?.lexicons.get(&speaker) {
                concepts.extend(row.keys().take(16).cloned());
            }
            concepts.push(format!("place:{}", area.0));
            if let Ok(cargo) = tx.get::<game_cargo::Cargo>() {
                concepts.extend(
                    cargo
                        .0
                        .values()
                        .filter(|i| i.custody == game_cargo::Custody::Carried(speaker))
                        .flat_map(|i| i.goods.iter())
                        .filter(|(_, q)| **q > 0)
                        .take(8)
                        .map(|(g, _)| format!("good:{}", g.0)),
                );
            }
            let salt = tx.now().0 / MINUTE.0;
            let concept = concepts
                [(random(tx.seed(), speaker.0, salt) % concepts.len() as u64) as usize]
                .clone();
            let _ = tx.attempt(Convey {
                speaker,
                listener,
                concept,
                demonstrate: true,
                explanation: vec![],
            });
        }
        Ok(())
    })
}

pub fn understood(
    tx: &mut Transaction<'_>,
    speaker: PersonId,
    listener: PersonId,
    concept: &str,
    explain: bool,
) -> Result<bool> {
    let explanation = if explain {
        tx.get::<Communication>()?
            .lexicons
            .get(&speaker)
            .into_iter()
            .flat_map(|r| r.keys())
            .take(2)
            .cloned()
            .collect()
    } else {
        vec![]
    };
    Ok(tx
        .call(Convey {
            speaker,
            listener,
            concept: concept.into(),
            demonstrate: false,
            explanation,
        })?
        .interpretation
        .as_deref()
        == Some(concept))
}

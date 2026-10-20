//! Personally learned sound–meaning associations and perceived nonverbal signals.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
use serde::{Deserialize, Serialize};

mod decoding;
pub use decoding::shared_meaning;
mod sounds;
pub use game_lexicon::Association;
pub use game_phonology::PHONEMES;
pub use game_signals::{Notice, Perception, Signalled, estimated_hunger};
pub use sounds::{
    Coin, Convey, Reception, Translation, WordFor, decode, illustrate, phonetic, render,
};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Communication {
    pub enabled: bool,
    pub lexicons: Table<PersonId, Records<String, Vec<Association>>>,
}
impl Resource for Communication {
    const KEY: &'static str = "communication.experience";
    fn validate(&self) -> Result<()> {
        for row in self.lexicons.values() {
            validate_words(row.iter())?;
        }
        Ok(())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (_, row, previous) in self.lexicons.changed_pairs(&old.lexicons) {
            if let Some(previous) = previous {
                validate_words(row.changed(previous))?;
            } else {
                validate_words(row.iter())?;
            }
        }
        Ok(())
    }
}
fn validate_words<'a>(
    mut words: impl Iterator<Item = (&'a String, &'a Vec<Association>)>,
) -> Result<()> {
    require(
        words.all(|(key, variants)| {
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
    game_signals::plugin(b)?;
    b.plugin("communication", 2)?;
    b.resource::<Communication>()?;
    b.request::<Enable>(|tx, _| {
        tx.call(game_signals::Enable)?;
        tx.edit::<Communication>()?.enabled = true;
        Ok(())
    })?;
    sounds::register(b)?;
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

/// Script and other instruction needs an understood language or an available visual model.
pub fn instruction(
    tx: &mut Transaction<'_>,
    teacher: PersonId,
    student: PersonId,
    concept: &str,
    model: bool,
) -> Result<bool> {
    local(tx, teacher, student)?;
    if ability(tx, student, "interpretation") < 80 {
        return Ok(false);
    }
    if model && ability(tx, teacher, "gesturing") >= 80 {
        return Ok(true);
    }
    if ability(tx, teacher, "vocalization") < 80
        || ability(tx, student, "hearing") < 80
        || !game_vitality::can_converse(tx, teacher)
    {
        return Ok(false);
    }
    Ok(!active(tx) || understood(tx, teacher, student, concept, true)?)
}

/// Decode available meanings without granting a creator's private gloss.
pub fn decode_many(
    c: &Communication,
    reader: PersonId,
    sounds: &[Vec<u8>],
    able: bool,
) -> Vec<Translation> {
    sounds
        .iter()
        .map(|w| {
            if able {
                decode(c, reader, w)
            } else {
                Translation {
                    surface: render(w),
                    meaning: None,
                    confidence: 0,
                }
            }
        })
        .collect()
}

//! Bounded personal episodes and selection of personally known creative subject matter.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recollections(pub Table<PersonId, Vec<String>>);
impl Resource for Recollections {
    const KEY: &'static str = "recollection.episodes";
    fn validate(&self) -> Result<()> {
        require(
            self.0
                .values()
                .all(|r| r.len() <= 32 && r.iter().all(|s| !s.is_empty() && s.len() <= 160)),
            "invalid personal episodes",
        )
    }
}
fn remember(
    tx: &mut Transaction<'_>,
    observer: PersonId,
    relation: &str,
    actor: PersonId,
    patient: PersonId,
) -> Result<()> {
    if game_physiology::capable(tx, observer).is_err()
        || game_vitality::ability(tx, observer, "intelligence") < 150
    {
        return Ok(());
    }
    let tag = format!("{relation}:{}:{}", actor.0, patient.0);
    if tx
        .get::<Recollections>()?
        .0
        .get(&observer)
        .is_some_and(|r| r.contains(&tag))
    {
        return Ok(());
    }
    let replacement = random(tx.seed(), observer.0, tx.now().0) as usize % 32;
    let row = tx.edit::<Recollections>()?.0.entry(observer).or_default();
    let slot = if row.len() < 32 {
        row.push(tag);
        row.len() - 1
    } else {
        // Retain a bounded personal sample rather than a world event archive.
        let slot = replacement;
        row[slot] = tag;
        slot
    };
    let mut scene = BTreeMap::from([
        ("@predicate".into(), format!("relation:{relation}")),
        ("actor".into(), format!("person:{}", actor.0)),
        ("patient".into(), format!("person:{}", patient.0)),
    ]);
    if let Ok(area) = game_transport::area(tx.get()?, observer) {
        scene.insert("place".into(), format!("place:{}", area.0));
    }
    if tx.registered::<game_syntax::Witness>() {
        tx.call(game_syntax::Witness {
            observer,
            scene: scene.clone(),
        })?;
    }
    tx.call(game_knowledge::Conclude {
        person: observer,
        key: format!("memory:episode:{slot}"),
        value: serde_json::to_value(scene).unwrap(),
        confidence: 80,
    })
}
/// Subject selection is personal and reproducible; knowledge does not imply objective truth.
pub fn inspiration(tx: &Transaction<'_>, person: PersonId) -> Vec<(String, game_knowledge::Fact)> {
    let mut evidence: Vec<_> = tx
        .get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&person))
        .into_iter()
        .flat_map(|f| f.iter())
        .filter(|(k, _)| {
            k.split_once(':').is_some_and(|(kind, _)| {
                matches!(
                    kind,
                    "theory" | "mechanics" | "measured" | "memory" | "roads" | "patch"
                )
            })
        })
        .map(|(k, f)| (k.to_string(), f.to_owned()))
        .collect();
    evidence.sort_by_key(|(key, f)| {
        let personal = key.starts_with("memory:") || key.starts_with("theory:");
        let salt = key.bytes().fold(person.0, |s, b| {
            s.wrapping_mul(131).wrapping_add(u64::from(b))
        });
        (
            !personal,
            std::cmp::Reverse(f.observed.0 / (365 * DAY.0)),
            random(tx.seed(), salt, 0),
        )
    });
    evidence.truncate(8);
    evidence
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("recollection", 1)?;
    b.resource::<Recollections>()?;
    b.subscribe::<game_physiology::Fed>("recollection.care", |tx, e| {
        if e.caregiver != e.patient {
            for observer in [e.caregiver, e.patient] {
                remember(tx, observer, "fed", e.caregiver, e.patient)?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_lifecycle::Born>("recollection.birth", |tx, e| {
        let family = tx.get::<game_lifecycle::Population>()?.families[&e.person].clone();
        for observer in [family.mother, family.father] {
            if game_transport::area(tx.get()?, observer).ok() == Some(e.area) {
                remember(tx, observer, "bore", family.mother, e.person)?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_conflict::Struck>("recollection.conflict", |tx, e| {
        for observer in [e.actor, e.target] {
            remember(tx, observer, "struck", e.actor, e.target)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("recollection.loss", |tx, e| {
        let Ok(area) = game_transport::area(tx.get()?, e.person) else {
            return Ok(());
        };
        let observers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()
            .ok()
            .and_then(|s| s.residents.get(&area))
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .take(32)
            .collect();
        for observer in observers {
            if observer != e.person {
                remember(tx, observer, "died", e.person, e.person)?;
            }
        }
        Ok(())
    })?;
    Ok(())
}

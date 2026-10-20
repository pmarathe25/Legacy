//! Relational speech learns from personal observations and actual conversations.
use game_core::*;
pub use game_grammar::{Grammar, Scene, Utterance};
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heard {
    pub speaker: PersonId,
    pub at: SimTime,
    pub utterance: Utterance,
    pub interpretation: Option<Scene>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Languages {
    pub people: Table<PersonId, Grammar>,
    pub observed: Table<PersonId, Vec<Scene>>,
    pub heard: Table<PersonId, Vec<Heard>>,
}
impl Resource for Languages {
    const KEY: &'static str = "syntax.languages";
    fn validate(&self) -> Result<()> {
        require(
            self.people
                .values()
                .all(|g| g.valid(game_communication::PHONEMES.len()))
                && self
                    .observed
                    .values()
                    .all(|s| s.len() <= 16 && s.iter().all(game_grammar::valid_scene))
                && self.heard.values().all(|r| {
                    r.len() <= 8
                        && r.iter().all(|h| {
                            h.utterance.words.len() <= 48
                                && h.utterance.words.iter().all(|w| {
                                    !w.is_empty()
                                        && w.len() <= 16
                                        && w.iter().all(|s| {
                                            usize::from(*s) < game_communication::PHONEMES.len()
                                        })
                                })
                        })
                }),
            "invalid relational language",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (_, g) in self.people.changed(&old.people) {
            require(
                g.valid(game_communication::PHONEMES.len()),
                "invalid grammar",
            )?;
        }
        for (_, s) in self.observed.changed(&old.observed) {
            require(
                s.len() <= 16 && s.iter().all(game_grammar::valid_scene),
                "invalid observations",
            )?;
        }
        for (_, h) in self.heard.changed(&old.heard) {
            require(
                h.len() <= 8
                    && h.iter().all(|h| {
                        h.utterance.words.len() <= 48
                            && h.utterance.words.iter().all(|s| {
                                !s.is_empty()
                                    && s.len() <= 16
                                    && s.iter().all(|p| {
                                        usize::from(*p) < game_communication::PHONEMES.len()
                                    })
                            })
                    }),
                "invalid speech memory",
            )?;
        }
        Ok(())
    }
}
/// The originating feature validates the perception, as with a personal knowledge conclusion.
/// This records only that observer's scene; it teaches no listener and supplies no grammar.
#[derive(Clone, Serialize, Deserialize)]
pub struct Witness {
    pub observer: PersonId,
    pub scene: Scene,
}
impl Request for Witness {
    const KEY: &'static str = "syntax.witness";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Describe {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub scene: Scene,
    pub demonstrate: bool,
}
impl Request for Describe {
    const KEY: &'static str = "syntax.describe";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Read {
    pub reader: PersonId,
}
impl Request for Read {
    const KEY: &'static str = "syntax.read";
    type Response = Vec<Reading>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    pub speaker: PersonId,
    pub surface: String,
    pub interpretation: Option<Scene>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Learned {
    pub person: PersonId,
    pub source: PersonId,
    pub constructions: usize,
    pub marked: bool,
    pub embedded: bool,
    pub sample: Utterance,
    pub meaning: Scene,
}
impl Event for Learned {
    const KEY: &'static str = "syntax.learned";
}
fn capacity(tx: &Transaction<'_>, p: PersonId) -> u64 {
    game_vitality::ability(tx, p, "intelligence")
        .min(game_vitality::ability(tx, p, "interpretation").saturating_mul(4))
        .min(
            game_vitality::ability(tx, p, "attention")
                .saturating_mul(2)
                .saturating_add(game_vitality::ability(tx, p, "learning") / 2),
        )
}
fn at(tx: &Transaction<'_>, person: PersonId) -> Option<Scene> {
    let area = game_transport::area(tx.get().ok()?, person).ok()?;
    Some(BTreeMap::from([
        ("@predicate".into(), "relation:at".into()),
        ("actor".into(), format!("person:{}", person.0)),
        ("place".into(), format!("place:{}", area.0)),
    ]))
}
fn observe(tx: &mut Transaction<'_>, p: PersonId, scene: Scene) -> Result<()> {
    if tx
        .get::<Languages>()?
        .observed
        .get(&p)
        .is_some_and(|s| s.contains(&scene))
    {
        return Ok(());
    }
    let row = tx.edit::<Languages>()?.observed.entry(p).or_default();
    if !row.contains(&scene) {
        if row.len() == 16 {
            row.remove(0);
        }
        row.push(scene);
    }
    Ok(())
}
fn grounded(tx: &Transaction<'_>, p: PersonId, scene: &Scene) -> bool {
    if tx
        .get::<Languages>()
        .ok()
        .and_then(|s| s.observed.get(&p))
        .is_some_and(|s| s.contains(scene))
    {
        return true;
    }
    let who = scene
        .get("actor")
        .and_then(|s| s.strip_prefix("person:"))
        .and_then(|s| s.parse().ok())
        .map(PersonId);
    if scene.contains_key("actor.@predicate") {
        let base: Scene = scene
            .iter()
            .filter(|(k, _)| !k.starts_with("actor."))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut relative: Scene = scene
            .iter()
            .filter_map(|(k, v)| k.strip_prefix("actor.").map(|r| (r.into(), v.clone())))
            .collect();
        if let Some(actor) = scene.get("actor") {
            relative.insert("actor".into(), actor.clone());
        }
        return grounded(tx, p, &base) && grounded(tx, p, &relative);
    }
    if scene.len() != 3 {
        return false;
    }
    let Some(who) = who.filter(|who| game_transport::together(tx.get().unwrap(), p, *who)) else {
        return false;
    };
    match scene.get("@predicate").map(String::as_str) {
        Some("relation:at") => at(tx, who).as_ref() == Some(scene),
        Some("relation:carries") => {
            let good = scene
                .get("thing")
                .and_then(|s| s.strip_prefix("good:"))
                .and_then(|s| s.parse::<u64>().ok())
                .map(GoodId);
            tx.get::<game_cargo::Cargo>()
                .is_ok_and(|c| good.is_some_and(|g| c.carried(who).any(|i| c.0[&i].amount(g) > 0)))
        }
        _ => false,
    }
}
fn decode(tx: &Transaction<'_>, p: PersonId, utterance: &Utterance) -> Option<Scene> {
    let grammar = tx.get::<Languages>().ok()?.people.get(&p)?;
    grammar.interpret(utterance, |w| {
        game_communication::decode(tx.get().unwrap(), p, w).meaning
    })
}
fn describe(tx: &mut Transaction<'_>, r: Describe) -> Result<bool> {
    require(r.speaker != r.listener, "speech needs another participant")?;
    require(
        game_grammar::valid_scene(&r.scene),
        "invalid relational message",
    )?;
    for p in [r.speaker, r.listener] {
        game_physiology::capable(tx, p)?;
    }
    require(
        game_transport::together(tx.get()?, r.speaker, r.listener),
        "speech requires contact",
    )?;
    if !game_communication::active(tx)
        || !game_vitality::can_converse(tx, r.speaker)
        || game_vitality::ability(tx, r.speaker, "vocalization") < 80
        || game_vitality::ability(tx, r.listener, "hearing") < 80
    {
        return Ok(false);
    }
    require(
        grounded(tx, r.speaker, &r.scene),
        "speaker has not observed this scene",
    )?;
    let shared_scene = r.demonstrate
        && grounded(tx, r.listener, &r.scene)
        && game_vitality::ability(tx, r.speaker, "gesturing") >= 80;
    let mut words = BTreeMap::new();
    let mut concrete: Vec<_> = r
        .scene
        .values()
        .filter(|s| s.starts_with("person:") || s.starts_with("place:"))
        .take(2)
        .cloned()
        .collect();
    if concrete.len() < 2
        && let Some(place) = at(tx, r.speaker).and_then(|s| s.get("place").cloned())
    {
        concrete.push(place);
    }
    for concept in r.scene.values() {
        let sounds = tx.call(game_communication::WordFor {
            person: r.speaker,
            concept: concept.clone(),
        })?;
        if shared_scene {
            // Labels need their own physical/shared grounding; never inject an author's gloss.
            let _ = tx.call(game_communication::Convey {
                speaker: r.speaker,
                listener: r.listener,
                concept: concept.clone(),
                demonstrate: true,
                explanation: concrete.clone(),
            })?;
        }
        words.insert(concept.clone(), sounds);
    }
    let grammar = tx
        .get::<Languages>()?
        .people
        .get(&r.speaker)
        .cloned()
        .unwrap_or_default();
    let template = grammar
        .rules
        .iter()
        .filter(|rule| {
            rule.predicate == r.scene["@predicate"]
                && rule.order.len() == r.scene.len()
                && rule.order.iter().all(|s| r.scene.contains_key(s))
                && rule.confirmations >= 4
                && rule.contrasts.len() >= 2
        })
        .max_by_key(|rule| rule.confirmations);
    let order = template
        .map(|rule| rule.order.clone())
        .unwrap_or_else(|| Grammar::role_order(&r.scene, random(tx.seed(), r.speaker.0, 811)));
    let mut markers = template
        .map(|rule| rule.markers.clone())
        .unwrap_or_default();
    if grammar.can_mark_roles(capacity(tx, r.speaker)) {
        for role in r.scene.keys().filter(|r| r.as_str() != "@predicate") {
            markers.insert(
                role.clone(),
                tx.call(game_communication::WordFor {
                    person: r.speaker,
                    concept: format!("grammar-role:{role}"),
                })?,
            );
        }
    }
    let Some(utterance) = grammar.express(&r.scene, &order, &markers, |c| words[c].clone()) else {
        return Ok(false);
    };
    let before = tx
        .get::<Languages>()?
        .people
        .get(&r.listener)
        .map_or(0, Grammar::productive_rules);
    let mut listener = tx
        .get::<Languages>()?
        .people
        .get(&r.listener)
        .cloned()
        .unwrap_or_default();
    let known = listener.interpret(&utterance, |w| {
        game_communication::decode(tx.get().unwrap(), r.listener, w).meaning
    });
    let learned = shared_scene
        && listener.observe(&utterance, &r.scene, capacity(tx, r.listener), |w| {
            game_communication::decode(tx.get().unwrap(), r.listener, w).meaning
        });
    // A successfully inferred, independently observed context can correct an old parse.
    let interpretation = if learned {
        Some(r.scene.clone())
    } else {
        known
    };
    let understood = interpretation.as_ref() == Some(&r.scene);
    let count = listener.productive_rules();
    tx.edit::<Languages>()?.people.insert(r.listener, listener);
    let speaker = tx.edit::<Languages>()?.people.entry(r.speaker).or_default();
    if understood {
        speaker.confirm(&r.scene, order, markers.clone());
    } else {
        speaker.failed();
    }
    let milestone = (count > before).then(|| utterance.clone());
    let now = tx.now();
    let heard = tx.edit::<Languages>()?.heard.entry(r.listener).or_default();
    if heard.len() == 8 {
        heard.remove(0);
    }
    heard.push(Heard {
        speaker: r.speaker,
        at: now,
        utterance,
        interpretation,
    });
    if count > before {
        tx.emit(
            Learned {
                person: r.listener,
                source: r.speaker,
                constructions: count,
                marked: !markers.is_empty(),
                embedded: r.scene.keys().any(|k| k.contains('.')),
                sample: milestone.unwrap(),
                meaning: r.scene.clone(),
            },
            vec![r.speaker, r.listener],
        )?;
    }
    Ok(understood)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("syntax", 1)?;
    b.resource::<Languages>()?;
    b.request::<Witness>(|tx, r| {
        game_physiology::capable(tx, r.observer)?;
        require(
            game_grammar::valid_scene(&r.scene),
            "invalid witnessed relation",
        )?;
        if capacity(tx, r.observer) < 300 {
            return Ok(false);
        }
        observe(tx, r.observer, r.scene)?;
        Ok(true)
    })?;
    b.request::<Describe>(describe)?;
    b.request::<Read>(|tx, r| {
        Ok(tx
            .get::<Languages>()?
            .heard
            .get(&r.reader)
            .into_iter()
            .flatten()
            .map(|h| Reading {
                speaker: h.speaker,
                surface: h
                    .utterance
                    .words
                    .iter()
                    .map(|w| game_communication::render(w))
                    .collect::<Vec<_>>()
                    .join(" "),
                interpretation: if game_vitality::can_converse(tx, r.reader) {
                    h.interpretation
                        .clone()
                        .or_else(|| decode(tx, r.reader, &h.utterance))
                } else {
                    None
                },
            })
            .collect())
    })?;
    b.subscribe::<game_physiology::Fed>("syntax.witnessed-feeding", |tx, e| {
        let mut scene = Scene::from([
            ("@predicate".into(), "relation:fed".into()),
            ("actor".into(), format!("person:{}", e.caregiver.0)),
            ("patient".into(), format!("person:{}", e.patient.0)),
            ("food".into(), format!("good:{}", e.good.0)),
        ]);
        if e.caregiver == e.patient {
            scene.remove("patient");
            scene.insert("@predicate".into(), "relation:ate".into());
        }
        for p in [e.caregiver, e.patient] {
            if game_communication::active(tx) && capacity(tx, p) >= 300 {
                observe(tx, p, scene.clone())?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_social_life::Encounter>("zz.syntax.conversation", |tx, e| {
        for (speaker, listener) in [(e.first, e.second), (e.second, e.first)] {
            let Some(place) = at(tx, listener) else {
                continue;
            };
            observe(tx, speaker, place.clone())?;
            observe(tx, listener, place.clone())?;
            let grammar = tx
                .get::<Languages>()?
                .people
                .get(&speaker)
                .cloned()
                .unwrap_or_default();
            let held = tx
                .get::<game_cargo::Cargo>()?
                .carried(speaker)
                .find_map(|i| {
                    tx.get::<game_cargo::Cargo>().ok()?.0[&i]
                        .goods
                        .iter()
                        .find(|(_, n)| **n > 0)
                        .map(|(g, _)| *g)
                });
            let mut scene = if grammar.productive_rules() > 0
                && let Some(good) = held
            {
                Scene::from([
                    ("@predicate".into(), "relation:carries".into()),
                    ("actor".into(), format!("person:{}", speaker.0)),
                    ("thing".into(), format!("good:{}", good.0)),
                ])
            } else {
                place.clone()
            };
            if grammar.can_embed(capacity(tx, speaker))
                && scene.get("@predicate").map(String::as_str) == Some("relation:carries")
                && let Some(location) = at(tx, speaker).and_then(|s| s.get("place").cloned())
            {
                scene.insert("actor.@predicate".into(), "relation:at".into());
                scene.insert("actor.place".into(), location);
            }
            let _ = tx.attempt(Describe {
                speaker,
                listener,
                scene,
                demonstrate: true,
            });
        }
        Ok(())
    })?;
    Ok(())
}

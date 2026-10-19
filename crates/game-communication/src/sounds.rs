use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Coin {
    pub person: PersonId,
    pub salt: u64,
}
impl Request for Coin {
    const KEY: &'static str = "communication.coin";
    type Response = String;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Convey {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub concept: String,
    pub demonstrate: bool,
    pub explanation: Vec<String>,
}
impl Request for Convey {
    const KEY: &'static str = "communication.convey";
    type Response = Reception;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WordFor {
    pub person: PersonId,
    pub concept: String,
}
impl Request for WordFor {
    const KEY: &'static str = "communication.word";
    type Response = Vec<u8>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Learned {
    pub person: PersonId,
    pub source: PersonId,
    pub concept: String,
    pub sounds: Vec<u8>,
}
impl Event for Learned {
    const KEY: &'static str = "communication.learned";
}
/// Receiver interpretation for effect owners; it is not a speaker's knowledge oracle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reception {
    pub sounds: Vec<u8>,
    pub interpretation: Option<String>,
    pub demonstrated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub surface: String,
    pub meaning: Option<String>,
    pub confidence: u16,
}
pub fn render(sounds: &[u8]) -> String {
    sounds
        .iter()
        .filter_map(|p| PHONEMES.get(usize::from(*p)))
        .copied()
        .collect()
}
pub fn phonetic(seed: u64, salt: u64) -> String {
    render(&invent(seed, PersonId(salt), salt, &[]))
}
fn invent(seed: u64, person: PersonId, salt: u64, familiar: &[u8]) -> Vec<u8> {
    let vowels = [0, 1, 2, 3, 4];
    let consonants: Vec<_> = (5..PHONEMES.len() as u8)
        .filter(|n| familiar.is_empty() || familiar.contains(n))
        .collect();
    let consonants = if consonants.is_empty() {
        vec![5, 6]
    } else {
        consonants
    };
    (0..2 + random(seed, person.0, salt) % 2)
        .flat_map(|i| {
            let draw = random(seed, person.0.wrapping_add(i), salt.wrapping_add(79));
            [
                consonants[(draw % consonants.len() as u64) as usize],
                vowels[((draw / 31) % 5) as usize],
            ]
        })
        .collect()
}
pub fn decode(c: &Communication, reader: PersonId, sounds: &[u8]) -> Translation {
    let best = c
        .lexicons
        .get(&reader)
        .into_iter()
        .flat_map(|r| r.iter())
        .flat_map(|(key, variants)| variants.iter().map(move |w| (key, w)))
        .filter(|(_, w)| w.sounds == sounds)
        .max_by_key(|(key, w)| (w.strength, std::cmp::Reverse(*key)));
    Translation {
        surface: render(sounds),
        meaning: best.map(|(key, _)| key.clone()),
        confidence: best.map_or(0, |(_, w)| w.strength),
    }
}
fn associate(
    tx: &mut Transaction<'_>,
    p: PersonId,
    source: PersonId,
    key: &str,
    sounds: &[u8],
    amount: u16,
) -> Result<()> {
    let at = tx.now();
    let row = tx.edit::<Communication>()?.lexicons.entry(p).or_default();
    let variants = row.entry(key.into()).or_default();
    if let Some(word) = variants.iter_mut().find(|w| w.sounds == sounds) {
        word.strength = word.strength.saturating_add(amount);
        word.at = at;
    } else {
        if variants.len() == 8 {
            let index = variants
                .iter()
                .enumerate()
                .min_by_key(|(_, w)| (w.strength, w.at))
                .unwrap()
                .0;
            variants.remove(index);
        }
        variants.push(Association {
            sounds: sounds.to_vec(),
            strength: amount,
            source,
            at,
        });
        tx.emit(
            Learned {
                person: p,
                source,
                concept: key.into(),
                sounds: sounds.to_vec(),
            },
            vec![p],
        )?;
    }
    Ok(())
}
fn visible(tx: &Transaction<'_>, p: PersonId, area: AreaId, concept: &str) -> bool {
    if let Some(id) = concept
        .strip_prefix("person:")
        .and_then(|s| s.parse::<u64>().ok())
    {
        return game_transport::area(tx.get().unwrap(), PersonId(id)).ok() == Some(area);
    }
    if concept == format!("place:{}", area.0) {
        return true;
    }
    if let Some(id) = concept
        .strip_prefix("good:")
        .and_then(|s| s.parse::<u64>().ok())
    {
        return tx.get::<game_cargo::Cargo>().is_ok_and(|c| {
            c.0.values().any(|i| {
                (matches!(i.custody, game_cargo::Custody::Area(a) if a == area)
                    || matches!(i.custody, game_cargo::Custody::Carried(person) if person == p))
                    && i.amount(GoodId(id)) > 0
            })
        });
    }
    false
}
fn familiar(c: &Communication, person: PersonId) -> Vec<u8> {
    c.lexicons
        .get(&person)
        .into_iter()
        .flat_map(|r| r.values())
        .flatten()
        .take(32)
        .flat_map(|w| w.sounds.iter().copied())
        .collect()
}
fn word(tx: &mut Transaction<'_>, person: PersonId, key: &str) -> Result<Vec<u8>> {
    require(!key.is_empty() && key.len() <= 160, "invalid concept")?;
    let row = tx.get::<Communication>()?.lexicons.get(&person);
    if let Some(best) = row.and_then(|r| r.get(key)).and_then(|v| {
        v.iter()
            .max_by_key(|w| (w.strength, std::cmp::Reverse(&w.sounds)))
    }) {
        return Ok(best.sounds.clone());
    }
    let familiar = familiar(tx.get()?, person);
    let salt = key
        .bytes()
        .fold(0u64, |n, b| n.wrapping_mul(31).wrapping_add(u64::from(b)));
    let sounds = invent(tx.seed(), person, salt, &familiar);
    associate(tx, person, person, key, &sounds, 1)?;
    Ok(sounds)
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<WordFor>(|tx, r| word(tx, r.person, &r.concept))?;
    b.request::<Coin>(|tx, r| {
        let familiar = familiar(tx.get()?, r.person);
        Ok(render(&invent(tx.seed(), r.person, r.salt, &familiar)))
    })?;
    b.request::<Convey>(|tx, r| {
        let area = local(tx, r.speaker, r.listener)?;
        require(
            active(tx)
                && !r.concept.is_empty()
                && r.concept.len() <= 160
                && r.explanation.len() <= 8,
            "invalid communication",
        )?;
        let can_speak = ability(tx, r.speaker, "vocalization") >= 80
            && game_vitality::can_converse(tx, r.speaker);
        let gesturing = ability(tx, r.speaker, "gesturing") >= 80
            && ability(tx, r.listener, "interpretation") >= 80;
        let demonstrated = r.demonstrate
            && gesturing
            && (visible(tx, r.speaker, area, &r.concept)
                || r.explanation.len() >= 2
                    && r.explanation
                        .iter()
                        .all(|k| visible(tx, r.speaker, area, k)));
        let hearing = ability(tx, r.listener, "hearing") >= 80;
        let comprehension = ability(tx, r.listener, "interpretation") >= 80;
        let explained = can_speak
            && hearing
            && comprehension
            && r.explanation.len() >= 2
            && r.explanation.iter().all(|k| {
                let c = tx.get::<Communication>().unwrap();
                c.lexicons
                    .get(&r.speaker)
                    .and_then(|row| row.get(k))
                    .is_some_and(|v| {
                        v.iter()
                            .any(|w| decode(c, r.listener, &w.sounds).meaning.as_ref() == Some(k))
                    })
            });
        if !can_speak {
            return Ok(Reception {
                sounds: vec![],
                interpretation: demonstrated.then_some(r.concept),
                demonstrated,
            });
        }
        let sounds = word(tx, r.speaker, &r.concept)?;
        associate(tx, r.speaker, r.speaker, &r.concept, &sounds, 1)?;
        let interpretation = if demonstrated || explained {
            Some(r.concept.clone())
        } else if hearing && comprehension {
            decode(tx.get()?, r.listener, &sounds).meaning
        } else {
            None
        };
        if hearing && comprehension && interpretation.as_ref() == Some(&r.concept) {
            let learning = game_vitality::ability(tx, r.listener, "learning");
            associate(
                tx,
                r.listener,
                r.speaker,
                &r.concept,
                &sounds,
                (1 + learning / 1000).min(8) as u16,
            )?;
        }
        Ok(Reception {
            sounds,
            interpretation,
            demonstrated,
        })
    })
}

/// A visual explanation needs concrete local referents already distinguished by the teacher.
pub fn illustrate(
    tx: &mut Transaction<'_>,
    speaker: PersonId,
    listener: PersonId,
    concept: &str,
) -> Result<bool> {
    let area = local(tx, speaker, listener)?;
    let explanation = tx
        .get::<Communication>()?
        .lexicons
        .get(&speaker)
        .into_iter()
        .flat_map(|r| r.keys())
        .filter(|k| visible(tx, speaker, area, k))
        .take(2)
        .cloned()
        .collect();
    Ok(tx
        .call(Convey {
            speaker,
            listener,
            concept: concept.into(),
            demonstrate: true,
            explanation,
        })?
        .interpretation
        .as_deref()
        == Some(concept))
}

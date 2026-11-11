//! Bounded personal vocabularies learned from grounded, physical encounters.
//! Each participant belongs to at most one interaction in a batch, allowing
//! independent preparation and deterministic publication across workers.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Display sounds; numeric indices are the physical speech inventory.
    pub sounds: Vec<String>,
    pub word_length: u8,
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Word {
    /// An observed referent, not an inherited universal word.
    pub meaning: Meaning,
    pub sounds: Vec<u8>,
}

/// Referents are shared only when the host supplies a grounded observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Meaning {
    Place(u64),
    Object(u32),
    Need(u32),
}

impl Meaning {
    fn key(self) -> u64 {
        match self {
            Self::Place(id) => random(id ^ 1),
            Self::Object(id) => random(u64::from(id) ^ 2),
            Self::Need(id) => random(u64::from(id) ^ 3),
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Place(id) => id != 0,
            Self::Object(id) | Self::Need(id) => id != 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    #[serde(default = "enabled")]
    pub speak: bool,
    #[serde(default = "enabled")]
    pub hear: bool,
    #[serde(default = "enabled")]
    pub learn: bool,
    #[serde(default = "enabled")]
    pub coin: bool,
    /// A personal inclination to initiate a communication opportunity.
    #[serde(default = "default_inclination")]
    pub initiative: u16,
    /// A personal inclination to incorporate an observed form.
    #[serde(default = "default_inclination")]
    pub adoption: u16,
    #[serde(default)]
    pub modify: bool,
    /// Empty permits the whole physical palette; otherwise only these sounds.
    #[serde(default)]
    pub phonemes: Vec<u8>,
    #[serde(default)]
    pub words: Vec<Word>,
}

fn enabled() -> bool {
    true
}

fn default_inclination() -> u16 {
    500
}

impl Mind {
    fn fresh(person: u64) -> Self {
        Self {
            person,
            speak: true,
            hear: true,
            learn: true,
            coin: true,
            initiative: default_inclination(),
            adoption: default_inclination(),
            modify: false,
            phonemes: Vec::new(),
            words: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub speaker: u64,
    pub listener: u64,
    pub meaning: Meaning,
    pub sounds: Vec<u8>,
    pub heard: bool,
    /// The listener's prior interpretation, which can be wrong or ambiguous.
    pub interpreted: Option<Meaning>,
    pub learned: bool,
    /// The listener's chosen form, which may differ from the heard form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adopted_sounds: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Language {
    pub rules: Rules,
    #[serde(default)]
    pub minds: Vec<Mind>,
    #[serde(default)]
    pub events: Vec<Event>,
}

/// The host supplies an actual meeting and a referent both can observe.
#[derive(Clone, Copy, Debug)]
pub struct Meeting {
    pub a: u64,
    pub b: u64,
    pub meaning: Meaning,
}

fn random(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

impl Language {
    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        let r = &self.rules;
        if r.sounds.is_empty()
            || r.sounds.len() > 256
            || r.sounds.iter().any(|s| s.is_empty() || s.len() > 16)
            || r.word_length == 0
            || r.word_length > 16
            || r.capacity == 0
            || r.capacity > 1024
            || self.minds.windows(2).any(|w| w[0].person >= w[1].person)
            || self.minds.iter().any(|m| {
                !people.contains(&m.person)
                    || m.initiative > 1000
                    || m.adoption > 1000
                    || m.phonemes
                        .iter()
                        .any(|sound| usize::from(*sound) >= r.sounds.len())
                    || m.phonemes.windows(2).any(|sounds| sounds[0] >= sounds[1])
                    || m.words.len() > r.capacity
                    || m.words.windows(2).any(|w| w[0].meaning >= w[1].meaning)
                    || m.words.iter().any(|w| {
                        !w.meaning.valid()
                            || w.sounds.is_empty()
                            || w.sounds.len() > 16
                            || w.sounds.iter().any(|s| usize::from(*s) >= r.sounds.len())
                    })
            })
            || self.events.iter().any(|e| {
                e.tick > now
                    || !people.contains(&e.speaker)
                    || !people.contains(&e.listener)
                    || !e.meaning.valid()
                    || e.speaker == e.listener
                    || e.sounds.is_empty()
                    || e.sounds.iter().any(|s| usize::from(*s) >= r.sounds.len())
                    || e.adopted_sounds.as_ref().is_some_and(|sounds| {
                        sounds.is_empty()
                            || sounds.iter().any(|s| usize::from(*s) >= r.sounds.len())
                    })
                    || e.learned != e.adopted_sounds.is_some()
                    || (!e.heard && (e.interpreted.is_some() || e.learned))
            })
        {
            return Err("invalid personal language state");
        }
        Ok(())
    }

    /// Stage a complete batch from opening vocabularies; invalid batches are inert.
    pub fn prepare(
        &self,
        meetings: &[Meeting],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        if self.rules.sounds.is_empty()
            || self.rules.sounds.len() > 256
            || self.rules.word_length == 0
            || self.rules.word_length > 16
            || self.rules.capacity == 0
            || self.rules.capacity > 1024
            || self
                .minds
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self.minds.par_iter().any(|m| {
                m.person == 0
                    || m.initiative > 1000
                    || m.adoption > 1000
                    || m.phonemes.windows(2).any(|sounds| sounds[0] >= sounds[1])
                    || m.phonemes
                        .iter()
                        .any(|sound| usize::from(*sound) >= self.rules.sounds.len())
                    || m.words.len() > self.rules.capacity
                    || m.words.windows(2).any(|w| w[0].meaning >= w[1].meaning)
                    || m.words.iter().any(|w| {
                        !w.meaning.valid()
                            || w.sounds.is_empty()
                            || w.sounds.len() > 16
                            || w.sounds
                                .iter()
                                .any(|s| usize::from(*s) >= self.rules.sounds.len())
                    })
            })
        {
            return Err("invalid personal language state");
        }
        let mut participants: Vec<_> = meetings.par_iter().flat_map_iter(|m| [m.a, m.b]).collect();
        if participants.par_windows(2).any(|rows| rows[0] > rows[1]) {
            participants.par_sort_unstable();
        }
        if tick == 0
            || meetings
                .par_iter()
                .any(|m| m.a == 0 || m.b == 0 || m.a == m.b || !m.meaning.valid())
            || participants.par_windows(2).any(|w| w[0] == w[1])
        {
            return Err("invalid or overlapping language meetings");
        }
        let mut next = Self {
            rules: self.rules.clone(),
            minds: self.minds.par_iter().cloned().collect(),
            events: Vec::new(),
        };
        next.minds.par_extend(
            participants
                .into_par_iter()
                .filter(|p| self.minds.binary_search_by_key(p, |m| m.person).is_err())
                .map(Mind::fresh),
        );
        if next
            .minds
            .par_windows(2)
            .any(|rows| rows[0].person > rows[1].person)
        {
            next.minds.par_sort_unstable_by_key(|m| m.person);
        }
        let events: Vec<_> = meetings
            .par_iter()
            .filter_map(|meeting| {
                let candidate = |id| {
                    let mind =
                        &next.minds[next.minds.binary_search_by_key(&id, |m| m.person).unwrap()];
                    mind.speak
                        && random(seed ^ tick.rotate_left(11) ^ id ^ meeting.meaning.key()) % 1000
                            < u64::from(mind.initiative)
                        && (mind
                            .words
                            .binary_search_by_key(&meeting.meaning, |w| w.meaning)
                            .is_ok()
                            || (mind.coin && mind.words.len() < next.rules.capacity))
                };
                let (speaker, listener) = match (candidate(meeting.a), candidate(meeting.b)) {
                    (false, false) => return None,
                    (true, false) => (meeting.a, meeting.b),
                    (false, true) => (meeting.b, meeting.a),
                    (true, true)
                        if random(seed ^ tick ^ meeting.a ^ meeting.b.rotate_left(17)) & 1 != 0 =>
                    {
                        (meeting.b, meeting.a)
                    }
                    (true, true) => (meeting.a, meeting.b),
                };
                let source = &next.minds[next
                    .minds
                    .binary_search_by_key(&speaker, |m| m.person)
                    .unwrap()];
                let target = &next.minds[next
                    .minds
                    .binary_search_by_key(&listener, |m| m.person)
                    .unwrap()];
                let known = source
                    .words
                    .binary_search_by_key(&meeting.meaning, |w| w.meaning)
                    .ok()
                    .map(|i| source.words[i].sounds.clone());
                let sounds = known.clone().or_else(|| {
                    (source.coin && source.words.len() < next.rules.capacity).then(|| {
                        (0..next.rules.word_length)
                            .map(|i| {
                                let draw = random(
                                    seed ^ speaker
                                        ^ meeting.meaning.key().rotate_left(13)
                                        ^ u64::from(i),
                                );
                                if source.phonemes.is_empty() {
                                    (draw % next.rules.sounds.len() as u64) as u8
                                } else {
                                    source.phonemes[draw as usize % source.phonemes.len()]
                                }
                            })
                            .collect()
                    })
                })?;
                let mut interpretations = target
                    .words
                    .iter()
                    .filter(|w| w.sounds == sounds)
                    .map(|w| w.meaning);
                let first = interpretations.next();
                let interpreted = target
                    .hear
                    .then(|| first.filter(|_| interpretations.next().is_none()))
                    .flatten();
                let old = target
                    .words
                    .binary_search_by_key(&meeting.meaning, |w| w.meaning)
                    .ok();
                let mut adopted = sounds.clone();
                if !target.phonemes.is_empty() {
                    for sound in &mut adopted {
                        if target.phonemes.binary_search(sound).is_err() {
                            *sound = target.phonemes[usize::from(*sound) % target.phonemes.len()];
                        }
                    }
                }
                if target.modify {
                    let count = if target.phonemes.is_empty() {
                        next.rules.sounds.len()
                    } else {
                        target.phonemes.len()
                    };
                    if count > 1 {
                        let first = adopted[0];
                        let index = if target.phonemes.is_empty() {
                            usize::from(first)
                        } else {
                            target.phonemes.binary_search(&first).unwrap()
                        };
                        let changed =
                            (index + 1 + random(seed ^ tick ^ listener) as usize % (count - 1))
                                % count;
                        adopted[0] = if target.phonemes.is_empty() {
                            changed as u8
                        } else {
                            target.phonemes[changed]
                        };
                    }
                }
                let learned = target.hear
                    && target.learn
                    && random(seed ^ tick.rotate_left(23) ^ listener ^ speaker) % 1000
                        < u64::from(target.adoption)
                    && (old.is_some() || target.words.len() < next.rules.capacity)
                    && old.is_none_or(|i| target.words[i].sounds != adopted);
                Some((
                    Event {
                        tick,
                        speaker,
                        listener,
                        meaning: meeting.meaning,
                        sounds,
                        heard: target.hear,
                        interpreted,
                        learned,
                        adopted_sounds: learned.then(|| adopted.clone()),
                    },
                    known.is_none(),
                    adopted,
                ))
            })
            .collect();
        let mut additions: Vec<_> = events
            .par_iter()
            .flat_map_iter(|(e, coined, adopted)| {
                let mut rows = Vec::with_capacity(2);
                if *coined {
                    rows.push((
                        e.speaker,
                        Word {
                            meaning: e.meaning,
                            sounds: e.sounds.clone(),
                        },
                    ));
                }
                if e.learned {
                    rows.push((
                        e.listener,
                        Word {
                            meaning: e.meaning,
                            sounds: adopted.clone(),
                        },
                    ));
                }
                rows
            })
            .collect();
        let apply = |mind: &mut Mind, word: &Word| match mind
            .words
            .binary_search_by_key(&word.meaning, |row| row.meaning)
        {
            Ok(index) => mind.words[index] = word.clone(),
            Err(index) => mind.words.insert(index, word.clone()),
        };
        let dense = next
            .minds
            .first()
            .zip(next.minds.last())
            .and_then(|(first, last)| {
                let range = last.person.checked_sub(first.person)?.checked_add(1)?;
                let range = usize::try_from(range).ok()?;
                (range <= next.minds.len().saturating_mul(4) && range <= 10_000_000)
                    .then_some((first.person, range))
            });
        if let Some((base, range)) = dense {
            // Participants are disjoint, so every actor writes a different slot.
            let index: Vec<_> = (0..range)
                .into_par_iter()
                .map(|_| AtomicUsize::new(0))
                .collect();
            additions.par_iter().enumerate().for_each(|(i, row)| {
                index[(row.0 - base) as usize].store(i + 1, Ordering::Relaxed);
            });
            next.minds.par_iter_mut().for_each(|mind| {
                let slot = index[(mind.person - base) as usize].load(Ordering::Relaxed);
                if slot != 0 {
                    apply(mind, &additions[slot - 1].1);
                }
            });
        } else {
            if additions.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
                additions.par_sort_unstable_by_key(|r| r.0);
            }
            next.minds.par_iter_mut().for_each(|mind| {
                if let Ok(i) = additions.binary_search_by_key(&mind.person, |r| r.0) {
                    apply(mind, &additions[i].1);
                }
            });
        }
        next.events = events.into_par_iter().map(|(event, _, _)| event).collect();
        if next
            .events
            .par_windows(2)
            .any(|rows| (rows[0].speaker, rows[0].listener) > (rows[1].speaker, rows[1].listener))
        {
            next.events
                .par_sort_unstable_by_key(|e| (e.speaker, e.listener));
        }
        Ok(next)
    }
}

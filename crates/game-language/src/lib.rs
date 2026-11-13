//! Bounded personal vocabularies learned from grounded, physical encounters.
//! Each participant belongs to at most one interaction in a batch, allowing
//! independent preparation and deterministic publication across workers.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
mod names;
pub use names::{Namesake, NamingPolicy};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Display sounds; numeric indices are the physical speech inventory.
    pub sounds: Vec<String>,
    pub word_length: u8,
    pub capacity: usize,
}

impl Rules {
    /// Render a sound sequence for an external observer without defining a
    /// writing system or giving the translation to any simulated character.
    pub fn render(&self, sounds: &[u8]) -> Result<String, &'static str> {
        if sounds.is_empty() {
            return Err("empty sound sequence");
        }
        if self
            .sounds
            .iter()
            .any(|sound| sound.is_empty() || sound.contains('·'))
            || self.sounds.iter().collect::<HashSet<_>>().len() != self.sounds.len()
        {
            return Err("ambiguous sound inventory");
        }
        sounds
            .iter()
            .map(|id| {
                self.sounds
                    .get(usize::from(*id))
                    .map(String::as_str)
                    .ok_or("unknown phoneme")
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join("·"))
    }
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
    /// A personally identified person, grounded in an actual encounter.
    Person(u64),
    /// An observable physical signal form, not its private intention.
    Signal(u32),
    /// A proposition about one apparatus and a reported mechanical outcome.
    MechanicalResult {
        apparatus: u64,
        outcome: u8,
    },
}

impl Meaning {
    fn key(self) -> u64 {
        match self {
            Self::Place(id) => random(id ^ 1),
            Self::Object(id) => random(u64::from(id) ^ 2),
            Self::Need(id) => random(u64::from(id) ^ 3),
            Self::Person(id) => random(id ^ 4),
            Self::Signal(id) => random(u64::from(id) ^ 5),
            Self::MechanicalResult { apparatus, outcome } => {
                random(apparatus ^ u64::from(outcome).rotate_left(37) ^ 6)
            }
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Place(id) | Self::Person(id) => id != 0,
            Self::Object(id) | Self::Need(id) | Self::Signal(id) => id != 0,
            Self::MechanicalResult { apparatus, outcome } => apparatus != 0 && outcome < 4,
        }
    }

    fn known(self, people: &HashSet<u64>) -> bool {
        self.valid()
            && match self {
                Self::Person(id) => people.contains(&id),
                _ => true,
            }
    }
}

/// A person's inclination to discuss one referent when it is observable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicPreference {
    pub meaning: Meaning,
    pub enabled: bool,
    pub value: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicPolicy {
    pub idle: i64,
    #[serde(default)]
    pub act_on_tie: bool,
    pub preferences: Vec<TopicPreference>,
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
    /// None retains the earlier place-only behavior; an empty policy can refuse all topics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_policy: Option<TopicPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming: Option<NamingPolicy>,
    /// Personal value for discussing a mechanical result when it is grounded
    /// or deliberately offered; this does not supply a word or its meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanical_interest: Option<i64>,
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
            topic_policy: None,
            naming: None,
            mechanical_interest: None,
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

/// A person-selected assertion whose intended referent is not jointly grounded.
/// The caller must establish an actual contact and the person's intent to speak.
#[derive(Clone, Copy, Debug)]
pub struct UngroundedUtterance {
    pub speaker: u64,
    pub listener: u64,
    pub meaning: Meaning,
}

/// The shared place and things each participant could choose to demonstrate.
#[derive(Clone, Debug)]
pub struct GroundedMeeting {
    pub a: u64,
    pub b: u64,
    pub a_topics: Vec<Meaning>,
    pub b_topics: Vec<Meaning>,
}

fn random(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

impl Language {
    fn new_form<F: Fn(u64, u64) -> i32 + Sync>(
        &self,
        mind: &Mind,
        meaning: Meaning,
        seed: u64,
        tick: u64,
        affinity: &F,
    ) -> Option<Vec<u8>> {
        if !mind.coin
            || mind.words.len() >= self.rules.capacity
            || self.rules.sounds.is_empty()
            || self.rules.word_length == 0
        {
            return None;
        }
        let novel: Vec<_> = (0..self.rules.word_length)
            .map(|i| {
                let draw =
                    random(seed ^ mind.person ^ meaning.key().rotate_left(13) ^ u64::from(i));
                if mind.phonemes.is_empty() {
                    (draw % self.rules.sounds.len() as u64) as u8
                } else {
                    mind.phonemes[draw as usize % mind.phonemes.len()]
                }
            })
            .collect();
        match meaning {
            Meaning::Person(target) if mind.naming.is_some() => {
                names::choose(mind, target, novel, affinity, seed, tick)
            }
            _ => Some(novel),
        }
    }

    fn can_form<F: Fn(u64, u64) -> i32 + Sync>(
        &self,
        mind: &Mind,
        meaning: Meaning,
        seed: u64,
        tick: u64,
        affinity: &F,
    ) -> bool {
        if mind
            .words
            .binary_search_by_key(&meaning, |word| word.meaning)
            .is_ok()
        {
            return true;
        }
        if !mind.coin || mind.words.len() >= self.rules.capacity {
            return false;
        }
        if matches!(meaning, Meaning::Person(_)) && mind.naming.is_some() {
            self.new_form(mind, meaning, seed, tick, affinity).is_some()
        } else {
            true
        }
    }

    /// Set personal topic policies without creating or sharing vocabulary.
    pub fn configure_topics(&mut self, entries: &[(u64, TopicPolicy)]) -> Result<(), &'static str> {
        let mut entries = entries.to_vec();
        entries.sort_unstable_by_key(|row| row.0);
        if self
            .minds
            .windows(2)
            .any(|rows| rows[0].person >= rows[1].person)
            || entries.windows(2).any(|rows| rows[0].0 == rows[1].0)
            || entries.iter().any(|(person, policy)| {
                *person == 0
                    || policy
                        .preferences
                        .windows(2)
                        .any(|rows| rows[0].meaning >= rows[1].meaning)
                    || policy.preferences.iter().any(|row| !row.meaning.valid())
            })
        {
            return Err("invalid personal topic policies");
        }
        let mut minds: Vec<_> = self.minds.par_iter().cloned().collect();
        minds.par_extend(entries.par_iter().filter_map(|(person, _)| {
            self.minds
                .binary_search_by_key(person, |mind| mind.person)
                .is_err()
                .then(|| Mind::fresh(*person))
        }));
        if minds
            .par_windows(2)
            .any(|rows| rows[0].person > rows[1].person)
        {
            minds.par_sort_unstable_by_key(|mind| mind.person);
        }
        minds.par_iter_mut().for_each(|mind| {
            if let Ok(index) = entries.binary_search_by_key(&mind.person, |row| row.0) {
                mind.topic_policy = Some(entries[index].1.clone());
            }
        });
        self.minds = minds;
        Ok(())
    }

    /// Each participant proposes a personally chosen observable topic; one
    /// speaker is admitted per disjoint encounter before sound is produced.
    pub fn prepare_grounded(
        &self,
        meetings: &[GroundedMeeting],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        self.prepare_grounded_with_affinity(meetings, seed, tick, &|_, _| 0)
    }

    /// Personal naming may use only the speaker's own directed affinities.
    pub fn prepare_grounded_with_affinity<F: Fn(u64, u64) -> i32 + Sync>(
        &self,
        meetings: &[GroundedMeeting],
        seed: u64,
        tick: u64,
        affinity: &F,
    ) -> Result<Self, &'static str> {
        self.prepare_mixed_with_affinity(meetings, None, seed, tick, affinity)
    }

    /// Arbitrate grounded topics and personal assertions together.
    pub fn prepare_mixed_with_affinity<F: Fn(u64, u64) -> i32 + Sync>(
        &self,
        meetings: &[GroundedMeeting],
        claims: Option<&[(Option<Meaning>, Option<Meaning>)]>,
        seed: u64,
        tick: u64,
        affinity: &F,
    ) -> Result<Self, &'static str> {
        if claims.is_some_and(|rows| {
            rows.len() != meetings.len()
                || rows.par_iter().any(|(a, b)| {
                    [a, b].into_iter().flatten().any(|claim| {
                        !matches!(claim, Meaning::MechanicalResult { .. }) || !claim.valid()
                    })
                })
        }) {
            return Err("invalid ungrounded claim offers");
        }
        if meetings.par_iter().any(|meeting| {
            let valid = |topics: &[Meaning]| {
                !topics.is_empty()
                    && !topics.iter().any(|meaning| !meaning.valid())
                    && !topics.windows(2).any(|rows| rows[0] >= rows[1])
                    && topics
                        .iter()
                        .filter(|meaning| matches!(meaning, Meaning::Place(_)))
                        .count()
                        == 1
            };
            !valid(&meeting.a_topics)
                || !valid(&meeting.b_topics)
                || meeting
                    .a_topics
                    .iter()
                    .find(|meaning| matches!(meaning, Meaning::Place(_)))
                    != meeting
                        .b_topics
                        .iter()
                        .find(|meaning| matches!(meaning, Meaning::Place(_)))
        }) {
            return Err("invalid grounded meeting topics");
        }
        let selected: Vec<_> = meetings
            .par_iter()
            .enumerate()
            .map(|(position, meeting)| {
                let offered = claims.map_or((None, None), |rows| rows[position]);
                let offer = |person: u64, observed: &[Meaning], claim: Option<Meaning>| {
                    let fresh = Mind::fresh(person);
                    let mind = self
                        .minds
                        .binary_search_by_key(&person, |mind| mind.person)
                        .ok()
                        .map_or(&fresh, |index| &self.minds[index]);
                    let eligible = |meaning: Meaning| {
                        mind.speak
                            && random(seed ^ tick.rotate_left(11) ^ person ^ meaning.key()) % 1000
                                < u64::from(mind.initiative)
                            && self.can_form(mind, meaning, seed, tick, affinity)
                    };
                    match &mind.topic_policy {
                        None => observed
                            .iter()
                            .copied()
                            .find(|meaning| {
                                matches!(meaning, Meaning::Place(_)) && eligible(*meaning)
                            })
                            .map(|meaning| (meaning, true)),
                        Some(policy) => {
                            let explicit = policy
                                .preferences
                                .iter()
                                .filter(|topic| {
                                    topic.enabled
                                        && (topic.value > policy.idle
                                            || (topic.value == policy.idle && policy.act_on_tie))
                                        && (observed.binary_search(&topic.meaning).is_ok()
                                            || claim == Some(topic.meaning))
                                        && eligible(topic.meaning)
                                })
                                .map(|topic| (topic.meaning, topic.value));
                            let dynamic =
                                observed.iter().copied().chain(claim).filter_map(|meaning| {
                                    let value = mind.mechanical_interest?;
                                    (matches!(meaning, Meaning::MechanicalResult { .. })
                                        && policy
                                            .preferences
                                            .binary_search_by_key(&meaning, |topic| topic.meaning)
                                            .is_err()
                                        && (value > policy.idle
                                            || (value == policy.idle && policy.act_on_tie))
                                        && eligible(meaning))
                                    .then_some((meaning, value))
                                });
                            explicit
                                .chain(dynamic)
                                .max_by_key(|(meaning, value)| {
                                    (
                                        *value,
                                        random(
                                            seed ^ tick.rotate_left(19) ^ person ^ meaning.key(),
                                        ),
                                        *meaning,
                                    )
                                })
                                .map(|(meaning, _)| {
                                    (meaning, observed.binary_search(&meaning).is_ok())
                                })
                        }
                    }
                };
                let a = offer(meeting.a, &meeting.a_topics, offered.0);
                let b = offer(meeting.b, &meeting.b_topics, offered.1);
                let selected = match (a, b) {
                    (None, None) => None,
                    (Some((meaning, grounded)), None) => Some((meeting.a, meaning, grounded)),
                    (None, Some((meaning, grounded))) => Some((meeting.b, meaning, grounded)),
                    (Some(_a), Some(b))
                        if random(seed ^ tick ^ meeting.a ^ meeting.b.rotate_left(17)) & 1 != 0 =>
                    {
                        Some((meeting.b, b.0, b.1))
                    }
                    (Some(a), Some(_)) => Some((meeting.a, a.0, a.1)),
                };
                let place = meeting
                    .a_topics
                    .iter()
                    .copied()
                    .find(|meaning| matches!(meaning, Meaning::Place(_)))
                    .expect("validated place");
                (
                    Meeting {
                        a: meeting.a,
                        b: meeting.b,
                        meaning: selected.map_or(place, |row| row.1),
                    },
                    selected.map(|row| row.0),
                    selected.is_none_or(|row| row.2),
                )
            })
            .collect();
        let meetings: Vec<_> = selected.par_iter().map(|row| row.0).collect();
        let speakers: Vec<_> = selected.par_iter().map(|row| row.1).collect();
        let grounded: Vec<_> = selected.par_iter().map(|row| row.2).collect();
        self.prepare_internal(
            &meetings,
            Some(&speakers),
            seed,
            tick,
            affinity,
            Some(&grounded),
        )
    }

    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        let r = &self.rules;
        if r.sounds.is_empty()
            || r.sounds.len() > 256
            || r.sounds
                .iter()
                .any(|s| s.is_empty() || s.len() > 16 || s.contains('·'))
            || r.sounds.iter().collect::<HashSet<_>>().len() != r.sounds.len()
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
                    || m.naming.as_ref().is_some_and(|policy| {
                        !policy.valid()
                            || policy
                                .namesakes
                                .iter()
                                .any(|row| !people.contains(&row.person))
                    })
                    || m.topic_policy.as_ref().is_some_and(|policy| {
                        policy
                            .preferences
                            .windows(2)
                            .any(|w| w[0].meaning >= w[1].meaning)
                            || policy
                                .preferences
                                .iter()
                                .any(|topic| !topic.meaning.known(people))
                    })
                    || m.words.windows(2).any(|w| w[0].meaning >= w[1].meaning)
                    || m.words.iter().any(|w| {
                        !w.meaning.known(people)
                            || w.sounds.is_empty()
                            || w.sounds.len() > 16
                            || w.sounds.iter().any(|s| usize::from(*s) >= r.sounds.len())
                    })
            })
            || self.events.iter().any(|e| {
                e.tick > now
                    || !people.contains(&e.speaker)
                    || !people.contains(&e.listener)
                    || !e.meaning.known(people)
                    || e.interpreted.is_some_and(|meaning| !meaning.known(people))
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
        self.prepare_internal(meetings, None, seed, tick, &|_, _| 0, None)
    }

    /// Stage assertions from opening vocabularies without teaching a listener
    /// the speaker's private intended meaning. A listener may hear an unknown,
    /// ambiguous, or differently understood form. Speakers can still coin a
    /// personal form; the listener's mappings are never altered by this call.
    pub fn prepare_ungrounded(
        &self,
        utterances: &[UngroundedUtterance],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        let meetings: Vec<_> = utterances
            .par_iter()
            .map(|row| Meeting {
                a: row.speaker,
                b: row.listener,
                meaning: row.meaning,
            })
            .collect();
        let speakers: Vec<_> = utterances.par_iter().map(|row| Some(row.speaker)).collect();
        let grounded = vec![false; meetings.len()];
        self.prepare_internal(
            &meetings,
            Some(&speakers),
            seed,
            tick,
            &|_, _| 0,
            Some(&grounded),
        )
    }

    fn prepare_internal<F: Fn(u64, u64) -> i32 + Sync>(
        &self,
        meetings: &[Meeting],
        speakers: Option<&[Option<u64>]>,
        seed: u64,
        tick: u64,
        affinity: &F,
        grounded: Option<&[bool]>,
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
                    || m.naming.as_ref().is_some_and(|policy| !policy.valid())
                    || m.topic_policy.as_ref().is_some_and(|policy| {
                        policy
                            .preferences
                            .windows(2)
                            .any(|w| w[0].meaning >= w[1].meaning)
                            || policy
                                .preferences
                                .iter()
                                .any(|topic| !topic.meaning.valid())
                    })
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
            || grounded.is_some_and(|rows| rows.len() != meetings.len())
            || speakers.is_some_and(|rows| {
                rows.len() != meetings.len()
                    || rows.iter().zip(meetings).any(|(speaker, meeting)| {
                        speaker.is_some_and(|speaker| speaker != meeting.a && speaker != meeting.b)
                    })
            })
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
            .enumerate()
            .filter_map(|(meeting_index, meeting)| {
                let candidate = |id| {
                    let mind =
                        &next.minds[next.minds.binary_search_by_key(&id, |m| m.person).unwrap()];
                    mind.speak
                        && mind.topic_policy.as_ref().is_none_or(|policy| {
                            policy
                                .preferences
                                .binary_search_by_key(&meeting.meaning, |topic| topic.meaning)
                                .ok()
                                .map_or_else(
                                    || {
                                        matches!(meeting.meaning, Meaning::MechanicalResult { .. })
                                            && mind.mechanical_interest.is_some_and(|value| {
                                                value > policy.idle
                                                    || (value == policy.idle && policy.act_on_tie)
                                            })
                                    },
                                    |index| {
                                        let topic = policy.preferences[index];
                                        topic.enabled
                                            && (topic.value > policy.idle
                                                || (topic.value == policy.idle
                                                    && policy.act_on_tie))
                                    },
                                )
                        })
                        && random(seed ^ tick.rotate_left(11) ^ id ^ meeting.meaning.key()) % 1000
                            < u64::from(mind.initiative)
                        && next.can_form(mind, meeting.meaning, seed, tick, affinity)
                };
                let (speaker, listener) = if let Some(speakers) = speakers {
                    let speaker = speakers[meeting_index]?;
                    if !candidate(speaker) {
                        return None;
                    }
                    (
                        speaker,
                        if speaker == meeting.a {
                            meeting.b
                        } else {
                            meeting.a
                        },
                    )
                } else {
                    match (candidate(meeting.a), candidate(meeting.b)) {
                        (false, false) => return None,
                        (true, false) => (meeting.a, meeting.b),
                        (false, true) => (meeting.b, meeting.a),
                        (true, true)
                            if random(seed ^ tick ^ meeting.a ^ meeting.b.rotate_left(17)) & 1
                                != 0 =>
                        {
                            (meeting.b, meeting.a)
                        }
                        (true, true) => (meeting.a, meeting.b),
                    }
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
                let sounds = known
                    .clone()
                    .or_else(|| next.new_form(source, meeting.meaning, seed, tick, affinity))?;
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
                let learned = grounded.is_none_or(|rows| rows[meeting_index])
                    && target.hear
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

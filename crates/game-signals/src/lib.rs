//! Personal, nonverbal expressions and subjective interpretations.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub id: u32,
    pub name: String,
}

/// Names describe available forms and possible personal meanings, not mappings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub forms: Vec<Named>,
    pub meanings: Vec<Named>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expression {
    pub form: u32,
    /// This person's own intention; it is not sent to the observer.
    #[serde(default)]
    pub intended: Option<u32>,
    #[serde(default)]
    pub recipient: Option<u64>,
    pub enabled: bool,
    /// An optional personal condition, not an objective requirement.
    #[serde(default)]
    pub hunger_at_least: Option<u32>,
    pub value: i64,
    #[serde(default)]
    pub hunger_weight: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interpretation {
    pub form: u32,
    pub meaning: u32,
}

/// How this observer personally feels about an inferred meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    pub meaning: u32,
    #[serde(default)]
    pub trust: i32,
    #[serde(default)]
    pub affinity: i32,
    #[serde(default)]
    pub fear: i64,
    #[serde(default)]
    pub obligation: i64,
}

/// Receiver-owned request to adjust a directed relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeelingChange {
    pub observer: u64,
    pub target: u64,
    pub reaction: Reaction,
}

/// A personally valued possible gift in response to an interpreted meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GiftPreference {
    pub meaning: u32,
    pub good: u32,
    pub quantity: u64,
    pub value: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiftOpportunity {
    pub giver: u64,
    pub recipient: u64,
    pub good: u32,
    pub quantity: u64,
    pub value: i64,
}

/// A listener's own recollection, without access to the sender's intention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub tick: u64,
    pub actor: u64,
    pub form: u32,
    pub interpreted: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    pub idle: i64,
    #[serde(default)]
    pub act_on_tie: bool,
    /// Per-thousand personal likelihood of noticing a signal.
    pub attention: u16,
    #[serde(default)]
    pub expressions: Vec<Expression>,
    #[serde(default)]
    pub interpretations: Vec<Interpretation>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub gift_preferences: Vec<GiftPreference>,
    #[serde(default)]
    pub gift_idle: i64,
    #[serde(default)]
    pub gift_on_tie: bool,
    #[serde(default)]
    pub remember: bool,
    #[serde(default)]
    pub capacity: usize,
    #[serde(default)]
    pub observations: Vec<Observation>,
}

/// Only the actual form and the observer's interpretation are published.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub actor: u64,
    pub observer: u64,
    pub form: u32,
    pub noticed: bool,
    pub interpreted: Option<u32>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signals {
    pub catalog: Catalog,
    #[serde(default)]
    pub minds: Vec<Mind>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Clone for Signals {
    fn clone(&self) -> Self {
        Self {
            catalog: self.catalog.clone(),
            minds: self.minds.par_iter().cloned().collect(),
            events: self.events.par_iter().copied().collect(),
        }
    }
}

/// A real, disjoint encounter and each participant's own bodily state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    pub a: u64,
    pub b: u64,
    pub a_hunger: u32,
    pub b_hunger: u32,
}

/// A potential directed signal to a physically reachable person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Directed {
    pub actor: u64,
    pub observer: u64,
    pub hunger: u32,
}

#[derive(Clone, Copy)]
struct Candidate {
    actor: u64,
    observer: u64,
    hunger: u32,
    directed_only: bool,
}

fn random(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

impl Signals {
    fn mind(&self, person: u64) -> Option<&Mind> {
        self.minds
            .first()
            .and_then(|first| person.checked_sub(first.person))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&index| {
                self.minds
                    .get(index)
                    .is_some_and(|mind| mind.person == person)
            })
            .or_else(|| {
                self.minds
                    .binary_search_by_key(&person, |mind| mind.person)
                    .ok()
            })
            .map(|index| &self.minds[index])
    }

    fn valid_catalog(&self) -> bool {
        let valid = |rows: &[Named]| {
            !rows.is_empty()
                && !rows.windows(2).any(|rows| rows[0].id >= rows[1].id)
                && rows.iter().all(|row| row.id != 0 && !row.name.is_empty())
                && rows
                    .iter()
                    .map(|row| &row.name)
                    .collect::<HashSet<_>>()
                    .len()
                    == rows.len()
        };
        valid(&self.catalog.forms) && valid(&self.catalog.meanings)
    }

    fn valid_form(&self, id: u32) -> bool {
        self.catalog
            .forms
            .binary_search_by_key(&id, |row| row.id)
            .is_ok()
    }

    fn valid_meaning(&self, id: u32) -> bool {
        self.catalog
            .meanings
            .binary_search_by_key(&id, |row| row.id)
            .is_ok()
    }

    fn valid_state(&self, now: u64) -> bool {
        self.valid_catalog()
            && !self
                .minds
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            && !self.minds.par_iter().any(|mind| {
                mind.person == 0
                    || mind.attention > 1000
                    || mind.observations.len() > mind.capacity
                    || mind.expressions.windows(2).any(|rows| {
                        (rows[0].form, rows[0].recipient) >= (rows[1].form, rows[1].recipient)
                    })
                    || mind
                        .interpretations
                        .windows(2)
                        .any(|rows| rows[0].form >= rows[1].form)
                    || mind
                        .reactions
                        .windows(2)
                        .any(|rows| rows[0].meaning >= rows[1].meaning)
                    || mind
                        .reactions
                        .iter()
                        .any(|row| !self.valid_meaning(row.meaning))
                    || mind.gift_preferences.iter().any(|row| {
                        !self.valid_meaning(row.meaning) || row.good == 0 || row.quantity == 0
                    })
                    || mind
                        .observations
                        .windows(2)
                        .any(|rows| rows[0].tick > rows[1].tick)
                    || mind.expressions.iter().any(|row| {
                        !self.valid_form(row.form)
                            || row.intended.is_some_and(|id| !self.valid_meaning(id))
                            || row.recipient.is_some_and(|id| id == 0 || id == mind.person)
                    })
                    || mind
                        .interpretations
                        .iter()
                        .any(|row| !self.valid_form(row.form) || !self.valid_meaning(row.meaning))
                    || mind.observations.iter().any(|row| {
                        row.tick > now
                            || row.actor == 0
                            || row.actor == mind.person
                            || !self.valid_form(row.form)
                            || row.interpreted.is_some_and(|id| !self.valid_meaning(id))
                    })
            })
            && !self.events.par_iter().any(|event| {
                event.tick > now
                    || event.actor == 0
                    || event.observer == 0
                    || event.actor == event.observer
                    || !self.valid_form(event.form)
                    || event.interpreted.is_some_and(|id| !self.valid_meaning(id))
                    || (!event.noticed && event.interpreted.is_some())
            })
    }

    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        if !self.valid_state(now)
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || mind
                        .expressions
                        .iter()
                        .any(|row| row.recipient.is_some_and(|id| !people.contains(&id)))
                    || mind
                        .observations
                        .iter()
                        .any(|row| !people.contains(&row.actor))
            })
            || self
                .events
                .par_iter()
                .any(|event| !people.contains(&event.actor) || !people.contains(&event.observer))
        {
            return Err("invalid personal signal state");
        }
        Ok(())
    }

    /// Potential explicit audiences; physical reach remains the host's decision.
    pub fn directed_targets(&self) -> Vec<(u64, u64)> {
        let mut targets: Vec<_> = self
            .minds
            .par_iter()
            .flat_map_iter(|mind| {
                mind.expressions
                    .iter()
                    .filter(|row| row.enabled)
                    .filter_map(|row| row.recipient.map(|recipient| (mind.person, recipient)))
            })
            .collect();
        if targets.par_windows(2).any(|rows| rows[0] > rows[1]) {
            targets.par_sort_unstable();
        }
        if targets.par_windows(2).any(|rows| rows[0] == rows[1]) {
            targets = targets
                .par_iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    (index == 0 || targets[index - 1] != *row).then_some(*row)
                })
                .collect();
        }
        targets
    }

    /// Interpret only noticed events, using each observer's own preferences.
    pub fn feeling_changes(&self) -> Vec<FeelingChange> {
        self.events
            .par_iter()
            .filter_map(|event| {
                let meaning = event.interpreted?;
                let mind = self.mind(event.observer)?;
                let index = mind
                    .reactions
                    .binary_search_by_key(&meaning, |row| row.meaning)
                    .ok()?;
                Some(FeelingChange {
                    observer: event.observer,
                    target: event.actor,
                    reaction: mind.reactions[index],
                })
            })
            .collect()
    }

    /// Opportunities arise from the potential giver's inference, not a request.
    pub fn gift_opportunities(&self) -> Vec<GiftOpportunity> {
        self.events
            .par_iter()
            .flat_map_iter(|event| {
                self.mind(event.observer).into_iter().flat_map(move |mind| {
                    mind.gift_preferences.iter().filter_map(move |preference| {
                        (event.interpreted == Some(preference.meaning)
                            && (preference.value > mind.gift_idle
                                || (preference.value == mind.gift_idle && mind.gift_on_tie)))
                            .then_some(GiftOpportunity {
                                giver: event.observer,
                                recipient: event.actor,
                                good: preference.good,
                                quantity: preference.quantity,
                                value: preference.value,
                            })
                    })
                })
            })
            .collect()
    }

    /// Prepare independent expressions from actual encounters.
    pub fn prepare(
        &self,
        contacts: &[Contact],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        self.prepare_with_directed(contacts, &[], seed, tick)
    }

    /// Directed audiences need not consent or choose an encounter. Each actor
    /// chooses at most one expression, and each observer decides what to notice.
    pub fn prepare_with_directed(
        &self,
        contacts: &[Contact],
        directed: &[Directed],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        if tick == 0 || !self.valid_state(tick) {
            return Err("invalid personal signal state");
        }
        let mut participants: Vec<_> = contacts
            .par_iter()
            .flat_map_iter(|row| [row.a, row.b])
            .collect();
        if participants.par_windows(2).any(|rows| rows[0] > rows[1]) {
            participants.par_sort_unstable();
        }
        if participants.first() == Some(&0)
            || participants.par_windows(2).any(|rows| rows[0] == rows[1])
        {
            return Err("invalid or overlapping signal encounters");
        }
        if directed
            .par_iter()
            .any(|row| row.actor == 0 || row.observer == 0 || row.actor == row.observer)
        {
            return Err("invalid directed signal audience");
        }
        let mut candidates: Vec<_> = contacts
            .par_iter()
            .flat_map_iter(|row| {
                [
                    Candidate {
                        actor: row.a,
                        observer: row.b,
                        hunger: row.a_hunger,
                        directed_only: false,
                    },
                    Candidate {
                        actor: row.b,
                        observer: row.a,
                        hunger: row.b_hunger,
                        directed_only: false,
                    },
                ]
            })
            .collect();
        candidates.par_extend(directed.par_iter().map(|row| Candidate {
            actor: row.actor,
            observer: row.observer,
            hunger: row.hunger,
            directed_only: true,
        }));
        let key = |row: &Candidate| (row.actor, row.observer, row.directed_only);
        if candidates
            .par_windows(2)
            .any(|rows| key(&rows[0]) > key(&rows[1]))
        {
            candidates.par_sort_unstable_by_key(key);
        }
        if candidates
            .par_windows(2)
            .any(|rows| rows[0].actor == rows[1].actor && rows[0].hunger != rows[1].hunger)
        {
            return Err("inconsistent personal signal state");
        }
        if candidates
            .par_windows(2)
            .any(|rows| rows[0].actor == rows[1].actor && rows[0].observer == rows[1].observer)
        {
            candidates = candidates
                .par_iter()
                .enumerate()
                .filter_map(|(index, row)| {
                    (index == 0
                        || candidates[index - 1].actor != row.actor
                        || candidates[index - 1].observer != row.observer)
                        .then_some(*row)
                })
                .collect();
        }
        let event = |group: &[Candidate]| {
            let actor = group[0].actor;
            let source = self.mind(actor)?;
            let (_, _, form, observer) = group
                .iter()
                .flat_map(|candidate| {
                    source.expressions.iter().filter_map(move |row| {
                        let allowed = if candidate.directed_only {
                            row.recipient == Some(candidate.observer)
                        } else {
                            row.recipient
                                .is_none_or(|target| target == candidate.observer)
                        };
                        if !row.enabled
                            || !allowed
                            || row
                                .hunger_at_least
                                .is_some_and(|threshold| candidate.hunger < threshold)
                        {
                            return None;
                        }
                        let value = i128::from(row.value)
                            + i128::from(row.hunger_weight) * i128::from(candidate.hunger) / 1000;
                        (value > i128::from(source.idle)
                            || (value == i128::from(source.idle) && source.act_on_tie))
                            .then_some((
                                value,
                                random(
                                    seed ^ tick.rotate_left(13)
                                        ^ actor
                                        ^ candidate.observer.rotate_left(17)
                                        ^ u64::from(row.form),
                                ),
                                row.form,
                                candidate.observer,
                            ))
                    })
                })
                .max()?;
            let listener = self.mind(observer);
            let noticed = listener.is_some_and(|mind| {
                random(
                    seed ^ tick.rotate_left(29)
                        ^ observer
                        ^ actor.rotate_left(11)
                        ^ u64::from(form),
                ) % 1000
                    < u64::from(mind.attention)
            });
            let interpreted = listener.filter(|_| noticed).and_then(|mind| {
                mind.interpretations
                    .binary_search_by_key(&form, |row| row.form)
                    .ok()
                    .map(|index| mind.interpretations[index].meaning)
            });
            Some(Event {
                tick,
                actor,
                observer,
                form,
                noticed,
                interpreted,
            })
        };
        let mut events: Vec<_> = candidates
            .par_chunk_by(|a, b| a.actor == b.actor)
            .filter_map(event)
            .collect();
        if events
            .par_windows(2)
            .any(|rows| (rows[0].observer, rows[0].actor) > (rows[1].observer, rows[1].actor))
        {
            events.par_sort_unstable_by_key(|event| (event.observer, event.actor));
        }
        let mut observed: Vec<_> = events
            .par_iter()
            .filter(|event| event.noticed)
            .copied()
            .collect();
        if observed
            .par_windows(2)
            .any(|rows| (rows[0].observer, rows[0].actor) > (rows[1].observer, rows[1].actor))
        {
            observed.par_sort_unstable_by_key(|event| (event.observer, event.actor));
        }
        let mut minds: Vec<_> = self.minds.par_iter().cloned().collect();
        minds.par_iter_mut().for_each(|mind| {
            if !mind.remember || mind.capacity == 0 {
                return;
            }
            let start = observed.partition_point(|event| event.observer < mind.person);
            let end = observed.partition_point(|event| event.observer <= mind.person);
            let keep_old = mind.capacity.saturating_sub(end - start);
            if mind.observations.len() > keep_old {
                mind.observations
                    .drain(..mind.observations.len() - keep_old);
            }
            let first = start.max(end.saturating_sub(mind.capacity));
            mind.observations
                .extend(observed[first..end].iter().map(|event| Observation {
                    tick,
                    actor: event.actor,
                    form: event.form,
                    interpreted: event.interpreted,
                }));
        });
        Ok(Self {
            catalog: self.catalog.clone(),
            minds,
            events,
        })
    }
}

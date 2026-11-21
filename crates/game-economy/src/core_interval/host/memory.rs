//! Private memories from committed first-hand events.
use super::*;
use game_economy_knowledge::episodes::{Cue, Memory, Observation, Recall};
use std::sync::Arc;

impl CoreHost {
    fn episode_memory(&self) -> Option<&Memory> {
        self.knowledge.as_ref()?.episodes.as_deref()
    }

    fn episode_memory_mut(&mut self) -> Option<&mut Memory> {
        self.knowledge
            .as_mut()?
            .episodes
            .as_mut()
            .map(Arc::make_mut)
    }

    pub fn recall_episodes_controlled(
        &self,
        person: u64,
        other: Option<u64>,
    ) -> Result<Vec<Recall>, String> {
        let actor = self.controlled_person(person)?;
        if actor.health == 0 {
            return Err("person cannot recall an experience".into());
        }
        Ok(self.episode_memory().map_or_else(Vec::new, |memory| {
            memory.recall(person, other, self.core.simulated_millis())
        }))
    }

    pub fn reflect_on_episode_controlled(&mut self, person: u64, cue: Cue) -> Result<bool, String> {
        let actor = self.controlled_person(person)?;
        if actor.health == 0 {
            return Err("person cannot reflect on an experience".into());
        }
        let now = self.core.simulated_millis();
        Ok(self
            .episode_memory_mut()
            .is_some_and(|memory| memory.reflect(person, cue, now)))
    }

    pub(super) fn remember_meetings(&mut self, meetings: &[game_social::Encounter], now: u128) {
        if meetings.is_empty() {
            return;
        }
        let Some(memory) = self.episode_memory_mut() else {
            return;
        };
        let mut observations = meetings
            .par_iter()
            .flat_map_iter(|meeting| {
                [(meeting.a, meeting.b), (meeting.b, meeting.a)].map(|(person, other)| {
                    Observation {
                        person,
                        at_millis: now,
                        cue: Cue::Meeting {
                            other,
                            region: meeting.region,
                        },
                    }
                })
            })
            .collect::<Vec<_>>();
        memory.remember(&mut observations, now);
    }

    pub(super) fn remember_pastimes(&mut self, events: &[game_pastimes::Event]) {
        if events.is_empty() {
            return;
        }
        let now = self.core.simulated_millis();
        let Some(memory) = self.episode_memory_mut() else {
            return;
        };
        let mut observations = events
            .par_iter()
            .filter(|event| event.outcome == game_pastimes::Outcome::Completed)
            .map(|event| Observation {
                person: event.person,
                at_millis: event.at_millis,
                cue: Cue::CreativePractice {
                    activity: event.activity,
                },
            })
            .collect::<Vec<_>>();
        memory.remember(&mut observations, now);
    }

    pub(super) fn record_episode_rest(&mut self, choices: impl IntoIterator<Item = (u64, bool)>) {
        let mut choices = choices.into_iter().peekable();
        if choices.peek().is_none() {
            return;
        }
        let now = self.core.simulated_millis();
        if let Some(memory) = self.episode_memory_mut() {
            for (person, resting) in choices {
                memory.rest(person, resting, now);
            }
        }
    }
}

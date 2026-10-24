//! Personal memory traces and request-local cue joins; no world state.
use game_core::*;
use game_sim::*;
mod persistence;
mod resource;
mod validation;
use game_storage::{Records, Table};
pub use resource::{Content, Fact, Facts, Memories};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
mod cues;
pub use cues::CueIndex;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub learned: SimTime,
    pub rehearsed: SimTime,
    pub expires: Option<SimTime>,
    pub importance: u16,
    pub emotion: i16,
    pub strength: u16,
    pub recalls: u64,
    pub cues: Vec<String>,
    pub related: Vec<Arc<str>>,
}
impl Trace {
    pub fn legacy(observed: SimTime, cues: Vec<String>) -> Self {
        Self {
            learned: observed,
            rehearsed: observed,
            expires: None,
            importance: 500,
            emotion: 0,
            strength: 500,
            recalls: 0,
            cues,
            related: vec![],
        }
    }

    pub fn accessibility(&self, at: SimTime) -> u16 {
        if let Some(end) = self.expires {
            return (u128::from(end.0.saturating_sub(at.0)) * 1000
                / u128::from(end.0 - self.learned.0))
            .min(1000) as u16;
        }
        self.strength
            .saturating_sub((at.0.saturating_sub(self.rehearsed.0) / (30 * DAY.0)).min(1000) as u16)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Book {
    pub entries: Records<Arc<str>, Trace>,
    pub sweep: Option<ScheduleId>,
    pub due: Option<SimTime>,
}

impl Book {
    pub fn crowded_short(&self, limit: usize) -> Vec<Arc<str>> {
        if self.entries.len() <= limit {
            return vec![];
        }
        let mut ranked: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, t)| t.expires.is_some())
            .map(|(k, t)| {
                (
                    t.importance.max(t.emotion.unsigned_abs()),
                    t.learned,
                    k.clone(),
                )
            })
            .collect();
        let excess = ranked.len().saturating_sub(limit);
        if excess == 0 {
            return vec![];
        }
        ranked.sort();
        ranked.into_iter().take(excess).map(|x| x.2).collect()
    }
    pub fn crowded_long(&self, now: SimTime, limit: usize) -> Vec<Arc<str>> {
        if self.entries.len() <= limit {
            return vec![];
        }
        let mut ranked: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, t)| t.expires.is_none())
            .map(|(k, t)| {
                (
                    t.accessibility(now),
                    t.importance.max(t.emotion.unsigned_abs()),
                    t.rehearsed,
                    k.clone(),
                )
            })
            .collect();
        let excess = ranked.len().saturating_sub(limit);
        if excess == 0 {
            return vec![];
        }
        ranked.sort();
        ranked.into_iter().take(excess).map(|x| x.3).collect()
    }
}

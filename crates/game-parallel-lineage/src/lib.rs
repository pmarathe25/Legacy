//! Reincarnation through actual births, independent of property and office.
use game_parallel_families::Families;
pub use game_parallel_invocation::{
    Invocation, Rules as InvocationRules, Warning as InvocationWarning,
    WarningKind as InvocationWarningKind,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lineage {
    pub current: Option<u64>,
    pub started: u64,
    pub waiting_since: Option<u64>,
    #[serde(default)]
    pub history: Vec<Life>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<game_parallel_invocation::Invocation>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Life {
    pub person: u64,
    pub started: u64,
    pub ended: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub kind: EventKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Death,
    Succession,
}
impl Lineage {
    pub fn controlled_person(&self, now: u64) -> Option<u64> {
        self.current.filter(|_| {
            self.suspended_person(now).is_none()
                && !self
                    .invocation
                    .as_ref()
                    .is_some_and(|invocation| invocation.will_lose_control())
        })
    }
    pub fn suspended_person(&self, now: u64) -> Option<u64> {
        self.invocation
            .as_ref()
            .and_then(|invocation| invocation.control_until.filter(|&until| now < until))
            .and(self.current)
    }
    pub fn validate_optional(
        lineage: Option<&Self>,
        families: Option<&Families>,
        now: u64,
    ) -> Result<(), String> {
        if let Some(lineage) = lineage {
            lineage.validate(families.ok_or("lineage requires families")?, now)?;
        }
        Ok(())
    }
    pub fn validate(&self, families: &Families, now: u64) -> Result<(), String> {
        // Keep the lookup proportional to the lineage, rather than allocating a
        // hash table for every person on every tick. Family validation owns the
        // uniqueness invariant for its members.
        let wanted: HashSet<_> = self
            .history
            .iter()
            .map(|life| life.person)
            .chain(self.events.iter().map(|event| event.person))
            .chain(self.current)
            .collect();
        let people: HashSet<_> = families
            .members
            .par_iter()
            .filter(|member| wanted.contains(&member.id))
            .map(|member| (member.id, member.alive))
            .collect();
        let mut seen = HashSet::new();
        if self.started > now
            || self.current.is_some() == self.waiting_since.is_some()
            || self.waiting_since.is_some_and(|tick| tick > now)
            || self.current.is_some_and(|id| !people.contains(&(id, true)))
            || self.history.iter().any(|life| {
                !(people.contains(&(life.person, true)) || people.contains(&(life.person, false)))
                    || !seen.insert(life.person)
                    || Some(life.person) == self.current
                    || life.started > life.ended
                    || life.ended > now
            })
            || self.events.iter().any(|e| {
                e.tick > now
                    || !(people.contains(&(e.person, true)) || people.contains(&(e.person, false)))
            })
        {
            return Err("invalid lineage state".into());
        }
        if let Some(invocation) = &self.invocation {
            invocation.validate(
                self.current,
                &self
                    .history
                    .iter()
                    .map(|life| life.person)
                    .collect::<Vec<_>>(),
                now,
            )?;
        }
        Ok(())
    }
    /// Prepare a private invocation generation. A warning is published at the
    /// existing boundary and prevents every other owner from advancing.
    pub fn prepare_invocation(
        &mut self,
        now: u64,
    ) -> Result<Option<game_parallel_invocation::Invocation>, &'static str> {
        let Some(invocation) = &self.invocation else {
            return Ok(None);
        };
        let mut next = invocation.clone();
        if !next.prepare(now).map_err(|_| "invalid invocation state")? {
            self.invocation = Some(next);
            return Err("invocation warning needs a choice");
        }
        next.advance(now).map_err(|_| "invalid invocation state")?;
        Ok(Some(next))
    }
    /// Called after a validated family boundary; no person is created by succession.
    pub fn advance(&mut self, families: &Families, tick: u64) {
        let alive = self.current.and_then(|id| {
            families
                .members
                .par_iter()
                .find_any(|member| member.id == id)
                .map(|member| member.alive)
        });
        let newborn = families
            .members
            .par_iter()
            .filter(|member| member.alive && member.born == Some(tick))
            .map(|member| member.id)
            .min();
        self.advance_boundary(alive, newborn, tick);
    }
    /// A validated committed lifecycle boundary already identifies deaths and births.
    pub fn advance_events(&mut self, events: &[game_parallel_families::FamilyEvent], tick: u64) {
        let alive = self.current.map(|id| {
            !events.iter().any(|event| {
                event.tick == tick && event.kind == "death" && event.people.first() == Some(&id)
            })
        });
        let newborn = events
            .iter()
            .filter(|event| event.tick == tick && event.kind == "birth")
            .filter_map(|event| event.people.first().copied())
            .min();
        self.advance_boundary(alive, newborn, tick);
    }
    pub fn advance_boundary(&mut self, alive: Option<bool>, newborn: Option<u64>, tick: u64) {
        self.events.clear();
        if let Some(current) = self.current
            && alive == Some(false)
        {
            self.history.push(Life {
                person: current,
                started: self.started,
                ended: tick,
            });
            self.events.push(Event {
                tick,
                person: current,
                kind: EventKind::Death,
            });
            self.current = None;
            self.waiting_since = Some(tick);
            if let Some(invocation) = &mut self.invocation {
                invocation.bind(None);
            }
        }
        if self.current.is_none()
            && self.waiting_since.is_some_and(|since| since < tick)
            && let Some(person) = newborn
        {
            self.current = Some(person);
            self.started = tick;
            self.waiting_since = None;
            if let Some(invocation) = &mut self.invocation {
                invocation.bind(Some(person));
            }
            self.events.push(Event {
                tick,
                person,
                kind: EventKind::Succession,
            });
        }
    }
}
/// Player-directed incarnations do not receive autonomous actor decisions.
pub fn exclude_controlled(lineage: Option<&Lineage>, now: u64, busy: &mut Vec<u64>) {
    if let Some(person) = lineage.and_then(|lineage| lineage.controlled_person(now)) {
        busy.push(person);
    }
}

/// Whether invocation has configured physical consequences requiring the fused host.
pub fn has_physical_strain(lineage: Option<&Lineage>) -> bool {
    lineage
        .and_then(|lineage| lineage.invocation.as_ref())
        .is_some_and(|invocation| {
            invocation.rules.strain_damage_per_tick > 0
                || invocation.rules.lifespan_loss_per_tick > 0
        })
}

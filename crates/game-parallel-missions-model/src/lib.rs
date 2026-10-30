//! Saved personal journeys and their physical transitions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Ready,
    Outbound,
    Visiting,
    Returning,
    Completed,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    pub id: u64,
    pub person: u64,
    /// Linked physical journeys; participants may be willing or pressured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party: Option<u64>,
    /// The traveler explicitly agrees to join a matching expedition party.
    #[serde(default)]
    pub party_opt_in: bool,
    /// The person participates under pressure without private approval.
    #[serde(default)]
    pub party_compliant: bool,
    pub origin: u64,
    pub destination: u64,
    pub visit_ticks: u64,
    #[serde(default)]
    pub remaining: u64,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<Objective>,
    #[serde(default)]
    pub objective_met: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Objective {
    Meet { person: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Departure,
    Arrival,
    VisitComplete,
    Return,
    Completed,
    Unavailable,
    Rejected,
    Died,
    Cancelled,
    ObjectiveMet,
    ObjectiveFailed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub mission: u64,
    pub person: u64,
    pub kind: EventKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub person: u64,
    pub destination: u64,
    pub visit_ticks: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Departure {
    pub mission: u64,
    pub person: u64,
    pub party: Option<u64>,
    pub index: usize,
    pub source: u64,
    pub destination: u64,
    pub outbound: bool,
}

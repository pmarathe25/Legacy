//! Data for individual social assessments.
use serde::{Deserialize, Serialize};
mod merge;
pub use merge::{Change, merge};

/// Individual responses to experience; absent values use the authored defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sensitivity {
    pub person: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fear_per_damage: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gratitude_gain: Option<u64>,
    /// Optional personal feeling from making a promise; absent means no automatic feeling.
    #[serde(default)]
    pub promise_obligation_gain: Option<u64>,
    /// Personal reduction in felt obligation after giving help.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_relief: Option<u64>,
    /// Personal trust loss on witnessing another person use a claimed good.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theft_loss: Option<u32>,
    /// An individual tendency to confront people they personally distrust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confrontation: Option<Confrontation>,
    /// Personal interpretation of receiving help or harm and of encounters.
    #[serde(default)]
    pub affinity_help: Option<i32>,
    #[serde(default)]
    pub affinity_harm_per_damage: Option<i32>,
    #[serde(default)]
    pub affinity_encounter: Option<i32>,
    #[serde(default)]
    pub affinity_pressured_encounter: Option<i32>,
    /// Personal interpretation of another person's witnessed use of a claimed site.
    #[serde(default)]
    pub affinity_site_use: Option<i32>,
    /// Personal interpretation of another person's witnessed destruction of a claimed site.
    #[serde(default)]
    pub affinity_site_destruction: Option<i32>,
    /// Optional interpretations of experiences with particular people.
    #[serde(default)]
    pub relationship_cases: Vec<RelationshipCase>,
    /// Optional, individually configured change in felt-obligation disposition during childhood.
    #[serde(default)]
    pub obligation_development: Option<ObligationDevelopment>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObligationDevelopment {
    pub until_age: u64,
    pub minimum: u64,
    pub maximum: u64,
    #[serde(default)]
    pub cared_change: i64,
    #[serde(default)]
    pub unattended_change: i64,
    #[serde(default)]
    pub cared_promise_change: Option<i64>,
    #[serde(default)]
    pub unattended_promise_change: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipCase {
    pub target: u64,
    #[serde(default)]
    pub help: Option<i32>,
    #[serde(default)]
    pub harm_per_damage: Option<i32>,
    #[serde(default)]
    pub encounter: Option<i32>,
    #[serde(default)]
    pub pressured_encounter: Option<i32>,
    #[serde(default)]
    pub site_use: Option<i32>,
    #[serde(default)]
    pub site_destruction: Option<i32>,
    #[serde(default)]
    pub obligation_gain: Option<u64>,
    #[serde(default)]
    pub promise_obligation_gain: Option<u64>,
    #[serde(default)]
    pub obligation_relief: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confrontation {
    pub trust_at_most: i32,
    pub fear_at_most: u64,
    pub interval: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
/// Independent directed liking, learned only from personal experience.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffinityRules {
    pub limit: i32,
    #[serde(default)]
    pub encounter_change: i32,
    #[serde(default)]
    pub pressured_encounter_change: i32,
    #[serde(default)]
    pub help_change: i32,
    #[serde(default)]
    pub harm_change_per_damage: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub minimum: i32,
    pub maximum: i32,
    pub initial: i32,
    pub fulfilled_gain: u32,
    pub default_loss: u32,
    /// Trust lost when the owner witnesses an unauthorized taking.
    #[serde(default)]
    pub theft_loss: u32,
    /// Optional directed trust gain from a consenting encounter.
    #[serde(default)]
    pub encounter_gain: u32,
    /// Personal fear gained per unit of directly suffered harm.
    #[serde(default)]
    pub fear_per_damage: u64,
    /// Independent fear reduction per tick; zero preserves fear indefinitely.
    #[serde(default)]
    pub fear_decay_per_tick: u64,
    /// Personal obligation gained from directly receiving a gift.
    #[serde(default)]
    pub gratitude_gain: u64,
    /// Default reduction in felt obligation after giving a one-sided gift.
    #[serde(default)]
    pub obligation_relief: u64,
    /// Ignore reports from a sender whose directed trust falls below this threshold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_report_trust: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity: Option<AffinityRules>,
    /// Optional disposition template for actual newborns; it copies no relationships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newborn_obligation_development: Option<ObligationDevelopment>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opinion {
    pub observer: u64,
    pub target: u64,
    pub trust: i32,
    #[serde(default)]
    pub fear: u64,
    #[serde(default)]
    pub obligation: u64,
    /// Directed liking, independent of trust, fear, and obligation.
    #[serde(default)]
    pub affinity: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub observer: u64,
    pub target: u64,
    pub before: i32,
    pub after: i32,
}

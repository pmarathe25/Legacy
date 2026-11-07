//! Data for individual social assessments.
use serde::{Deserialize, Serialize};
use std::sync::Arc;
mod merge;
pub use merge::{Change, merge};

/// Individual responses to experience; absent values use the authored defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sensitivity {
    pub person: u64,
    /// This person's policy for accepting reports; absent uses the scenario default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_reception: Option<ReportReception>,
    /// Signed personal reaction when an earlier food report differs from direct observation.
    #[serde(default)]
    pub affinity_report_discrepancy: Option<i32>,
    /// Signed trust reaction to an observation differing from a remembered report.
    #[serde(default)]
    pub trust_report_discrepancy: Option<i32>,
    /// Signed personal trust reactions to witnessed promise outcomes.
    #[serde(default)]
    pub trust_promise_fulfilled: Option<i32>,
    #[serde(default)]
    pub trust_promise_unfulfilled: Option<i32>,
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
    /// Signed trust reaction to witnessed use of claimed stock; overrides theft_loss.
    #[serde(default)]
    pub trust_stock_use: Option<i32>,
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
    /// How this person weights directed affinity when seeking social meetings.
    #[serde(default)]
    pub social_affinity_weight: Option<i32>,
    /// Personal interpretation of another person's witnessed use of claimed regional stock.
    #[serde(default)]
    pub affinity_stock_use: Option<i32>,
    /// Personal interpretation of another person's witnessed use of a claimed site.
    #[serde(default)]
    pub affinity_site_use: Option<i32>,
    /// Personal interpretation of another person's witnessed destruction of a claimed site.
    #[serde(default)]
    pub affinity_site_destruction: Option<i32>,
    /// Personal interpretation of another person's witnessed recovery of claimed debris.
    #[serde(default)]
    pub affinity_site_salvage: Option<i32>,
    /// Personal reactions to witnessed requested work and to a refusal.
    #[serde(default)]
    pub affinity_work_success: Option<i32>,
    #[serde(default)]
    pub affinity_work_refusal: Option<i32>,
    /// This person's own signed interpretation of capture, resistance, and release.
    #[serde(default)]
    pub confinement_reaction: Option<ConfinementReaction>,
    /// Personal interpretations of directly opposed actions; absent outcomes do nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opposed_action_reaction: Option<Arc<OpposedActionReaction>>,
    /// Signed personal interpretations of giving or receiving a lesson.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teaching_reaction: Option<Arc<TeachingReaction>>,
    /// Optional interpretations of experiences with particular people.
    #[serde(default)]
    pub relationship_cases: Vec<RelationshipCase>,
    /// Optional, individually configured change in felt-obligation disposition during childhood.
    #[serde(default)]
    pub obligation_development: Option<ObligationDevelopment>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReception {
    Always,
    Never,
    TrustAtLeast(i32),
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipCase {
    pub target: u64,
    #[serde(default)]
    pub report_discrepancy: Option<i32>,
    #[serde(default)]
    pub report_discrepancy_trust: Option<i32>,
    /// Reactions to this particular person's observed promise outcomes.
    #[serde(default)]
    pub promise_fulfilled_trust: Option<i32>,
    #[serde(default)]
    pub promise_unfulfilled_trust: Option<i32>,
    #[serde(default)]
    pub help: Option<i32>,
    #[serde(default)]
    pub harm_per_damage: Option<i32>,
    #[serde(default)]
    pub encounter: Option<i32>,
    #[serde(default)]
    pub pressured_encounter: Option<i32>,
    /// Relationship-specific weight when considering this person for a meeting.
    #[serde(default)]
    pub social_affinity_weight: Option<i32>,
    /// Override the observer's general reaction to this person's stock use.
    #[serde(default)]
    pub stock_use: Option<i32>,
    /// Signed trust override for this person's witnessed stock use.
    #[serde(default)]
    pub stock_use_trust: Option<i32>,
    #[serde(default)]
    pub site_use: Option<i32>,
    #[serde(default)]
    pub site_destruction: Option<i32>,
    #[serde(default)]
    pub site_salvage: Option<i32>,
    #[serde(default)]
    pub work_success: Option<i32>,
    #[serde(default)]
    pub work_refusal: Option<i32>,
    /// Overrides this observer's general confinement interpretation for this person.
    #[serde(default)]
    pub confinement_reaction: Option<ConfinementReaction>,
    /// Per-outcome overrides for this particular person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opposed_action_reaction: Option<Arc<OpposedActionReaction>>,
    /// Per-person lesson interpretations override the general disposition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teaching_reaction: Option<Arc<TeachingReaction>>,
    #[serde(default)]
    pub obligation_gain: Option<u64>,
    #[serde(default)]
    pub promise_obligation_gain: Option<u64>,
    #[serde(default)]
    pub obligation_relief: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectiveReaction {
    #[serde(default)]
    pub trust: i32,
    #[serde(default)]
    pub affinity: i32,
    #[serde(default)]
    pub fear: i64,
    #[serde(default)]
    pub obligation: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeachingReaction {
    #[serde(default)]
    pub given: Option<SubjectiveReaction>,
    #[serde(default)]
    pub received: Option<SubjectiveReaction>,
}

/// An observer's signed interpretations of an action opposed by another person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpposedActionReaction {
    #[serde(default)]
    pub blocked: Option<SubjectiveReaction>,
    #[serde(default)]
    pub overcame: Option<SubjectiveReaction>,
    #[serde(default)]
    pub blocked_other: Option<SubjectiveReaction>,
    #[serde(default)]
    pub was_overcome: Option<SubjectiveReaction>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfinementReaction {
    #[serde(default)]
    pub captured: SubjectiveReaction,
    #[serde(default)]
    pub resisted: SubjectiveReaction,
    #[serde(default)]
    pub released: SubjectiveReaction,
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

use super::*;
use game_provisioning::Plan;
use game_utility::{Candidate, Decision};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_tier: Option<game_choice::FrameTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urgent_frame_at: Option<SimTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_retry: Option<SimTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub considered: Option<std::sync::Arc<crate::Consideration>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<Box<Routine>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_clock: Option<ReviewClock>,
    pub plan: Option<Plan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    #[serde(
        default,
        deserialize_with = "engagement",
        skip_serializing_if = "Option::is_none"
    )]
    pub engagement: Option<u64>,
    pub retry_after: BTreeMap<u64, SimTime>,
    pub last: Option<Decision>,
    pub failures: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectation: Option<Candidate>,
}
fn engagement<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_u64().or_else(|| value.get("id")?.as_u64()))
}

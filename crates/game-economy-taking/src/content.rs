//! Authored names are resolved before the physical simulation starts.
use crate::{
    Outcome, Policy, Reaction, Reception, ReportDetails, ReportPolicy, ReportSource, Response,
    Taking, Witness,
};
use serde::Deserialize;

fn one() -> u64 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub complaint_preferences: Vec<crate::ComplaintPreference>,
    #[serde(default)]
    pub policies: Vec<PolicyConfig>,
    #[serde(default)]
    pub responses: Vec<Response>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub witnesses: Vec<Witness>,
    #[serde(default)]
    pub report_policies: Vec<ReportPolicyConfig>,
    #[serde(default)]
    pub receptions: Vec<Reception>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportPolicyConfig {
    pub person: u64,
    pub recipient: u64,
    pub actor: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub claimed_outcome: Option<Outcome>,
    #[serde(default)]
    pub source: ReportSource,
    #[serde(default)]
    pub heard_from: Option<u64>,
    #[serde(default)]
    pub details: Option<ReportDetailsConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportDetailsConfig {
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default)]
    pub good: Option<String>,
    #[serde(default)]
    pub quantity: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfig {
    pub person: u64,
    pub target: u64,
    pub good: String,
    pub quantity: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default = "one")]
    pub interval: u64,
    #[serde(default)]
    pub escalate: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
    #[serde(default)]
    pub affinity_at_least: Option<i32>,
    #[serde(default)]
    pub affinity_at_most: Option<i32>,
}

impl Config {
    pub fn build(self, good: impl Fn(&str) -> Option<u32>) -> Result<Taking, String> {
        let policies = self
            .policies
            .into_iter()
            .map(|p| {
                Ok(Policy {
                    person: p.person,
                    target: p.target,
                    good: good(&p.good).ok_or_else(|| format!("unknown taking good {}", p.good))?,
                    quantity: p.quantity,
                    willing: p.willing,
                    interval: p.interval,
                    escalate: p.escalate,
                    pressured_by: p.pressured_by,
                    pressure_threshold: p.pressure_threshold,
                    affinity_at_least: p.affinity_at_least,
                    affinity_at_most: p.affinity_at_most,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let report_policies = self
            .report_policies
            .into_iter()
            .map(|p| {
                let details = p
                    .details
                    .map(|d| {
                        Ok::<_, String>(ReportDetails {
                            target: d.target,
                            good: d
                                .good
                                .map(|name| {
                                    good(&name)
                                        .ok_or_else(|| format!("unknown reported good {name}"))
                                })
                                .transpose()?,
                            quantity: d.quantity,
                        })
                    })
                    .transpose()?;
                Ok::<_, String>(ReportPolicy {
                    person: p.person,
                    recipient: p.recipient,
                    actor: p.actor,
                    willing: p.willing,
                    pressure_threshold: p.pressure_threshold,
                    claimed_outcome: p.claimed_outcome,
                    source: p.source,
                    heard_from: p.heard_from,
                    details,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Taking {
            complaints: Default::default(),
            complaint_preferences: self.complaint_preferences.into(),
            policies: policies.into(),
            responses: self.responses.into(),
            reactions: self.reactions.into(),
            witnesses: self.witnesses.into(),
            report_policies: report_policies.into(),
            receptions: self.receptions.into(),
            reports: Default::default(),
            events: vec![],
        })
    }
}

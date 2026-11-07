//! Authored names are resolved before the physical simulation starts.
use crate::{Policy, Reaction, Reception, ReportPolicy, Response, Taking, Witness};
use serde::Deserialize;

fn one() -> u64 {
    1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub policies: Vec<PolicyConfig>,
    #[serde(default)]
    pub responses: Vec<Response>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub witnesses: Vec<Witness>,
    #[serde(default)]
    pub report_policies: Vec<ReportPolicy>,
    #[serde(default)]
    pub receptions: Vec<Reception>,
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
        Ok(Taking {
            policies: policies.into(),
            responses: self.responses.into(),
            reactions: self.reactions.into(),
            witnesses: self.witnesses.into(),
            report_policies: self.report_policies.into(),
            receptions: self.receptions.into(),
            reports: Default::default(),
            events: vec![],
        })
    }
}

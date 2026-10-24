use game_core::*;
use game_sim::Request;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<crate::AttentionRates>,
    /// Experiment: heard care signals interrupt only for changed food evidence.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub care_signal_changes_only: bool,
    /// Coalesce interruptions while working; idle choices and completions stay prompt.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub busy_attention: bool,
    pub review: SimDuration,
    pub retry: SimDuration,
    pub meal_hunger: u64,
    pub urgent_hunger: u64,
    pub hunger_interrupts: std::collections::BTreeSet<String>,
    pub rest_fatigue: u64,
    pub switching_margin: i64,
    pub travel_effort_per_minute: u64,
    pub gathering_effort: u64,
    pub gathering_meals: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<crate::Policy>,
    /// Behavioral experiment: reuse non-survival offers between ordinary decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_memory: Option<SimDuration>,
    #[serde(default = "option_limit", skip_serializing_if = "standard_limit")]
    pub option_limit: usize,
}
fn option_limit() -> usize {
    16
}
fn standard_limit(value: &usize) -> bool {
    *value == option_limit()
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            attention: None,
            care_signal_changes_only: false,
            busy_attention: false,
            review: MINUTE,
            retry: SimDuration(5 * MINUTE.0),
            meal_hunger: 30,
            urgent_hunger: 100,
            hunger_interrupts: ["rest", "craft", "farm", "build"].map(str::to_owned).into(),
            rest_fatigue: 10,
            switching_margin: 5,
            travel_effort_per_minute: 1,
            gathering_effort: 10,
            gathering_meals: 1,
            routine: None,
            option_memory: None,
            option_limit: option_limit(),
        }
    }
}
impl Rules {
    pub fn review_floor(&self) -> Option<SimDuration> {
        self.attention
            .map(|p| p.ordinary)
            .or_else(|| self.routine.as_ref().and_then(|p| p.review_floor))
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(rates) = self.attention {
            rates.validate()?;
            require(
                self.routine.is_none() && self.option_memory.is_none(),
                "incompatible attention policies",
            )?;
        }
        if let Some(policy) = &self.routine {
            policy.validate()?;
        }
        require(
            self.option_memory.is_none_or(|span| span.0 > 0)
                && (self.option_memory.is_none() || self.routine.is_none())
                && self.option_limit > 0
                && self.option_limit <= game_choice::LIMIT
                && self.review.0 > 0
                && self.retry.0 > 0
                && self.meal_hunger > 0
                && self.urgent_hunger >= self.meal_hunger
                && self.switching_margin >= 0
                && self.gathering_meals > 0,
            "invalid autonomous survival rules",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure(pub Rules);
impl Request for Configure {
    const KEY: &'static str = "characters.configure";
    type Response = ();
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Rules> {
    let doc: game_content::Document<Rules> = store.load(&format!("characters/{key}.toml"))?;
    doc.data.validate()?;
    Ok(doc.data)
}

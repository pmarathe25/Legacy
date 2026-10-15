use super::*;
/// Defaults are the isolated survival fixture; campaigns select an authored profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
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
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            review: MINUTE,
            retry: SimDuration(5 * MINUTE.0),
            meal_hunger: 30,
            urgent_hunger: 100,
            hunger_interrupts: ["rest", "craft", "farm", "build"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            rest_fatigue: 10,
            switching_margin: 5,
            travel_effort_per_minute: 1,
            gathering_effort: 10,
            gathering_meals: 1,
        }
    }
}
impl Rules {
    pub fn validate(&self) -> Result<()> {
        require(
            self.review.0 > 0
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

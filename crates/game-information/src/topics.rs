use super::*;
use game_knowledge::{FactValue, Facts, Stock, StoreLocation, known, location_key};
/// These are semantic topics, not access to a global index or another mind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Topic {
    Fact(String),
    Category(String),
    Supplies(GoodId),
}
impl Topic {
    pub(super) fn valid(&self) -> bool {
        match self {
            Self::Fact(s) | Self::Category(s) => !s.is_empty() && s.len() <= 150,
            Self::Supplies(g) => g.0 > 0,
        }
    }
    pub fn concept(&self) -> String {
        match self {
            Self::Fact(s) => s.clone(),
            Self::Category(s) => s.trim_end_matches(':').to_owned(),
            Self::Supplies(g) => format!("good:{}", g.0),
        }
    }
    pub(super) fn relevant_title(&self, key: &str) -> bool {
        match self {
            Self::Fact(s) => key == s,
            Self::Category(s) => key.starts_with(s),
            Self::Supplies(_) => key.starts_with("store:") || key.starts_with("patch:"),
        }
    }
    pub fn matches(&self, key: &str, value: &FactValue) -> bool {
        match self {
            Self::Fact(s) => key == s,
            Self::Category(s) => key.starts_with(s),
            Self::Supplies(g) => {
                key.starts_with("store:")
                    && value
                        .decode::<Stock>()
                        .is_some_and(|stock| stock.get(g).is_some_and(|q| *q > 0))
                    || key.starts_with("patch:")
                        && value
                            .decode::<game_ecology::Patch>()
                            .is_some_and(|p| p.good == *g && p.stock > 0)
            }
        }
    }
}
pub(super) fn supporting(facts: &Facts, key: &str, here: AreaId) -> Vec<String> {
    let mut keys = vec![key.to_owned()];
    let destination = if let Some(id) = key.strip_prefix("store:").and_then(|s| s.parse().ok()) {
        let location = location_key(InventoryId(id));
        if facts.contains_key(location.as_str()) {
            keys.push(location.clone());
        }
        known::<StoreLocation>(facts, &location).map(|p| p.area)
    } else if key.starts_with("patch:") {
        known::<game_ecology::Patch>(facts, key).map(|p| p.area)
    } else {
        None
    };
    if let Some(destination) = destination
        && let Ok(route) = game_knowledge::route(facts, here, destination)
    {
        let roads = route
            .areas
            .iter()
            .take(6)
            .map(|id| game_knowledge::roads_key(*id));
        keys.extend(roads);
    }
    keys
}

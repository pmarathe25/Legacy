use super::*;
/// Cache immutable payloads only while sources match; personal evidence stays on each fact.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(untagged)]
pub enum FactValue {
    Roads(#[serde(serialize_with = "roads_wire")] Arc<game_land::Area>),
    Patch(game_ecology::Patch),
    Stock(#[serde(serialize_with = "stock_wire")] Table<GoodId, u64>),
    Location(StoreLocation),
    Theory(Box<game_reasoning::Claim>),
    Measurement(Box<game_reasoning::Observation>),
    Mechanical(game_physics::MechanicalReading),
    Other(serde_json::Value),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cached {
    pub key: Arc<str>,
    pub value: Arc<FactValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Subject {
    Roads(AreaId),
    Patch(u64),
    Stock(InventoryId),
    Location(InventoryId),
}
impl Subject {
    pub fn key(&self) -> String {
        match *self {
            Self::Roads(id) => roads_key(id),
            Self::Patch(id) => patch_key(id),
            Self::Stock(id) => store_key(id),
            Self::Location(id) => location_key(id),
        }
    }
}

impl<'de> Deserialize<'de> for FactValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        game_facts::deserialize_preferred(d, Self::Mechanical, Self::Other)
    }
}
impl FactValue {
    pub fn decode<T: serde::de::DeserializeOwned + Clone + 'static>(&self) -> Option<T> {
        let (typed, json): (&dyn std::any::Any, _) = match self {
            Self::Roads(v) => (v.as_ref(), None),
            Self::Patch(v) => (v, None),
            Self::Stock(v) => (v, None),
            Self::Location(v) => (v, None),
            Self::Theory(v) => (v.as_ref(), None),
            Self::Measurement(v) => (v.as_ref(), None),
            Self::Mechanical(v) => (&v.measured, None),
            Self::Other(v) => (v, Some(v)),
        };
        game_facts::decode(typed, self, json)
    }
    pub(super) fn promote(value: Arc<Self>, key: &str) -> Arc<Self> {
        let Self::Other(raw) = value.as_ref() else {
            return value;
        };
        let typed = if let Some(reading) = game_physics::MechanicalReading::from_json(raw) {
            Ok(Self::Mechanical(reading))
        } else if key.starts_with("roads:") {
            serde_json::from_value(raw.clone()).map(Self::Roads)
        } else if key.starts_with("store-location:") {
            serde_json::from_value(raw.clone()).map(Self::Location)
        } else if key.starts_with("store:") {
            serde_json::from_value(raw.clone()).map(Self::Stock)
        } else if key.starts_with("patch:") {
            serde_json::from_value(raw.clone()).map(Self::Patch)
        } else if key.starts_with("theory:") {
            game_reasoning::Claim::deserialize(raw).map(|c| Self::Theory(Box::new(c)))
        } else if key.starts_with("measured:") {
            game_reasoning::Observation::deserialize(raw).map(|o| Self::Measurement(Box::new(o)))
        } else {
            return value;
        };
        typed
            .ok()
            .filter(|typed| serde_json::to_value(typed).ok().as_ref() == Some(raw))
            .map_or(value, Arc::new)
    }
}
impl From<serde_json::Value> for FactValue {
    fn from(value: serde_json::Value) -> Self {
        Self::Other(value)
    }
}
fn stock_wire<S: serde::Serializer>(
    stock: &Stock,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    let entries: std::collections::BTreeMap<_, _> =
        stock.iter().map(|(id, q)| (id.0.to_string(), *q)).collect();
    entries.serialize(serializer)
}
fn roads_wire<S: serde::Serializer>(
    area: &game_land::Area,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Wire<'a> {
        links: std::collections::BTreeMap<String, SimDuration>,
        name: &'a str,
    }
    Wire {
        links: area
            .links
            .iter()
            .map(|(id, dt)| (id.0.to_string(), *dt))
            .collect(),
        name: &area.name,
    }
    .serialize(serializer)
}

impl game_facts::Interpretation for FactValue {
    fn promote(value: Arc<Self>, key: &str) -> Arc<Self> {
        Self::promote(value, key)
    }
}
impl game_memory::Content for FactValue {
    fn cues(&self, key: &str) -> Vec<String> {
        let mut cues = vec![key.split(':').next().unwrap_or(key).to_string()];
        if let Self::Other(v) = self
            && let Some(map) = v.as_object()
        {
            cues.extend(
                map.iter()
                    .filter(|(k, _)| {
                        ["actor", "patient", "place", "@predicate"].contains(&k.as_str())
                    })
                    .filter_map(|(_, v)| v.as_str().map(str::to_string)),
            );
        }
        cues
    }
    fn reconstruct(&self, draw: u64) -> Option<Self> {
        let mut raw = serde_json::to_value(self).ok()?;
        // Never change IDs, actors, sources, predicates, permissions or physical objects.
        let map = raw.as_object_mut()?;
        let names: Vec<_> = map
            .iter()
            .filter(|(k, v)| {
                ["quantity", "amount", "distance", "duration", "intensity"].contains(&k.as_str())
                    && (v.is_u64() || v.as_str().is_some_and(|s| s.parse::<u64>().is_ok()))
            })
            .map(|(k, _)| k.clone())
            .collect();
        if names.is_empty() {
            return None;
        }
        let key = &names[draw as usize % names.len()];
        let string = map[key].is_string();
        let n = map[key]
            .as_u64()
            .or_else(|| map[key].as_str()?.parse().ok())?;
        let step = (n / 100).max(1);
        let changed = if draw & 1 == 0 {
            n.saturating_add(step)
        } else {
            n.saturating_sub(step)
        };
        map.insert(
            key.clone(),
            if string {
                changed.to_string().into()
            } else {
                changed.into()
            },
        );
        Some(Self::Other(raw))
    }
}

use super::*;
/// Cache immutable payloads only while sources match; personal evidence stays on each fact.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(untagged)]
pub enum FactValue {
    Roads(#[serde(serialize_with = "roads_wire")] Arc<game_land::Area>),
    Patch(game_ecology::Patch),
    Stock(#[serde(serialize_with = "stock_wire")] Table<GoodId, u64>),
    Location(StoreLocation),
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
        serde_json::Value::deserialize(d).map(Self::Other)
    }
}
impl FactValue {
    pub fn decode<T: serde::de::DeserializeOwned + Clone + 'static>(&self) -> Option<T> {
        let (typed, json): (&dyn std::any::Any, _) = match self {
            Self::Roads(v) => (v.as_ref(), None),
            Self::Patch(v) => (v, None),
            Self::Stock(v) => (v, None),
            Self::Location(v) => (v, None),
            Self::Other(v) => (v, Some(v)),
        };
        game_facts::decode(typed, self, json)
    }
    pub(super) fn promote(value: Arc<Self>, key: &str) -> Arc<Self> {
        let Self::Other(raw) = value.as_ref() else {
            return value;
        };
        let typed = if key.starts_with("roads:") {
            serde_json::from_value(raw.clone()).map(Self::Roads)
        } else if key.starts_with("store-location:") {
            serde_json::from_value(raw.clone()).map(Self::Location)
        } else if key.starts_with("store:") {
            serde_json::from_value(raw.clone()).map(Self::Stock)
        } else if key.starts_with("patch:") {
            serde_json::from_value(raw.clone()).map(Self::Patch)
        } else {
            return value;
        };
        typed
            .ok()
            .filter(|typed| serde_json::to_value(typed).ok().as_ref() == Some(raw))
            .map(Arc::new)
            .unwrap_or(value)
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

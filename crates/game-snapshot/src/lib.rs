//! Streaming keyed resources and shared evidence without retaining duplicate expanded values.
pub mod evidence;
mod manifest;
pub use manifest::Manifest;
use serde::de::{Deserialize, DeserializeOwned, DeserializeSeed, Error, MapAccess, Visitor};
use serde_json::value::RawValue;
use std::{
    borrow::Borrow,
    collections::BTreeMap,
    fmt::{self, Display},
    io::Read,
};

pub type Decoder<V, E> = fn(&str) -> Result<V, E>;
pub struct Decoded<M, V> {
    pub metadata: M,
    pub resources: BTreeMap<String, V>,
}
pub fn to_string(
    write: impl FnOnce(&mut dyn std::io::Write) -> game_core::Result<()>,
) -> game_core::Result<String> {
    let mut bytes = Vec::new();
    write(&mut bytes)?;
    Ok(String::from_utf8(bytes).expect("JSON is UTF-8"))
}
/// Stream the manifest and encoded resources without materializing a combined tree.
pub fn write_with_resources<M: serde::Serialize>(
    out: &mut dyn std::io::Write,
    metadata: &M,
    resources: impl FnOnce(&mut dyn std::io::Write) -> game_core::Result<()>,
) -> game_core::Result<()> {
    let metadata = serde_json::to_string(metadata)
        .map_err(|e| game_core::Error::InvalidSave(e.to_string()))?;
    let (prefix, suffix) = metadata
        .split_once("\"resources\":{}")
        .expect("resource slot");
    let io = |e: std::io::Error| game_core::Error::InvalidSave(e.to_string());
    out.write_all(prefix.as_bytes()).map_err(io)?;
    out.write_all(b"\"resources\":{").map_err(io)?;
    resources(out)?;
    out.write_all(suffix.as_bytes()).map_err(io)
}

pub fn from_str<M: DeserializeOwned, V, E: Display>(
    source: &str,
    decoders: impl IntoIterator<Item = (&'static str, Decoder<V, E>)>,
) -> Result<Decoded<M, V>, serde_json::Error> {
    let codecs = decoders.into_iter().collect();
    let mut deserializer = serde_json::Deserializer::from_str(source);
    let decoded = Root::<M, V, E, &RawValue>(&codecs, std::marker::PhantomData)
        .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(decoded)
}
pub fn from_reader<M: DeserializeOwned, V, E: Display>(
    source: impl Read,
    decoders: impl IntoIterator<Item = (&'static str, Decoder<V, E>)>,
) -> Result<Decoded<M, V>, serde_json::Error> {
    let codecs = decoders.into_iter().collect();
    let mut deserializer = serde_json::Deserializer::from_reader(source);
    let decoded = Root::<M, V, E, Box<RawValue>>(&codecs, std::marker::PhantomData)
        .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(decoded)
}

type Codecs<V, E> = BTreeMap<&'static str, Decoder<V, E>>;
struct Root<'a, M, V, E, R>(&'a Codecs<V, E>, std::marker::PhantomData<(M, R)>);
impl<'de, M: DeserializeOwned, V, E: Display, R: Deserialize<'de> + Borrow<RawValue>>
    DeserializeSeed<'de> for Root<'_, M, V, E, R>
{
    type Value = Decoded<M, V>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de, M: DeserializeOwned, V, E: Display, R: Deserialize<'de> + Borrow<RawValue>> Visitor<'de>
    for Root<'_, M, V, E, R>
{
    type Value = Decoded<M, V>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("snapshot metadata and resources")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut metadata = BTreeMap::<String, Box<RawValue>>::new();
        let mut resources = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "resources" {
                if resources.is_some() {
                    return Err(A::Error::duplicate_field("resources"));
                }
                resources = Some(
                    map.next_value_seed(Resources::<V, E, R>(self.0, std::marker::PhantomData))?,
                );
            } else if metadata.insert(key, map.next_value()?).is_some() {
                return Err(A::Error::custom("duplicate metadata field"));
            }
        }
        let resources = resources.ok_or_else(|| A::Error::missing_field("resources"))?;
        let json = serde_json::to_string(&metadata).map_err(A::Error::custom)?;
        let metadata = serde_json::from_str(&json).map_err(A::Error::custom)?;
        Ok(Decoded {
            metadata,
            resources,
        })
    }
}
struct Resources<'a, V, E, R>(&'a Codecs<V, E>, std::marker::PhantomData<R>);
impl<'de, V, E: Display, R: Deserialize<'de> + Borrow<RawValue>> DeserializeSeed<'de>
    for Resources<'_, V, E, R>
{
    type Value = BTreeMap<String, V>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_map(self)
    }
}
impl<'de, V, E: Display, R: Deserialize<'de> + Borrow<RawValue>> Visitor<'de>
    for Resources<'_, V, E, R>
{
    type Value = BTreeMap<String, V>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("unique registered resources")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut resources = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let decoder = self
                .0
                .get(key.as_str())
                .ok_or_else(|| A::Error::custom(format!("unknown resource {key}")))?;
            let raw: R = map.next_value()?;
            let value = decoder(raw.borrow().get()).map_err(A::Error::custom)?;
            if resources.insert(key, value).is_some() {
                return Err(A::Error::custom("duplicate resource"));
            }
        }
        Ok(resources)
    }
}

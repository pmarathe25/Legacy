use super::*;
use serde::{
    Deserialize, Serialize,
    de::{Error, MapAccess, Visitor},
    ser::SerializeMap,
};
use std::{fmt, marker::PhantomData};
impl<K: Key + Serialize, V: Clone + Serialize> Serialize for Packed<K, V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}
impl<'de, K: Key + Deserialize<'de>, V: Clone + Deserialize<'de>> Deserialize<'de>
    for Packed<K, V>
{
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Rows<K, V>(PhantomData<(K, V)>);
        impl<'de, K: Key + Deserialize<'de>, V: Clone + Deserialize<'de>> Visitor<'de> for Rows<K, V> {
            type Value = Packed<K, V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("unique packed rows")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut rows = Packed::new();
                while let Some((k, v)) = map.next_entry()? {
                    if rows.insert(k, v).is_some() {
                        return Err(A::Error::custom("duplicate packed key"));
                    }
                }
                Ok(rows)
            }
        }
        d.deserialize_map(Rows(PhantomData))
    }
}

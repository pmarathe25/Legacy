use crate::*;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error, MapAccess, Visitor},
    ser::SerializeMap,
};
impl<K: Ord + Serialize, V: Serialize> Serialize for Table<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
impl<'de, K: Ord + Clone + Deserialize<'de>, V: Clone + Deserialize<'de>> Deserialize<'de>
    for Table<K, V>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Map<K, V>(std::marker::PhantomData<(K, V)>);
        impl<'de, K: Ord + Clone + Deserialize<'de>, V: Clone + Deserialize<'de>> Visitor<'de>
            for Map<K, V>
        {
            type Value = Table<K, V>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map of unique stable keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut table = Table::new();
                while let Some((k, v)) = access.next_entry()? {
                    if table.insert(k, v).is_some() {
                        return Err(A::Error::custom("duplicate table key"));
                    }
                }
                Ok(table)
            }
        }
        deserializer.deserialize_map(Map(std::marker::PhantomData))
    }
}

//! Stream an evidence archive and owner-defined metadata without expanding the complete save.
use game_core::PersonId;
use game_facts::{Fact, Facts, Interpretation};
use game_storage::Table;
use serde::{
    Deserialize,
    de::{DeserializeOwned, Error, MapAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub struct Parts<V, M> {
    pub facts: Table<PersonId, Facts<V>>,
    pub metadata: Option<M>,
}
impl<'de, V: Interpretation, M: DeserializeOwned> Deserialize<'de> for Parts<V, M> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Saved<V, M>(std::marker::PhantomData<(V, M)>);
        impl<'de, V: Interpretation, M: DeserializeOwned> Visitor<'de> for Saved<V, M> {
            type Value = Parts<V, M>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("evidence archive and optional owner metadata")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut facts = Table::new();
                let mut metadata = None;
                let mut seen = BTreeSet::new();
                let mut archive = false;
                while let Some(key) = map.next_key::<String>()? {
                    if !seen.insert(key.clone()) {
                        return Err(A::Error::custom("duplicate evidence field"));
                    }
                    if key == "cognition" {
                        metadata = Some(map.next_value()?);
                    } else if key == "archive" {
                        if !facts.is_empty() {
                            return Err(A::Error::custom("mixed evidence formats"));
                        }
                        facts = map
                            .next_value::<super::Archive<V>>()?
                            .restore(|k, v| V::promote(v, k))
                            .map_err(A::Error::custom)?
                            .into_iter()
                            .collect();
                        archive = true;
                    } else {
                        if archive {
                            return Err(A::Error::custom("mixed evidence formats"));
                        }
                        let p = PersonId(key.parse().map_err(A::Error::custom)?);
                        let mut row = Facts::default();
                        for (k, mut f) in map.next_value::<BTreeMap<Arc<str>, Fact<V>>>()? {
                            f.value = V::promote(f.value, &k);
                            row.insert(k, f);
                        }
                        if facts.insert(p, row).is_some() {
                            return Err(A::Error::custom("duplicate person"));
                        }
                    }
                }
                Ok(Parts { facts, metadata })
            }
        }
        d.deserialize_map(Saved::<V, M>(std::marker::PhantomData))
    }
}

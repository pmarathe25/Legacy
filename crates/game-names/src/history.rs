use super::*;
use serde::{Deserializer, Serializer, ser::SerializeSeq};
use std::sync::Arc;
/// Persistent append history shares old paths and preserves the legacy pair array.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes(Table<u64, Arc<(SimTime, String)>>);
impl Changes {
    pub fn iter(&self) -> impl Iterator<Item = &(SimTime, String)> {
        self.0.values().map(Arc::as_ref)
    }
    pub fn last(&self) -> Option<&(SimTime, String)> {
        self.0
            .len()
            .checked_sub(1)
            .and_then(|i| self.0.get(&(i as u64)))
            .map(Arc::as_ref)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn push(&mut self, entry: (SimTime, String)) {
        self.0.insert(self.len() as u64, Arc::new(entry));
    }
    pub(super) fn valid_since(&self, previous: Option<&Self>) -> bool {
        let previous = previous.filter(|old| {
            self.len() >= old.len()
                && self.0.changed_pairs(&old.0).all(|(id, value, was)| {
                    *id >= old.len() as u64 || was.is_some_and(|was| Arc::ptr_eq(was, value))
                })
        });
        match previous
            .and_then(|old| old.len().checked_sub(1))
            .map(|i| i as u64)
        {
            Some(last) => self
                .0
                .scan_after(Some(&last))
                .all(|(_, entry)| valid(&entry.1)),
            None => self.iter().all(|(_, name)| valid(name)),
        }
    }
}
impl FromIterator<(SimTime, String)> for Changes {
    fn from_iter<T: IntoIterator<Item = (SimTime, String)>>(iter: T) -> Self {
        Self(
            iter.into_iter()
                .enumerate()
                .map(|(i, v)| (i as u64, Arc::new(v)))
                .collect(),
        )
    }
}
impl Serialize for Changes {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.len()))?;
        for entry in self.iter() {
            seq.serialize_element(entry)?;
        }
        seq.end()
    }
}
impl<'de> Deserialize<'de> for Changes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Changes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of chronological name entries")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Changes, A::Error> {
                let mut result = Changes::default();
                while let Some(entry) = seq.next_element()? {
                    result.push(entry);
                }
                Ok(result)
            }
        }
        d.deserialize_seq(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_name_history_preserves_wire_format_and_revalidates_replaced_prefixes() {
        let original: Changes = [(SimTime(1), "First".to_owned())].into_iter().collect();
        let mut extended = original.clone();
        extended.push((SimTime(2), "Second".into()));
        assert_eq!(original.len(), 1);
        assert!(extended.valid_since(Some(&original)));
        assert_eq!(
            serde_json::to_value(&extended).unwrap(),
            serde_json::json!([[1, "First"], [2, "Second"]])
        );
        let loaded: Changes =
            serde_json::from_value(serde_json::json!([[1, "First"], [2, "Second"]])).unwrap();
        assert_eq!(loaded, extended);
        assert!(loaded.valid_since(Some(&original)));
        let invalid: Changes = [(SimTime(1), "\n".to_owned()), (SimTime(2), "Second".into())]
            .into_iter()
            .collect();
        assert!(!invalid.valid_since(Some(&original)));
        extended.push((SimTime(3), "\u{0}".into()));
        assert!(!extended.valid_since(Some(&original)));
    }
}

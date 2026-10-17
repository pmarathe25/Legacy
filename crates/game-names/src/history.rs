use super::*;
use std::sync::Arc;
/// Immutable entries preserve complete name histories without copying old strings on append.
/// The wire representation remains the ordinary array of (time, name) pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Changes(Vec<Arc<(SimTime, String)>>);
impl Changes {
    pub fn iter(&self) -> impl Iterator<Item = &(SimTime, String)> {
        self.0.iter().map(Arc::as_ref)
    }
    pub fn last(&self) -> Option<&(SimTime, String)> {
        self.0.last().map(Arc::as_ref)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn push(&mut self, entry: (SimTime, String)) {
        self.0.push(Arc::new(entry));
    }
    pub(super) fn valid_since(&self, previous: Option<&Self>) -> bool {
        let start = previous
            .filter(|old| {
                self.len() >= old.len() && self.0.iter().zip(&old.0).all(|(a, b)| Arc::ptr_eq(a, b))
            })
            .map_or(0, Self::len);
        self.0[start..].iter().all(|entry| valid(&entry.1))
    }
}
impl FromIterator<(SimTime, String)> for Changes {
    fn from_iter<T: IntoIterator<Item = (SimTime, String)>>(iter: T) -> Self {
        Self(iter.into_iter().map(Arc::new).collect())
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

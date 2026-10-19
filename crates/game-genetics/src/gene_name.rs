//! Immutable locus names share their storage across distinct genomes.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    borrow::Borrow,
    cell::RefCell,
    collections::HashMap,
    fmt,
    ops::Deref,
    sync::{Arc, Weak},
};
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneName(Arc<String>);
thread_local! { static NAMES: RefCell<HashMap<String, Weak<String>>> = RefCell::default(); }
impl From<&str> for GeneName {
    fn from(name: &str) -> Self {
        if name.len() > 64 {
            return Self(Arc::new(name.to_owned()));
        }
        NAMES.with(|slot| {
            let mut names = slot.borrow_mut();
            if let Some(value) = names.get(name).and_then(Weak::upgrade) {
                return Self(value);
            }
            if names.len() >= 256 {
                names.retain(|_, value| value.strong_count() > 0);
                if names.len() >= 256 {
                    names.clear();
                }
            }
            let value = Arc::new(name.to_owned());
            names.insert(name.to_owned(), Arc::downgrade(&value));
            Self(value)
        })
    }
}
impl From<String> for GeneName {
    fn from(name: String) -> Self {
        Self::from(name.as_str())
    }
}
impl GeneName {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl Deref for GeneName {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl Borrow<str> for GeneName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl Borrow<String> for GeneName {
    fn borrow(&self) -> &String {
        &self.0
    }
}
impl fmt::Debug for GeneName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl fmt::Display for GeneName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}
impl Serialize for GeneName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for GeneName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Name;
        impl de::Visitor<'_> for Name {
            type Value = GeneName;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a locus name")
            }
            fn visit_str<E: de::Error>(self, name: &str) -> Result<Self::Value, E> {
                Ok(name.into())
            }
            fn visit_borrowed_str<E: de::Error>(self, name: &str) -> Result<Self::Value, E> {
                self.visit_str(name)
            }
        }
        deserializer.deserialize_str(Name)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_share_immutable_storage_and_keep_string_order_and_borrowing() {
        let first = GeneName::from("cold");
        let second = GeneName::from(String::from("cold"));
        assert!(Arc::ptr_eq(&first.0, &second.0));
        let map = std::collections::BTreeMap::from([(first, 1), (GeneName::from("yield"), 2)]);
        assert_eq!(map.get("cold"), Some(&1));
        assert_eq!(map.get(&String::from("yield")), Some(&2));
        assert_eq!(format!("{:?}", second), "\"cold\"");
    }
}

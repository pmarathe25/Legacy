//! Bounded slices of a shared immutable receipt arena.
use crate::Lot;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    ops::{Deref, Range},
    sync::Arc,
};

#[derive(Clone, Debug)]
enum Storage {
    Owned(Vec<Lot>),
    Shared {
        arena: Arc<[Lot]>,
        range: Range<usize>,
    },
}

/// Exact dated cohorts. A batch can share one allocation across many receipts.
#[derive(Clone, Debug)]
pub struct LotList(Storage);
impl LotList {
    pub fn shared(arena: Arc<[Lot]>, range: Range<usize>) -> Self {
        assert!(range.start <= range.end && range.end <= arena.len());
        if range.is_empty() {
            return Self::default();
        }
        Self(Storage::Shared { arena, range })
    }
    pub fn as_slice(&self) -> &[Lot] {
        match &self.0 {
            Storage::Owned(lots) => lots,
            Storage::Shared { arena, range } => &arena[range.clone()],
        }
    }
    pub fn into_vec(self) -> Vec<Lot> {
        match self.0 {
            Storage::Owned(lots) => lots,
            Storage::Shared { arena, range } => arena[range].to_vec(),
        }
    }
}
impl Default for LotList {
    fn default() -> Self {
        Self(Storage::Owned(Vec::new()))
    }
}
impl From<Vec<Lot>> for LotList {
    fn from(lots: Vec<Lot>) -> Self {
        Self(Storage::Owned(lots))
    }
}
impl Deref for LotList {
    type Target = [Lot];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl<'a> IntoIterator for &'a LotList {
    type Item = &'a Lot;
    type IntoIter = std::slice::Iter<'a, Lot>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}
impl PartialEq for LotList {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl Eq for LotList {}
impl PartialEq<Vec<Lot>> for LotList {
    fn eq(&self, other: &Vec<Lot>) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl Serialize for LotList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for LotList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<Lot>::deserialize(deserializer).map(Self::from)
    }
}

//! Exact attribution membership with sparse offsets and dense bitmap pages.
use game_storage::Records;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};
const SHIFT: u32 = 12;
const WORDS: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Page {
    Sparse(Vec<u16>),
    Dense(Box<[u64; WORDS]>),
}
impl Default for Page {
    fn default() -> Self {
        Self::Sparse(Vec::new())
    }
}
impl Page {
    fn insert(&mut self, offset: u16) -> bool {
        match self {
            Self::Sparse(values) => {
                let Err(index) = values.binary_search(&offset) else {
                    return false;
                };
                values.insert(index, offset);
                if values.len() > WORDS * 4 {
                    let mut bits = Box::new([0; WORDS]);
                    for value in values {
                        bits[usize::from(*value / 64)] |= 1 << (*value % 64);
                    }
                    *self = Self::Dense(bits);
                }
                true
            }
            Self::Dense(bits) => {
                let word = &mut bits[usize::from(offset / 64)];
                let mask = 1 << (offset % 64);
                let added = *word & mask == 0;
                *word |= mask;
                added
            }
        }
    }
    fn contains(&self, offset: u16) -> bool {
        match self {
            Self::Sparse(values) => values.binary_search(&offset).is_ok(),
            Self::Dense(bits) => bits[usize::from(offset / 64)] & (1 << (offset % 64)) != 0,
        }
    }
    fn offsets(&self) -> impl Iterator<Item = u16> + '_ {
        let (mut sparse, mut dense) = match self {
            Self::Sparse(values) => (values.as_slice(), [].iter().enumerate()),
            Self::Dense(bits) => (&[][..], bits.iter().enumerate()),
        };
        let (mut base, mut remaining) = (0, 0u64);
        std::iter::from_fn(move || {
            if let Some((first, rest)) = sparse.split_first() {
                sparse = rest;
                return Some(*first);
            }
            loop {
                if remaining != 0 {
                    let offset = remaining.trailing_zeros() as u16;
                    remaining &= remaining - 1;
                    return Some(base + offset);
                }
                let (index, word) = dense.next()?;
                base = index as u16 * 64;
                remaining = *word;
            }
        })
    }
}
/// Serializes as the legacy ordered attribution-to-null map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credits {
    pages: Records<u64, Page>,
    len: usize,
}
impl Credits {
    pub fn contains_key(&self, id: &u64) -> bool {
        self.pages
            .get(&(id >> SHIFT))
            .is_some_and(|p| p.contains((id & 4095) as u16))
    }
    pub fn insert(&mut self, id: u64, _: ()) -> Option<()> {
        if self.contains_key(&id) {
            return Some(());
        }
        self.pages
            .entry(id >> SHIFT)
            .or_default()
            .insert((id & 4095) as u16);
        self.len += 1;
        None
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.pages
            .iter()
            .flat_map(|(page, values)| values.offsets().map(move |v| page << SHIFT | u64::from(v)))
    }
}
impl Serialize for Credits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len))?;
        for id in self.keys() {
            map.serialize_entry(&id, &())?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Credits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Credits;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unique attribution IDs mapped to null")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Credits, A::Error> {
                let mut credits = Credits::default();
                while let Some((id, ())) = map.next_entry::<u64, ()>()? {
                    if credits.insert(id, ()).is_some() {
                        return Err(de::Error::custom("duplicate attribution"));
                    }
                }
                Ok(credits)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

//! Numeric stable IDs permit compact paged slots without allocation by maximum ID.
use rustc_hash::FxHashMap;
use std::{collections::BTreeSet, hash::Hash, ops::Index as Lookup};
/// Numbers must be unique and preserve key order.
pub trait Key: Copy + Ord + Hash {
    fn number(self) -> u64;
}
impl Key for u64 {
    fn number(self) -> u64 {
        self
    }
}
impl Key for u32 {
    fn number(self) -> u64 {
        u64::from(self)
    }
}
impl Key for usize {
    fn number(self) -> u64 {
        self as u64
    }
}
impl Key for game_core::PersonId {
    fn number(self) -> u64 {
        self.0
    }
}
#[cfg(feature = "hash-index")]
#[derive(Clone, Debug)]
pub struct RowIndex<K> {
    map: FxHashMap<K, usize>,
    order: BTreeSet<K>,
}
#[cfg(feature = "hash-index")]
impl<K> Default for RowIndex<K> {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            order: BTreeSet::new(),
        }
    }
}
#[cfg(feature = "hash-index")]
impl<K: Key> RowIndex<K> {
    pub fn get(&self, key: &K) -> Option<&usize> {
        self.map.get(key)
    }
    pub fn insert(&mut self, key: K, slot: usize) {
        self.map.insert(key, slot);
        self.order.insert(key);
    }
    pub fn remove(&mut self, key: &K) {
        self.map.remove(key);
        self.order.remove(key);
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
    pub fn slots(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.order.iter().map(|key| self.map[key])
    }
}
#[cfg(not(feature = "hash-index"))]
mod pages {
    use super::*;
    use std::{collections::btree_set, marker::PhantomData};
    const WIDTH: usize = 256;
    const EMPTY: usize = usize::MAX;
    #[derive(Clone, Debug)]
    enum Page {
        Sparse(Vec<(u8, usize)>),
        Dense(Box<[usize; WIDTH]>, u16),
    }
    impl Default for Page {
        fn default() -> Self {
            Self::Sparse(vec![])
        }
    }
    impl Page {
        fn get(&self, offset: u8) -> Option<&usize> {
            match self {
                Self::Sparse(rows) => rows
                    .binary_search_by_key(&offset, |v| v.0)
                    .ok()
                    .map(|i| &rows[i].1),
                Self::Dense(slots, _) => {
                    let slot = &slots[usize::from(offset)];
                    (*slot != EMPTY).then_some(slot)
                }
            }
        }
        fn insert(&mut self, offset: u8, slot: usize) -> bool {
            match self {
                Self::Sparse(rows) => {
                    match rows.binary_search_by_key(&offset, |v| v.0) {
                        Ok(i) => {
                            rows[i].1 = slot;
                            return false;
                        }
                        Err(i) => rows.insert(i, (offset, slot)),
                    }
                    if rows.len() > 32 {
                        let mut slots = Box::new([EMPTY; WIDTH]);
                        for &(at, value) in rows.iter() {
                            slots[usize::from(at)] = value;
                        }
                        *self = Self::Dense(slots, rows.len() as u16);
                    }
                    true
                }
                Self::Dense(slots, len) => {
                    let at = &mut slots[usize::from(offset)];
                    let added = *at == EMPTY;
                    *at = slot;
                    *len += u16::from(added);
                    added
                }
            }
        }
        fn remove(&mut self, offset: u8) -> bool {
            match self {
                Self::Sparse(rows) => match rows.binary_search_by_key(&offset, |v| v.0) {
                    Ok(i) => {
                        rows.remove(i);
                        true
                    }
                    Err(_) => false,
                },
                Self::Dense(slots, len) => {
                    let at = &mut slots[usize::from(offset)];
                    if *at == EMPTY {
                        return false;
                    }
                    *at = EMPTY;
                    *len -= 1;
                    if *len < 16 {
                        *self = Self::Sparse(
                            slots
                                .iter()
                                .enumerate()
                                .filter(|(_, v)| **v != EMPTY)
                                .map(|(i, v)| (i as u8, *v))
                                .collect(),
                        );
                    }
                    true
                }
            }
        }
        fn empty(&self) -> bool {
            match self {
                Self::Sparse(v) => v.is_empty(),
                Self::Dense(_, len) => *len == 0,
            }
        }
        fn cursor(&self) -> Cursor<'_> {
            match self {
                Self::Sparse(v) => Cursor::Sparse(v.iter()),
                Self::Dense(v, _) => Cursor::Dense(v.iter()),
            }
        }
    }
    enum Cursor<'a> {
        Sparse(std::slice::Iter<'a, (u8, usize)>),
        Dense(std::slice::Iter<'a, usize>),
    }
    impl Iterator for Cursor<'_> {
        type Item = usize;
        fn next(&mut self) -> Option<usize> {
            match self {
                Self::Sparse(v) => v.next().map(|v| v.1),
                Self::Dense(v) => v.find(|v| **v != EMPTY).copied(),
            }
        }
    }
    #[derive(Clone, Debug)]
    pub struct RowIndex<K> {
        pages: FxHashMap<u64, Page>,
        order: BTreeSet<u64>,
        len: usize,
        kind: PhantomData<K>,
    }
    impl<K> Default for RowIndex<K> {
        fn default() -> Self {
            Self {
                pages: FxHashMap::default(),
                order: BTreeSet::new(),
                len: 0,
                kind: PhantomData,
            }
        }
    }
    impl<K: Key> RowIndex<K> {
        pub fn get(&self, key: &K) -> Option<&usize> {
            let id = key.number();
            self.pages.get(&(id >> 8))?.get(id as u8)
        }
        pub fn contains_key(&self, key: &K) -> bool {
            self.get(key).is_some()
        }
        pub fn insert(&mut self, key: K, slot: usize) {
            let id = key.number();
            let page = id >> 8;
            let order = &mut self.order;
            if self
                .pages
                .entry(page)
                .or_insert_with(|| {
                    order.insert(page);
                    Page::default()
                })
                .insert(id as u8, slot)
            {
                self.len += 1;
            }
        }
        pub fn remove(&mut self, key: &K) {
            let id = key.number();
            let page = id >> 8;
            if let Some(p) = self.pages.get_mut(&page) {
                if p.remove(id as u8) {
                    self.len -= 1;
                }
                if p.empty() {
                    self.pages.remove(&page);
                    self.order.remove(&page);
                }
            }
        }
        pub fn slots(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
            Ordered {
                index: self,
                pages: self.order.iter(),
                cursor: None,
                remaining: self.len,
            }
        }
    }
    struct Ordered<'a, K> {
        index: &'a RowIndex<K>,
        pages: btree_set::Iter<'a, u64>,
        cursor: Option<Cursor<'a>>,
        remaining: usize,
    }
    impl<K> Iterator for Ordered<'_, K> {
        type Item = usize;
        fn next(&mut self) -> Option<usize> {
            loop {
                if let Some(cursor) = &mut self.cursor
                    && let Some(slot) = cursor.next()
                {
                    self.remaining -= 1;
                    return Some(slot);
                }
                self.cursor = Some(
                    self.index
                        .pages
                        .get(self.pages.next()?)
                        .expect("indexed page")
                        .cursor(),
                );
            }
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            (self.remaining, Some(self.remaining))
        }
    }
    impl<K> ExactSizeIterator for Ordered<'_, K> {}
}
#[cfg(not(feature = "hash-index"))]
pub use pages::RowIndex;
impl<K: Key> Lookup<&K> for RowIndex<K> {
    type Output = usize;
    fn index(&self, key: &K) -> &usize {
        self.get(key).expect("indexed key")
    }
}

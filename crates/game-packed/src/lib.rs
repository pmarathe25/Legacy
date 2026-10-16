//! Dense stable-ID rows, indexed reads, and nested transactional undo.
mod index;
mod wire;
pub use index::Key;
use index::RowIndex;
#[cfg(feature = "hash-index")]
pub const INDEX_NAME: &str = "hash";
#[cfg(not(feature = "hash-index"))]
pub const INDEX_NAME: &str = "paged";
#[cfg(feature = "compact")]
pub const BUFFER_POLICY: &str = "compact";
#[cfg(not(feature = "compact"))]
pub const BUFFER_POLICY: &str = "reuse";
use rayon::prelude::*;
use rustc_hash::FxHashMap as HashMap;
use std::{ops::Index, sync::Arc};
#[derive(Clone, Debug)]
struct Data<K, V> {
    rows: Vec<(K, V)>,
    index: RowIndex<K>,
}
impl<K, V> Default for Data<K, V> {
    fn default() -> Self {
        Self {
            rows: vec![],
            index: RowIndex::default(),
        }
    }
}
#[derive(Debug)]
enum Undo<K, V> {
    Row(K, V, Option<u64>),
    Added(K, Option<u64>),
    Removed(usize, K, V, Option<u64>),
    Bulk(Vec<(K, V)>),
}
#[derive(Debug)]
pub struct Mark {
    offset: usize,
    token: u64,
    parent: u64,
    bulk: u64,
}
#[derive(Debug)]
pub struct Packed<K, V> {
    data: Arc<Data<K, V>>,
    undo: Vec<Undo<K, V>>,
    touched: HashMap<K, u64>,
    spare: Vec<(K, V)>,
    sequence: u64,
    current: u64,
    bulk: u64,
}
impl<K, V> Default for Packed<K, V> {
    fn default() -> Self {
        Self {
            data: Arc::new(Data::default()),
            undo: vec![],
            touched: HashMap::default(),
            spare: vec![],
            sequence: 0,
            current: 0,
            bulk: 0,
        }
    }
}
impl<K, V> Clone for Packed<K, V> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            undo: vec![],
            touched: HashMap::default(),
            spare: vec![],
            sequence: 0,
            current: 0,
            bulk: 0,
        }
    }
}
impl<K: Key, V: Clone> Packed<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.data.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.rows.is_empty()
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.data.index.get(key).map(|i| &self.data.rows[*i].1)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.data.index.contains_key(key)
    }
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> {
        self.iter().map(|(key, _)| key)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        self.data.index.slots().map(|slot| {
            let row = &self.data.rows[slot];
            (&row.0, &row.1)
        })
    }
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
    /// Physical order is never an arbitration order or a game identity.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = (&K, &V)> {
        self.data.rows.iter().map(|(key, value)| (key, value))
    }
    pub fn begin(&mut self) -> Mark {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("journal epoch exhausted");
        let mark = Mark {
            offset: self.undo.len(),
            token: self.sequence,
            parent: self.current,
            bulk: self.bulk,
        };
        self.current = mark.token;
        mark
    }
    fn touch(&mut self, key: K) {
        if self.current == 0
            || self.bulk == self.current
            || self.touched.get(&key) == Some(&self.current)
        {
            return;
        }
        let old = self.get(&key).expect("edited row").clone();
        let previous = self.touched.insert(key, self.current);
        self.undo.push(Undo::Row(key, old, previous));
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let i = *self.data.index.get(key)?;
        self.touch(*key);
        Some(&mut Arc::make_mut(&mut self.data).rows[i].1)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.contains_key(&key) {
            return self.get_mut(&key).map(|old| std::mem::replace(old, value));
        }
        if self.current != 0 {
            let old = self.touched.insert(key, self.current);
            self.undo.push(Undo::Added(key, old));
        }
        let data = Arc::make_mut(&mut self.data);
        data.index.insert(key, data.rows.len());
        #[cfg(feature = "compact")]
        if data.rows.len() == data.rows.capacity() {
            data.rows.reserve_exact((data.rows.len() / 8).max(4));
        }
        data.rows.push((key, value));
        None
    }
    fn remove_raw(&mut self, key: &K) -> Option<(usize, V)> {
        let i = *self.data.index.get(key)?;
        let data = Arc::make_mut(&mut self.data);
        data.index.remove(key);
        let (_, value) = data.rows.swap_remove(i);
        if i < data.rows.len() {
            data.index.insert(data.rows[i].0, i);
        }
        Some((i, value))
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (slot, value) = self.remove_raw(key)?;
        let previous = self.touched.remove(key);
        if self.current != 0 {
            self.undo
                .push(Undo::Removed(slot, *key, value.clone(), previous));
        }
        Some(value)
    }
    pub fn erase(&mut self, key: &K) -> bool {
        let Some((slot, value)) = self.remove_raw(key) else {
            return false;
        };
        let previous = self.touched.remove(key);
        if self.current != 0 {
            self.undo.push(Undo::Removed(slot, *key, value, previous));
        }
        true
    }
    fn restore_touch(&mut self, key: K, previous: Option<u64>) {
        match previous {
            Some(token) => {
                self.touched.insert(key, token);
            }
            None => {
                self.touched.remove(&key);
            }
        }
    }
    fn recycle(&mut self, mut rows: Vec<(K, V)>) {
        if cfg!(feature = "compact") {
            return;
        }
        rows.clear();
        if rows.capacity() > self.spare.capacity() {
            self.spare = rows;
        }
    }
    pub fn rollback(&mut self, mark: Mark) {
        assert_eq!(self.current, mark.token, "journal savepoint order");
        while self.undo.len() > mark.offset {
            match self.undo.pop().unwrap() {
                Undo::Row(key, value, previous) => {
                    let data = Arc::make_mut(&mut self.data);
                    data.rows[data.index[&key]].1 = value;
                    self.restore_touch(key, previous);
                }
                Undo::Added(key, previous) => {
                    self.remove_raw(&key).expect("added row");
                    self.restore_touch(key, previous);
                }
                Undo::Removed(slot, key, value, previous) => {
                    let data = Arc::make_mut(&mut self.data);
                    let last = data.rows.len();
                    data.rows.push((key, value));
                    data.rows.swap(slot, last);
                    data.index.insert(key, slot);
                    if slot < last {
                        data.index.insert(data.rows[last].0, last);
                    }
                    self.restore_touch(key, previous);
                }
                Undo::Bulk(rows) => {
                    let discarded =
                        std::mem::replace(&mut Arc::make_mut(&mut self.data).rows, rows);
                    self.recycle(discarded);
                }
            }
        }
        self.current = mark.parent;
        self.bulk = mark.bulk;
        if mark.parent == 0 {
            self.sequence = 0;
        }
    }
    pub fn commit(&mut self, mark: Mark) {
        assert_eq!(self.current, mark.token, "journal savepoint order");
        if mark.parent == 0 {
            while let Some(undo) = self.undo.pop() {
                if let Undo::Bulk(rows) = undo {
                    self.recycle(rows);
                }
            }
            self.touched.clear();
            self.bulk = 0;
            self.sequence = 0;
        } else {
            for undo in &self.undo[mark.offset..] {
                match undo {
                    Undo::Row(key, ..) | Undo::Added(key, ..) | Undo::Removed(_, key, ..) => {
                        self.touched.insert(*key, mark.parent);
                    }
                    Undo::Bulk(_) => {}
                }
            }
            if self.bulk == mark.token {
                self.bulk = mark.parent;
            }
        }
        self.current = mark.parent;
    }
    pub fn changed_since<'a>(&'a self, mark: &Mark) -> impl Iterator<Item = (&'a K, &'a V)> {
        let changes = &self.undo[mark.offset..];
        let bulk = changes.iter().any(|u| matches!(u, Undo::Bulk(_)));
        self.rows()
            .take(if bulk { self.len() } else { 0 })
            .chain(changes.iter().filter_map(move |u| {
                if bulk {
                    return None;
                }
                let key = match u {
                    Undo::Row(key, ..) | Undo::Added(key, ..) | Undo::Removed(_, key, ..) => key,
                    Undo::Bulk(_) => return None,
                };
                self.get(key).map(|value| (key, value))
            }))
    }
    pub fn changed_pairs_since<'a>(
        &'a self,
        mark: &Mark,
    ) -> impl Iterator<Item = (&'a K, &'a V, Option<&'a V>)> {
        self.undo[mark.offset..].iter().flat_map(move |undo| {
            let one = match undo {
                Undo::Row(key, old, _) | Undo::Removed(_, key, old, _) => {
                    self.get(key).map(|v| (key, v, Some(old)))
                }
                Undo::Added(key, _) => self.get(key).map(|v| (key, v, None)),
                Undo::Bulk(_) => None,
            };
            let bulk = match undo {
                Undo::Bulk(rows) => rows.as_slice(),
                _ => &[],
            };
            one.into_iter().chain(
                bulk.iter()
                    .filter_map(move |(key, old)| self.get(key).map(|v| (key, v, Some(old)))),
            )
        })
    }
    pub fn update(&mut self, pool: &rayon::ThreadPool, f: impl Fn(&K, &mut V) + Send + Sync)
    where
        K: Send + Sync,
        V: Send + Sync,
    {
        if self.current != 0 && self.bulk != self.current {
            let mut next = std::mem::take(&mut self.spare);
            next.clone_from(&self.data.rows);
            let old = std::mem::replace(&mut Arc::make_mut(&mut self.data).rows, next);
            self.undo.push(Undo::Bulk(old));
            self.bulk = self.current;
        }
        let rows = &mut Arc::make_mut(&mut self.data).rows;
        if rows.len() < 4096 || pool.current_num_threads() == 1 {
            for (k, v) in rows {
                f(k, v);
            }
        } else {
            pool.install(|| rows.par_iter_mut().for_each(|(k, v)| f(k, v)));
        }
    }
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        Entry { store: self, key }
    }
    pub fn modify<R>(
        &mut self,
        key: K,
        default: impl FnOnce() -> V,
        f: impl FnOnce(&mut V) -> R,
    ) -> R {
        if !self.contains_key(&key) {
            self.insert(key, default());
        }
        f(self.get_mut(&key).unwrap())
    }
}
pub struct Entry<'a, K, V> {
    store: &'a mut Packed<K, V>,
    key: K,
}
impl<'a, K: Key, V: Clone> Entry<'a, K, V> {
    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        if !self.store.contains_key(&self.key) {
            self.store.insert(self.key, f());
        }
        self.store.get_mut(&self.key).unwrap()
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}
impl<K: Key, V: Clone> Index<&K> for Packed<K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        self.get(key).expect("missing packed row")
    }
}
impl<K: Key, V: Clone> FromIterator<(K, V)> for Packed<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(rows: T) -> Self {
        let mut s = Self::new();
        for (k, v) in rows {
            s.insert(k, v);
        }
        s
    }
}
impl<K: Key, V: Clone, const N: usize> From<[(K, V); N]> for Packed<K, V> {
    fn from(rows: [(K, V); N]) -> Self {
        rows.into_iter().collect()
    }
}
impl<K: Key, V: Clone + PartialEq> PartialEq for Packed<K, V> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
            || (self.len() == other.len() && self.iter().eq(other.iter()))
    }
}
impl<K: Key, V: Clone + Eq> Eq for Packed<K, V> {}

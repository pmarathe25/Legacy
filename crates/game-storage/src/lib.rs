//! Ordered, persistent tables with dense leaves and bounded copy-on-write paths.
mod merge;
pub use merge::merge_unique;
mod version;
pub use version::Version;
mod agenda;
pub use agenda::Agenda;
mod iteration;
mod records;
use iteration::Cursor;
pub use iteration::Iter;
mod serde_impl;
use rayon::prelude::*;
pub use records::Records;
use std::{
    borrow::Borrow,
    ops::{Bound, Index},
    sync::Arc,
};
const ROWS: usize = 64;
const BRANCHES: usize = 32;
#[derive(Clone, Debug)]
enum Node<K, V> {
    Leaf(Vec<(K, V)>),
    Branch(Vec<(K, Arc<Node<K, V>>)>),
}
#[derive(Clone, Debug)]
pub struct Table<K, V> {
    root: Option<Arc<Node<K, V>>>,
    len: usize,
}
impl<K, V> Default for Table<K, V> {
    fn default() -> Self {
        Self { root: None, len: 0 }
    }
}
impl<K: Ord, V> Table<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    #[inline]
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.root.as_deref()?.get(key)
    }
    #[inline]
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.get(key).is_some()
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            cursor: Cursor::new(self.root.as_deref()),
            remaining: self.len,
        }
    }
    pub fn range<'a, Q: Ord + ?Sized + 'a>(
        &'a self,
        bounds: (Bound<&'a Q>, Bound<&'a Q>),
    ) -> impl Iterator<Item = (&'a K, &'a V)>
    where
        K: Borrow<Q>,
    {
        let cursor = Cursor::from_bound(self.root.as_deref(), bounds.0);
        cursor.take_while(move |(k, _)| match bounds.1 {
            Bound::Included(end) => (*k).borrow() <= end,
            Bound::Excluded(end) => (*k).borrow() < end,
            Bound::Unbounded => true,
        })
    }
    pub fn scan_after<'a>(&'a self, key: Option<&'a K>) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.range((
            key.map_or(Bound::Unbounded, Bound::Excluded),
            Bound::Unbounded,
        ))
    }
    /// Visits only leaves on changed paths. Structural changes are safe: values
    /// from a changed leaf may be reported conservatively, never omitted.
    pub fn changed<'a>(&'a self, previous: &'a Self) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.changed_pairs(previous).map(|(k, v, _)| (k, v))
    }
    /// Pair changed rows with their previous values. Aligned leaves merge in
    /// order, avoiding a second tree search per row during owner validation.
    pub fn changed_pairs<'a>(
        &'a self,
        previous: &'a Self,
    ) -> impl Iterator<Item = (&'a K, &'a V, Option<&'a V>)> {
        let mut stack = Vec::new();
        let mut next = self.root.as_deref().filter(|_| !self.ptr_eq(previous));
        let mut rows = [].iter();
        let mut old_rows = None;
        std::iter::from_fn(move || {
            loop {
                if let Some((k, v)) = rows.next() {
                    let old = if let Some(rows) = &mut old_rows {
                        let rows: &mut std::iter::Peekable<std::slice::Iter<'a, (K, V)>> = rows;
                        while rows.peek().is_some_and(|(key, _)| key < k) {
                            rows.next();
                        }
                        rows.peek().filter(|(key, _)| key == k).map(|(_, v)| v)
                    } else {
                        previous.get(k)
                    };
                    return Some((k, v, old));
                }
                let node = next.take().or_else(|| stack.pop())?;
                if node.size() == 0 {
                    continue;
                }
                let old = previous
                    .root
                    .as_deref()
                    .and_then(|old| old.at_height(node.last(), node.height()));
                if old.is_some_and(|old| std::ptr::eq(old, node)) {
                    continue;
                }
                match node {
                    Node::Leaf(leaf) => {
                        rows = leaf.iter();
                        old_rows = match old {
                            Some(Node::Leaf(old))
                                if !old.is_empty()
                                    && old[0].0 <= leaf[0].0
                                    && old.last().unwrap().0 >= leaf.last().unwrap().0 =>
                            {
                                Some(old.iter().peekable())
                            }
                            _ => None,
                        };
                    }
                    Node::Branch(children) => {
                        stack.extend(children.iter().rev().map(|(_, n)| n.as_ref()))
                    }
                }
            }
        })
    }
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl ExactSizeIterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }
}
impl<K: Ord + Clone, V: Clone> Table<K, V> {
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        // Search each level once; an absent key never copies a leaf.
        Node::get_mut(self.root.as_mut()?, key)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let Some(root) = self.root.as_mut() else {
            self.root = Some(Arc::new(Node::Leaf(vec![(key, value)])));
            self.len = 1;
            return None;
        };
        let (old, split) = Arc::make_mut(root).insert(key, value);
        if old.is_none() {
            self.len += 1;
        }
        if let Some(right) = split {
            self.root = Some(Arc::new(Node::Branch(vec![
                (root.last().clone(), root.clone()),
                (right.last().clone(), right),
            ])));
        }
        old
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.get(key)?;
        let root = self.root.as_mut().expect("present key");
        let old = Arc::make_mut(root).remove(key);
        self.len -= 1;
        while let Node::Branch(children) = root.as_ref() {
            if children.len() != 1 {
                break;
            }
            *root = children[0].1.clone();
        }
        if self.len == 0 {
            self.root = None;
        }
        old
    }
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        Entry { table: self, key }
    }
    /// Update an existing value with one search. Construct a default only when absent.
    pub fn modify<R>(
        &mut self,
        key: K,
        default: impl FnOnce() -> V,
        f: impl FnOnce(&mut V) -> R,
    ) -> R {
        if let Some(value) = self.get_mut(&key) {
            return f(value);
        }
        self.insert(key.clone(), default());
        f(self.get_mut(&key).expect("inserted"))
    }
    /// Evaluates disjoint leaves with the caller's bounded worker pool. The caller
    /// owns this staged table; a panic cannot mutate any shared committed leaf.
    pub fn update(&mut self, pool: &rayon::ThreadPool, f: impl Fn(&K, &mut V) + Send + Sync)
    where
        K: Send + Sync,
        V: Send + Sync,
    {
        let Some(root) = self.root.as_mut() else {
            return;
        };
        if self.len < 4096 || pool.current_num_threads() == 1 {
            Arc::make_mut(root).update(&f, false);
        } else {
            pool.install(|| Arc::make_mut(root).update(&f, true));
        }
    }
}
impl<K: Ord, V> Node<K, V> {
    fn height(&self) -> usize {
        match self {
            Self::Leaf(_) => 0,
            Self::Branch(c) => 1 + c[0].1.height(),
        }
    }
    fn at_height(&self, key: &K, height: usize) -> Option<&Self> {
        let mut node = self;
        let mut current = self.height();
        if current < height {
            return None;
        }
        while current > height {
            match node {
                Self::Branch(c) => node = &c[Self::child(c, key)].1,
                _ => return None,
            }
            current -= 1;
        }
        Some(node)
    }
    fn last(&self) -> &K {
        match self {
            Self::Leaf(rows) => &rows.last().expect("nonempty leaf").0,
            Self::Branch(children) => &children.last().expect("nonempty branch").0,
        }
    }
    fn size(&self) -> usize {
        match self {
            Self::Leaf(rows) => rows.len(),
            Self::Branch(children) => children.len(),
        }
    }
    fn child<Q: Ord + ?Sized>(children: &[(K, Arc<Self>)], key: &Q) -> usize
    where
        K: Borrow<Q>,
    {
        children
            .partition_point(|(max, _)| max.borrow() < key)
            .min(children.len() - 1)
    }
    #[inline]
    fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        let mut node = self;
        loop {
            match node {
                Self::Leaf(rows) => {
                    return rows
                        .binary_search_by(|(k, _)| k.borrow().cmp(key))
                        .ok()
                        .map(|i| &rows[i].1);
                }
                Self::Branch(children) => node = &children[Self::child(children, key)].1,
            }
        }
    }
}
impl<K: Ord + Clone, V: Clone> Node<K, V> {
    fn get_mut<'a, Q: Ord + ?Sized>(node: &'a mut Arc<Self>, key: &Q) -> Option<&'a mut V>
    where
        K: Borrow<Q>,
    {
        let i = match node.as_ref() {
            Self::Leaf(rows) => rows.binary_search_by(|(k, _)| k.borrow().cmp(key)).ok()?,
            Self::Branch(children) => Self::child(children, key),
        };
        match Arc::make_mut(node) {
            Self::Leaf(rows) => Some(&mut rows[i].1),
            Self::Branch(children) => Self::get_mut(&mut children[i].1, key),
        }
    }
    fn insert(&mut self, key: K, value: V) -> (Option<V>, Option<Arc<Self>>) {
        let old = match self {
            Self::Leaf(rows) => match rows.binary_search_by(|(k, _)| k.cmp(&key)) {
                Ok(i) => return (Some(std::mem::replace(&mut rows[i].1, value)), None),
                Err(i) => {
                    rows.insert(i, (key, value));
                    None
                }
            },
            Self::Branch(children) => {
                let i = Self::child(children, &key);
                let (old, split) = Arc::make_mut(&mut children[i].1).insert(key, value);
                children[i].0 = children[i].1.last().clone();
                if let Some(right) = split {
                    children.insert(i + 1, (right.last().clone(), right));
                }
                old
            }
        };
        let split = match self {
            Self::Leaf(rows) if rows.len() > ROWS => {
                let right = rows.split_off(rows.len() / 2);
                rows.shrink_to_fit();
                Some(Self::Leaf(right))
            }
            Self::Branch(children) if children.len() > BRANCHES => {
                let right = children.split_off(children.len() / 2);
                children.shrink_to_fit();
                Some(Self::Branch(right))
            }
            _ => None,
        };
        (old, split.map(Arc::new))
    }
    fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        match self {
            Self::Leaf(rows) => rows
                .binary_search_by(|(k, _)| k.borrow().cmp(key))
                .ok()
                .map(|i| rows.remove(i).1),
            Self::Branch(children) => {
                let i = Self::child(children, key);
                let old = Arc::make_mut(&mut children[i].1).remove(key);
                if children[i].1.size() == 0 {
                    children.remove(i);
                } else {
                    children[i].0 = children[i].1.last().clone();
                }
                if children.len() >= 2 {
                    let left = i.min(children.len() - 2);
                    let limit = match children[left].1.as_ref() {
                        Self::Leaf(_) => ROWS,
                        _ => BRANCHES,
                    };
                    if children[left].1.size() + children[left + 1].1.size() <= limit {
                        let (_, right) = children.remove(left + 1);
                        let right = Arc::unwrap_or_clone(right);
                        match (Arc::make_mut(&mut children[left].1), right) {
                            (Self::Leaf(a), Self::Leaf(mut b)) => a.append(&mut b),
                            (Self::Branch(a), Self::Branch(mut b)) => a.append(&mut b),
                            _ => unreachable!("balanced depth"),
                        }
                        children[left].0 = children[left].1.last().clone();
                    }
                }
                old
            }
        }
    }
    fn update(&mut self, f: &(impl Fn(&K, &mut V) + Send + Sync), parallel: bool)
    where
        K: Send + Sync,
        V: Send + Sync,
    {
        match self {
            Self::Leaf(rows) => {
                for (k, v) in rows {
                    f(k, v);
                }
            }
            Self::Branch(children) if parallel => children
                .par_iter_mut()
                .for_each(|(_, c)| Arc::make_mut(c).update(f, true)),
            Self::Branch(children) => {
                for (_, c) in children {
                    Arc::make_mut(c).update(f, false);
                }
            }
        }
    }
}
pub struct Entry<'a, K, V> {
    table: &'a mut Table<K, V>,
    key: K,
}
impl<'a, K: Ord + Clone, V: Clone> Entry<'a, K, V> {
    pub fn or_insert_with(self, f: impl FnOnce() -> V) -> &'a mut V {
        if !self.table.contains_key(&self.key) {
            self.table.insert(self.key.clone(), f());
        }
        self.table.get_mut(&self.key).expect("inserted")
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}
impl<'a, K: Ord, V> IntoIterator for &'a Table<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<K: Ord + Borrow<Q>, V, Q: Ord + ?Sized> Index<&Q> for Table<K, V> {
    type Output = V;
    fn index(&self, k: &Q) -> &V {
        self.get(k).expect("missing table key")
    }
}
impl<K: Ord + Clone, V: Clone> FromIterator<(K, V)> for Table<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut table = Self::new();
        for (k, v) in iter {
            table.insert(k, v);
        }
        table
    }
}
impl<K: Ord + Clone, V: Clone, const N: usize> From<[(K, V); N]> for Table<K, V> {
    fn from(rows: [(K, V); N]) -> Self {
        rows.into_iter().collect()
    }
}
impl<K: Ord + PartialEq, V: PartialEq> PartialEq for Table<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || (self.len == other.len && self.iter().eq(other.iter()))
    }
}
impl<K: Ord + Eq, V: Eq> Eq for Table<K, V> {}

impl<K: Ord + Clone, V: Clone> From<std::collections::BTreeMap<K, V>> for Table<K, V> {
    fn from(rows: std::collections::BTreeMap<K, V>) -> Self {
        rows.into_iter().collect()
    }
}

impl<K: Ord + std::hash::Hash, V: std::hash::Hash> std::hash::Hash for Table<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        for row in self {
            row.hash(state);
        }
    }
}

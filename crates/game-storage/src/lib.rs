//! Ordered, persistent tables with dense leaves and bounded copy-on-write paths.
mod records;
mod serde_impl;
use rayon::prelude::*;
pub use records::Records;
use std::{ops::Index, sync::Arc};
const ROWS: usize = 64;
const BRANCHES: usize = 32;
#[derive(Clone, Debug)]
enum Node<K, V> {
    Leaf(Vec<(K, V)>),
    Branch(Vec<(K, Arc<Node<K, V>>)>),
}
#[derive(Clone, Debug)]
pub struct Table<K, V> {
    root: Arc<Node<K, V>>,
    len: usize,
}
impl<K, V> Default for Table<K, V> {
    fn default() -> Self {
        Self {
            root: Arc::new(Node::Leaf(Vec::new())),
            len: 0,
        }
    }
}
impl<K: Ord, V> Table<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.root.get(key)
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            stack: vec![&self.root],
            rows: [].iter(),
            remaining: self.len,
        }
    }
    pub fn scan_after<'a>(&'a self, key: Option<&'a K>) -> impl Iterator<Item = (&'a K, &'a V)> {
        let mut stack = vec![self.root.as_ref()];
        let mut rows = [].iter();
        std::iter::from_fn(move || {
            loop {
                if let Some((k, v)) = rows.next() {
                    if key.is_none_or(|lower| k > lower) {
                        return Some((k, v));
                    }
                    continue;
                }
                let node = stack.pop()?;
                if node.size() == 0 || key.is_some_and(|lower| node.last() <= lower) {
                    continue;
                }
                match node {
                    Node::Leaf(leaf) => rows = leaf.iter(),
                    Node::Branch(children) => {
                        stack.extend(children.iter().rev().map(|(_, n)| n.as_ref()))
                    }
                }
            }
        })
    }
    /// Visits only leaves on changed paths. Structural changes are safe: values
    /// from a changed leaf may be reported conservatively, never omitted.
    pub fn changed<'a>(&'a self, previous: &'a Self) -> impl Iterator<Item = (&'a K, &'a V)> {
        let mut stack = vec![self.root.as_ref()];
        let mut rows = [].iter();
        std::iter::from_fn(move || {
            loop {
                if let Some((k, v)) = rows.next() {
                    return Some((k, v));
                }
                let node = stack.pop()?;
                if node.size() == 0 {
                    continue;
                }
                if previous
                    .root
                    .at_height(node.last(), node.height())
                    .is_some_and(|old| std::ptr::eq(old, node))
                {
                    continue;
                }
                match node {
                    Node::Leaf(leaf) => rows = leaf.iter(),
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
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        // Failed lookups must not copy a leaf.
        self.get(key)?;
        Arc::make_mut(&mut self.root).get_mut(key)
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let (old, split) = Arc::make_mut(&mut self.root).insert(key, value);
        if old.is_none() {
            self.len += 1;
        }
        if let Some(right) = split {
            self.root = Arc::new(Node::Branch(vec![
                (self.root.last().clone(), self.root.clone()),
                (right.last().clone(), right),
            ]));
        }
        old
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.get(key)?;
        let old = Arc::make_mut(&mut self.root).remove(key);
        self.len -= 1;
        while let Node::Branch(children) = self.root.as_ref() {
            if children.len() != 1 {
                break;
            }
            self.root = children[0].1.clone();
        }
        old
    }
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        Entry { table: self, key }
    }
    /// Evaluates disjoint leaves with the caller's bounded worker pool. The caller
    /// owns this staged table; a panic cannot mutate any shared committed leaf.
    pub fn update(&mut self, pool: &rayon::ThreadPool, f: impl Fn(&K, &mut V) + Send + Sync)
    where
        K: Send + Sync,
        V: Send + Sync,
    {
        if self.len < 4096 || pool.current_num_threads() == 1 {
            Arc::make_mut(&mut self.root).update(&f, false);
        } else {
            pool.install(|| Arc::make_mut(&mut self.root).update(&f, true));
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
    fn child(children: &[(K, Arc<Self>)], key: &K) -> usize {
        children
            .partition_point(|(max, _)| max < key)
            .min(children.len() - 1)
    }
    fn get(&self, key: &K) -> Option<&V> {
        match self {
            Self::Leaf(rows) => rows
                .binary_search_by(|(k, _)| k.cmp(key))
                .ok()
                .map(|i| &rows[i].1),
            Self::Branch(children) => children[Self::child(children, key)].1.get(key),
        }
    }
}
impl<K: Ord + Clone, V: Clone> Node<K, V> {
    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self {
            Self::Leaf(rows) => rows
                .binary_search_by(|(k, _)| k.cmp(key))
                .ok()
                .map(|i| &mut rows[i].1),
            Self::Branch(children) => {
                let i = Self::child(children, key);
                Arc::make_mut(&mut children[i].1).get_mut(key)
            }
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
    fn remove(&mut self, key: &K) -> Option<V> {
        match self {
            Self::Leaf(rows) => rows
                .binary_search_by(|(k, _)| k.cmp(key))
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
pub struct Iter<'a, K, V> {
    stack: Vec<&'a Node<K, V>>,
    rows: std::slice::Iter<'a, (K, V)>,
    remaining: usize,
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((k, v)) = self.rows.next() {
                self.remaining -= 1;
                return Some((k, v));
            }
            match self.stack.pop()? {
                Node::Leaf(rows) => self.rows = rows.iter(),
                Node::Branch(children) => self
                    .stack
                    .extend(children.iter().rev().map(|(_, n)| n.as_ref())),
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<'a, K: Ord, V> IntoIterator for &'a Table<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<K: Ord, V> Index<&K> for Table<K, V> {
    type Output = V;
    fn index(&self, k: &K) -> &V {
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
        self.len == other.len && self.iter().eq(other.iter())
    }
}
impl<K: Ord + Eq, V: Eq> Eq for Table<K, V> {}

impl<K: Ord + Clone, V: Clone> From<std::collections::BTreeMap<K, V>> for Table<K, V> {
    fn from(rows: std::collections::BTreeMap<K, V>) -> Self {
        rows.into_iter().collect()
    }
}

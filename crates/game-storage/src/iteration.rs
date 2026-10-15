use super::*;
type Frame<'a, K, V> = std::slice::Iter<'a, (K, Arc<Node<K, V>>)>;
/// Traversal stores one frame per tree level and never allocates for a leaf.
pub(crate) struct Cursor<'a, K, V> {
    stack: Vec<Frame<'a, K, V>>,
    next: Option<&'a Node<K, V>>,
    rows: std::slice::Iter<'a, (K, V)>,
}
impl<'a, K, V> Cursor<'a, K, V> {
    pub fn new(root: &'a Node<K, V>) -> Self {
        Self {
            stack: Vec::new(),
            next: Some(root),
            rows: [].iter(),
        }
    }
    pub fn from_bound<Q: Ord + ?Sized>(root: &'a Node<K, V>, lower: Bound<&Q>) -> Self
    where
        K: Borrow<Q>,
    {
        let before = |key: &K| match lower {
            Bound::Included(start) => key.borrow() < start,
            Bound::Excluded(start) => key.borrow() <= start,
            Bound::Unbounded => false,
        };
        let mut cursor = Self::new(root);
        let mut node = root;
        while let Node::Branch(children) = node {
            let i = children
                .partition_point(|(max, _)| before(max))
                .min(children.len() - 1);
            cursor.stack.push(children[i + 1..].iter());
            node = &children[i].1;
        }
        let Node::Leaf(rows) = node else {
            unreachable!()
        };
        cursor.rows = rows[rows.partition_point(|(key, _)| before(key))..].iter();
        cursor.next = None;
        cursor
    }
}
impl<'a, K, V> Iterator for Cursor<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((k, v)) = self.rows.next() {
                return Some((k, v));
            }
            let node = self.next.take().or_else(|| {
                loop {
                    let parent = self.stack.last_mut()?;
                    if let Some((_, child)) = parent.next() {
                        break Some(child.as_ref());
                    }
                    self.stack.pop();
                }
            })?;
            match node {
                Node::Leaf(rows) => self.rows = rows.iter(),
                Node::Branch(children) => self.stack.push(children.iter()),
            }
        }
    }
}
pub struct Iter<'a, K, V> {
    pub(crate) cursor: Cursor<'a, K, V>,
    pub(crate) remaining: usize,
}
impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let row = self.cursor.next()?;
        self.remaining -= 1;
        Some(row)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}

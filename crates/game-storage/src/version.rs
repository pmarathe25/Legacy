use super::*;
use std::sync::Weak;
/// A process-local cache token. It retains no records and is never serialized.
#[derive(Clone, Debug)]
pub struct Version<K, V>(Weak<Node<K, V>>);
impl<K, V> Table<K, V> {
    pub fn version(&self) -> Version<K, V> {
        Version(self.root.as_ref().map(Arc::downgrade).unwrap_or_default())
    }
}
impl<K, V> Version<K, V> {
    pub fn matches(&self, table: &Table<K, V>) -> bool {
        match &table.root {
            Some(root) => std::ptr::eq(self.0.as_ptr(), Arc::as_ptr(root)),
            None => self.0.ptr_eq(&Weak::new()),
        }
    }
}

//! Revision-bound speculative ranges for globally coordinated batch IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchCount {
    pub owner: u32,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchRange {
    pub owner: u32,
    pub start: u64,
    pub count: u64,
}

/// An unpublished allocation tied to one physical store generation.
#[derive(Debug)]
pub struct BatchAllocation {
    pub(crate) instance: u64,
    pub(crate) revision: u64,
    pub(crate) opening: u64,
    pub(crate) next: u64,
    pub(crate) ranges: Vec<BatchRange>,
}
impl BatchAllocation {
    pub fn ranges(&self) -> &[BatchRange] {
        &self.ranges
    }
    pub fn range(&self, owner: u32) -> Option<BatchRange> {
        self.ranges
            .binary_search_by_key(&owner, |row| row.owner)
            .ok()
            .map(|index| self.ranges[index])
    }
}

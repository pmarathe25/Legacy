//! Internal compact production block data.
use game_parallel_production_model::Amount;
use smallvec::SmallVec;
#[derive(Clone)]
pub struct Bundle {
    pub reserved: SmallVec<[Amount; 1]>,
    pub outputs: SmallVec<[Amount; 1]>,
}
#[derive(Clone, Copy)]
pub struct Row {
    pub id: u64,
    pub worker: u64,
    pub region: u64,
    pub remaining: u64,
    pub recipe: u32,
    pub bundle: u32,
    pub waiting: bool,
}
pub struct Block {
    pub rows: Vec<Row>,
    pub bundles: Vec<Bundle>,
}

//! Envelope metadata shared by streaming save readers and writers.
use game_core::SimTime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest<J, N, R> {
    pub version: u32,
    pub plugins: BTreeMap<String, u32>,
    #[serde(default)]
    pub resources: BTreeMap<String, ()>,
    pub time: SimTime,
    pub seed: u64,
    pub revision: u64,
    pub next_id: u64,
    pub next_notice: u64,
    pub jobs: Vec<J>,
    pub notices: VecDeque<N>,
    pub receipts: BTreeMap<u64, R>,
}

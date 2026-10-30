//! Dense columns for a fixed logical block of people.
use game_parallel_people::Person;
pub const BLOCK: usize = 4096;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Journey {
    pub source: u64,
    pub destination: u64,
    pub destination_index: usize,
    pub remaining: u32,
}
pub struct Block {
    pub journey: Option<Journey>,
    pub region: usize,
    pub ids: Vec<u64>,
    pub health: Vec<u32>,
    pub hunger: Vec<u32>,
    pub next_health: Vec<u32>,
    pub next_hunger: Vec<u32>,
    pub claims: usize,
    pub prefix: usize,
}
impl Block {
    pub fn new(region: usize, people: &[Person]) -> Self {
        Self {
            journey: None,
            region,
            ids: people.iter().map(|p| p.id).collect(),
            health: people.iter().map(|p| p.health).collect(),
            hunger: people.iter().map(|p| p.hunger).collect(),
            next_health: vec![0; people.len()],
            next_hunger: vec![0; people.len()],
            claims: 0,
            prefix: 0,
        }
    }
}

//! Compact immutable lifecycle receipts. Expansion belongs at snapshot boundaries.
use game_parallel_families::FamilyEvent;
use rayon::prelude::*;
use std::sync::Arc;

/// Variant order is the legacy event-kind lexical order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Adulthood,
    Birth,
    Conception,
    Death,
    Partnership,
    PregnancyLoss,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Adulthood => "adulthood",
            Self::Birth => "birth",
            Self::Conception => "conception",
            Self::Death => "death",
            Self::Partnership => "partnership",
            Self::PregnancyLoss => "pregnancy_loss",
        }
    }
    pub fn arity(self) -> usize {
        match self {
            Self::Adulthood | Self::Death => 1,
            Self::Birth => 3,
            _ => 2,
        }
    }
    fn parse(name: &str) -> Result<Self, &'static str> {
        match name {
            "adulthood" => Ok(Self::Adulthood),
            "birth" => Ok(Self::Birth),
            "conception" => Ok(Self::Conception),
            "death" => Ok(Self::Death),
            "partnership" => Ok(Self::Partnership),
            "pregnancy_loss" => Ok(Self::PregnancyLoss),
            _ => Err("unknown family event kind"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    kind: Kind,
    people: [u64; 3],
    region: Option<u64>,
}
impl Row {
    pub fn new(kind: Kind, mut people: [u64; 3], region: Option<u64>) -> Self {
        people[kind.arity()..].fill(0);
        Self {
            kind,
            people,
            region,
        }
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn people(&self) -> &[u64] {
        &self.people[..self.kind.arity()]
    }
    pub fn region(&self) -> Option<u64> {
        self.region
    }
    fn expand(self, tick: u64) -> FamilyEvent {
        FamilyEvent {
            tick,
            kind: self.kind.name().into(),
            people: self.people().to_vec(),
            region: self.region,
        }
    }
}
/// Cloning a frame retains its immutable generation even when a later draft fails.
#[derive(Clone, Debug)]
pub struct Frame {
    tick: u64,
    rows: Arc<Vec<Row>>,
}
impl Frame {
    /// New phase receipts have precisely the reference kind/people ordering.
    pub fn canonical(tick: u64, mut rows: Vec<Row>) -> Self {
        rows.par_sort_unstable_by_key(|row| (row.kind, row.people));
        Self {
            tick,
            rows: Arc::new(rows),
        }
    }
    /// Initial authored order is retained until the first successful phase.
    pub fn import(tick: u64, events: &[FamilyEvent]) -> Result<Self, &'static str> {
        let rows = events
            .par_iter()
            .map(|event| {
                let kind = Kind::parse(&event.kind)?;
                if event.tick != tick || event.people.len() != kind.arity() {
                    return Err("invalid family event shape");
                }
                let mut people = [0; 3];
                people[..kind.arity()].copy_from_slice(&event.people);
                Ok(Row::new(kind, people, event.region))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            tick,
            rows: Arc::new(rows),
        })
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn export(&self) -> Vec<FamilyEvent> {
        self.rows
            .par_iter()
            .map(|row| row.expand(self.tick))
            .collect()
    }
    /// Narrow consumers need no heap-bearing event expansion.
    pub fn people(&self, kind: Kind) -> impl Iterator<Item = u64> + '_ {
        self.rows
            .iter()
            .filter(move |row| row.kind == kind)
            .map(|row| row.people[0])
    }
    pub fn counts(&self) -> [u64; 6] {
        self.rows
            .par_iter()
            .fold(
                || [0; 6],
                |mut counts, row| {
                    counts[row.kind as usize] += 1;
                    counts
                },
            )
            .reduce(
                || [0; 6],
                |mut a, b| {
                    for (a, b) in a.iter_mut().zip(b) {
                        *a += b;
                    }
                    a
                },
            )
    }
}

#[cfg(test)]
#[path = "../tests/support/generations.rs"]
mod integration_tests;

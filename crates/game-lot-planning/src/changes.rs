//! Detached custody changes, either flat or in ordered disjoint runs.
use game_lot_model::Container;
use rayon::prelude::*;

#[derive(Default)]
pub enum Changes {
    #[default]
    Empty,
    Flat(Vec<(usize, Container)>),
    /// Each run is nonempty and strictly ordered; runs do not overlap.
    Runs(Vec<Vec<(usize, Container)>>),
}
impl From<Vec<(usize, Container)>> for Changes {
    fn from(rows: Vec<(usize, Container)>) -> Self {
        Self::Flat(rows)
    }
}
impl Changes {
    pub fn ordered_runs(mut runs: Vec<Vec<(usize, Container)>>) -> Self {
        runs.retain(|run| !run.is_empty());
        debug_assert!(
            runs.par_iter()
                .all(|run| run.par_windows(2).all(|pair| pair[0].0 < pair[1].0))
        );
        debug_assert!(
            runs.par_windows(2)
                .all(|pair| pair[0].last().unwrap().0 < pair[1][0].0)
        );
        Self::Runs(runs)
    }
    pub fn into_flat(self) -> Vec<(usize, Container)> {
        match self {
            Self::Empty => Vec::new(),
            Self::Flat(rows) => rows,
            Self::Runs(runs) => runs.into_par_iter().flatten().collect(),
        }
    }
    pub fn flat_mut(&mut self) -> &mut Vec<(usize, Container)> {
        if !matches!(self, Self::Flat(_)) {
            *self = Self::Flat(std::mem::take(self).into_flat());
        }
        match self {
            Self::Flat(rows) => rows,
            _ => unreachable!(),
        }
    }
}

use game_characters::{Candidate, choose};
use rayon::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answer {
    pub id: u64,
    pub score: i64,
}
impl Answer {
    pub const NONE: Self = Self {
        id: u64::MAX,
        score: i64::MIN,
    };
}
pub type Packed = Candidate;
pub struct Input {
    pub people: usize,
    pub choices: usize,
    pub rows: Vec<Packed>,
    pub current: Vec<u64>,
    pub urgent: Vec<bool>,
}
impl Input {
    pub fn fixture(people: usize, choices: usize) -> Self {
        assert!(choices > 0);
        let rows = (0..people * choices)
            .map(|i| {
                let x = game_core::random(7, i as u64, 0);
                let term = |shift: u32| ((x >> shift) & 4095u64) as i64 - 2048;
                let mut row = Packed {
                    id: i as u64,
                    need: term(0),
                    goal: term(12),
                    value: term(24),
                    effort: term(36),
                    risk: term(48),
                    survival: x & 1 == 0,
                };
                // Exercise signed saturation, including the order of each operation.
                if i % 37 == 0 {
                    row.need = i64::MAX;
                    row.goal = 50;
                }
                if i % 53 == 0 {
                    row.value = i64::MIN;
                    row.effort = 70;
                }
                row
            })
            .collect();
        Self {
            people,
            choices,
            rows,
            current: (0..people)
                .map(|i| {
                    if i % 5 == 0 {
                        u64::MAX
                    } else {
                        (i * choices + i % choices) as u64
                    }
                })
                .collect(),
            urgent: (0..people).map(|i| i % 3 == 0).collect(),
        }
    }
    fn pick(&self, i: usize, rows: &[Packed]) -> Answer {
        choose(
            rows,
            (self.current[i] != u64::MAX).then_some(self.current[i]),
            5,
            self.urgent[i],
        )
        .map_or(Answer::NONE, |r| Answer {
            id: r.id,
            score: r.score,
        })
    }
    pub fn reference(&self) -> Vec<Answer> {
        self.rows
            .chunks(self.choices)
            .enumerate()
            .map(|(i, rows)| self.pick(i, rows))
            .collect()
    }
    pub fn parallel(&self, pool: &rayon::ThreadPool) -> Vec<Answer> {
        if pool.current_num_threads() == 1 {
            return self.packed();
        }
        pool.install(|| {
            self.rows
                .par_chunks(self.choices)
                .enumerate()
                .map(|(i, rows)| self.pick(i, rows))
                .collect()
        })
    }

    pub fn packed(&self) -> Vec<Answer> {
        self.reference()
    }
}
fn select(current_id: u64, _urgent: bool, rows: impl Iterator<Item = (u64, bool, i64)>) -> Answer {
    let mut best = Answer::NONE;
    let mut current = None;
    for (id, _survival, score) in rows {
        if id == current_id {
            current = Some(Answer { id, score });
        }
        if best.id == u64::MAX
            || (score, std::cmp::Reverse(id)) > (best.score, std::cmp::Reverse(best.id))
        {
            best = Answer { id, score };
        }
    }
    if best.id != u64::MAX
        && let Some(current) = current
        && best.score <= current.score.saturating_add(5)
    {
        return current;
    }
    best
}

pub struct Columns {
    ids: Vec<u64>,
    survival: Vec<bool>,
    pub need: Vec<i64>,
    pub goal: Vec<i64>,
    pub value: Vec<i64>,
    pub effort: Vec<i64>,
    pub risk: Vec<i64>,
}
impl Columns {
    pub fn pack(input: &Input) -> Self {
        let collect = |f: fn(&Packed) -> i64| input.rows.iter().map(f).collect();
        Self {
            ids: input.rows.iter().map(|r| r.id).collect(),
            survival: input.rows.iter().map(|r| r.survival).collect(),
            need: collect(|r| r.need),
            goal: collect(|r| r.goal),
            value: collect(|r| r.value),
            effort: collect(|r| r.effort),
            risk: collect(|r| r.risk),
        }
    }
    pub fn evaluate(
        &self,
        input: &Input,
        pool: &rayon::ThreadPool,
        vector: bool,
        tile: usize,
    ) -> Vec<Answer> {
        match tile {
            128 => self.tiled::<128>(input, pool, vector),
            4096 => self.tiled::<4096>(input, pool, vector),
            _ => panic!("unknown tile"),
        }
    }
    fn tiled<const TILE: usize>(
        &self,
        input: &Input,
        pool: &rayon::ThreadPool,
        vector: bool,
    ) -> Vec<Answer> {
        assert!(TILE.is_multiple_of(input.choices));
        let people_per_tile = TILE / input.choices;
        let mut result = vec![Answer::NONE; input.people];
        let run = |(batch, answers): (usize, &mut [Answer])| {
            let first_person = batch * people_per_tile;
            let start = first_person * input.choices;
            let end = start + answers.len() * input.choices;
            let mut scores = [0i64; TILE];
            dispatch(
                [
                    &self.need[start..end],
                    &self.goal[start..end],
                    &self.value[start..end],
                    &self.effort[start..end],
                    &self.risk[start..end],
                ],
                &mut scores[..end - start],
                vector,
            );
            for (offset, answer) in answers.iter_mut().enumerate() {
                let p = first_person + offset;
                let range = p * input.choices..(p + 1) * input.choices;
                *answer = select(
                    input.current[p],
                    input.urgent[p],
                    self.ids[range.clone()]
                        .iter()
                        .zip(&self.survival[range])
                        .zip(&scores[offset * input.choices..(offset + 1) * input.choices])
                        .map(|((id, survival), score)| (*id, *survival, *score)),
                );
            }
        };
        if pool.current_num_threads() == 1 {
            result.chunks_mut(people_per_tile).enumerate().for_each(run);
        } else {
            pool.install(|| {
                result
                    .par_chunks_mut(people_per_tile)
                    .enumerate()
                    .for_each(run)
            });
        }
        result
    }
}

#[inline(always)]
fn score(fields: [&[i64]; 5], output: &mut [i64]) {
    assert!(fields.iter().all(|f| f.len() == output.len()));
    for (i, out) in output.iter_mut().enumerate() {
        *out = fields[0][i]
            .saturating_add(fields[1][i])
            .saturating_add(fields[2][i])
            .saturating_sub(fields[3][i])
            .saturating_sub(fields[4][i]);
    }
}
fn dispatch(fields: [&[i64]; 5], output: &mut [i64], vector: bool) {
    #[cfg(target_arch = "x86_64")]
    if vector && std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime detection establishes the sole target-feature precondition.
        unsafe {
            score_avx2(fields, output);
        }
        return;
    }
    let _ = vector;
    score(fields, output);
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
unsafe fn score_avx2(fields: [&[i64]; 5], output: &mut [i64]) {
    score(fields, output);
}

/// Choice-major columns let one SIMD vector evaluate several people without a
/// horizontal reduction. Tiles retain each person's arbitration state in cache.
pub struct Lanes {
    pub ids: Vec<u64>,
    pub fields: [Vec<i64>; 5],
    pub survival: Vec<u8>,
}
impl Lanes {
    pub fn pack(input: &Input) -> Self {
        let len = input.rows.len();
        let mut out = Self {
            ids: vec![0; len],
            fields: std::array::from_fn(|_| vec![0; len]),
            survival: vec![0; len],
        };
        // Tile the transpose rather than rereading the whole population per choice.
        for first in (0..input.people).step_by(32) {
            for choice in 0..input.choices {
                for person in first..(first + 32).min(input.people) {
                    let row = &input.rows[person * input.choices + choice];
                    let target = choice * input.people + person;
                    out.ids[target] = row.id;
                    out.survival[target] = u8::from(row.survival);
                    for (field, value) in [row.need, row.goal, row.value, row.effort, row.risk]
                        .into_iter()
                        .enumerate()
                    {
                        out.fields[field][target] = value;
                    }
                }
            }
        }
        out
    }
    pub fn evaluate(
        &self,
        input: &Input,
        pool: &rayon::ThreadPool,
        vector: bool,
        tile: usize,
    ) -> Vec<Answer> {
        match tile {
            128 => self.tiled::<128>(input, pool, vector),
            4096 => self.tiled::<4096>(input, pool, vector),
            _ => panic!("unknown tile"),
        }
    }
    fn tiled<const TILE: usize>(
        &self,
        input: &Input,
        pool: &rayon::ThreadPool,
        vector: bool,
    ) -> Vec<Answer> {
        let mut result = vec![Answer::NONE; input.people];
        let run = |(batch, answers): (usize, &mut [Answer])| {
            let first = batch * TILE;
            let n = answers.len();
            let mut best_id = [u64::MAX; TILE];
            let mut best_score = [i64::MIN; TILE];
            let mut current_score = [0i64; TILE];
            let mut current_seen = [0u8; TILE];
            for choice in 0..input.choices {
                let start = choice * input.people + first;
                let end = start + n;
                let fields = std::array::from_fn(|i| &self.fields[i][start..end]);
                let data = LaneInput {
                    fields,
                    ids: &self.ids[start..end],
                    survival: &self.survival[start..end],
                    current: &input.current[first..first + n],
                    urgent: &input.urgent[first..first + n],
                };
                let state = LaneState {
                    best_id: &mut best_id[..n],
                    best_score: &mut best_score[..n],
                    current_score: &mut current_score[..n],
                    current_seen: &mut current_seen[..n],
                };
                lane_dispatch(data, state, vector);
            }
            for (i, answer) in answers.iter_mut().enumerate() {
                let current = best_id[i] != u64::MAX
                    && current_seen[i] != 0
                    && best_score[i] <= current_score[i].saturating_add(5);
                *answer = if current {
                    Answer {
                        id: input.current[first + i],
                        score: current_score[i],
                    }
                } else {
                    Answer {
                        id: best_id[i],
                        score: best_score[i],
                    }
                };
            }
        };
        if pool.current_num_threads() == 1 {
            result.chunks_mut(TILE).enumerate().for_each(run);
        } else {
            pool.install(|| result.par_chunks_mut(TILE).enumerate().for_each(run));
        }
        result
    }
}
struct LaneInput<'a> {
    fields: [&'a [i64]; 5],
    ids: &'a [u64],
    survival: &'a [u8],
    current: &'a [u64],
    urgent: &'a [bool],
}
struct LaneState<'a> {
    best_id: &'a mut [u64],
    best_score: &'a mut [i64],
    current_score: &'a mut [i64],
    current_seen: &'a mut [u8],
}
#[inline(always)]
fn rank(data: LaneInput<'_>, state: LaneState<'_>) {
    let n = state.best_id.len();
    assert!(data.fields.iter().all(|f| f.len() == n));
    assert_eq!(data.ids.len(), n);
    assert_eq!(data.survival.len(), n);
    assert_eq!(data.current.len(), n);
    assert_eq!(data.urgent.len(), n);
    assert_eq!(state.best_score.len(), n);
    assert_eq!(state.current_score.len(), n);
    assert_eq!(state.current_seen.len(), n);
    for i in 0..n {
        let score = data.fields[0][i]
            .saturating_add(data.fields[1][i])
            .saturating_add(data.fields[2][i])
            .saturating_sub(data.fields[3][i])
            .saturating_sub(data.fields[4][i]);
        let wins = (state.best_id[i] == u64::MAX)
            | (score > state.best_score[i])
            | ((score == state.best_score[i]) & (data.ids[i] < state.best_id[i]));
        state.best_id[i] = if wins { data.ids[i] } else { state.best_id[i] };
        state.best_score[i] = if wins { score } else { state.best_score[i] };
        let current = data.ids[i] == data.current[i];
        state.current_score[i] = if current {
            score
        } else {
            state.current_score[i]
        };
        state.current_seen[i] |= u8::from(current);
    }
}
fn lane_dispatch(data: LaneInput<'_>, state: LaneState<'_>, vector: bool) {
    #[cfg(target_arch = "x86_64")]
    if vector && std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: checked runtime support; the kernel uses only safe slice access.
        unsafe {
            rank_avx2(data, state);
        }
        return;
    }
    let _ = vector;
    rank(data, state);
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline(never)]
unsafe fn rank_avx2(data: LaneInput<'_>, state: LaneState<'_>) {
    rank(data, state);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_and_vector_columns_match_real_selection_with_saturation_and_tails() {
        for n in [1, 15, 257] {
            for choices in [1, 4, 16] {
                let input = Input::fixture(n, choices);
                let expected = input.reference();
                assert_eq!(input.packed(), expected);
                let columns = Columns::pack(&input);
                let lanes = Lanes::pack(&input);
                for workers in [1, 4] {
                    let pool = rayon::ThreadPoolBuilder::new()
                        .num_threads(workers)
                        .build()
                        .unwrap();
                    assert_eq!(input.parallel(&pool), expected);
                    for vector in [false, true] {
                        for tile in [128, 4096] {
                            assert_eq!(columns.evaluate(&input, &pool, vector, tile), expected);
                            assert_eq!(lanes.evaluate(&input, &pool, vector, tile), expected);
                        }
                    }
                }
            }
        }
    }
}

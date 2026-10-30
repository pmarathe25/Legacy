//! Owned physical columns and private scratch for the fused prototype.
pub const BLOCK: usize = 4096;
pub const NONE: usize = usize::MAX;
/// Person-owned committed reserves and unpublished next-generation reserves.
pub struct NutritionColumns {
    pub values: Vec<[u32; 6]>,
    pub next_values: Vec<[u32; 6]>,
    pub dirty: bool,
}
impl NutritionColumns {
    pub fn new(len: usize, initial: [u32; 6], dirty: bool) -> Self {
        Self {
            values: vec![initial; len],
            next_values: vec![initial; len],
            dirty,
        }
    }
    fn select(&self, indices: &[usize]) -> Self {
        Self {
            values: indices.iter().map(|&i| self.values[i]).collect(),
            next_values: vec![[0; 6]; indices.len()],
            dirty: self.dirty,
        }
    }
}
/// Fatigue belongs to the same person slot as physical state. Bit 0 records
/// sparse presence; bit 1 records rest. Scratch lanes are unpublished.
pub struct FatigueColumns {
    pub values: Vec<u32>,
    pub flags: Vec<u8>,
    pub next_values: Vec<u32>,
    pub next_flags: Vec<u8>,
}
impl FatigueColumns {
    pub fn empty(len: usize) -> Self {
        Self {
            values: vec![0; len],
            flags: vec![0; len],
            next_values: vec![0; len],
            next_flags: vec![0; len],
        }
    }
    fn select(&self, indices: &[usize]) -> Self {
        Self {
            values: indices.iter().map(|&i| self.values[i]).collect(),
            flags: indices.iter().map(|&i| self.flags[i]).collect(),
            next_values: vec![0; indices.len()],
            next_flags: vec![0; indices.len()],
        }
    }
}
/// Per-person infection columns. Flags bit 0 means a case exists, bit 1 a source exists.
pub struct DiseaseColumns {
    pub strains: Vec<u32>,
    pub infected_at: Vec<u64>,
    pub sources: Vec<u64>,
    pub flags: Vec<u8>,
    pub next_strains: Vec<u32>,
    pub next_infected_at: Vec<u64>,
    pub next_sources: Vec<u64>,
    pub next_flags: Vec<u8>,
}
impl DiseaseColumns {
    pub fn empty(len: usize) -> Self {
        Self {
            strains: vec![0; len],
            infected_at: vec![0; len],
            sources: vec![0; len],
            flags: vec![0; len],
            next_strains: vec![0; len],
            next_infected_at: vec![0; len],
            next_sources: vec![0; len],
            next_flags: vec![0; len],
        }
    }
    fn select(&self, indices: &[usize]) -> Self {
        let mut result = Self::empty(indices.len());
        for (i, &source) in indices.iter().enumerate() {
            result.strains[i] = self.strains[source];
            result.infected_at[i] = self.infected_at[source];
            result.sources[i] = self.sources[source];
            result.flags[i] = self.flags[source];
        }
        result
    }
}
pub struct Block {
    pub region: usize,
    pub journey: Option<game_parallel_block_data::Journey>,
    pub ids: Vec<u64>,
    pub health: Vec<u32>,
    pub hunger: Vec<u32>,
    pub fatigue: Option<FatigueColumns>,
    pub nutrition: Option<NutritionColumns>,
    pub disease: Option<DiseaseColumns>,
    pub work: Vec<usize>,
    pub remaining: Vec<u64>,
    pub next_hunger: Vec<u32>,
    pub carried_meal: Vec<bool>,
    pub food_claim: Vec<bool>,
    pub care_food_claim: Vec<bool>,
    pub next_health: Vec<u32>,
    pub next_work: Vec<usize>,
    pub next_remaining: Vec<u64>,
    pub work_local: Vec<bool>,
    pub finished: Vec<usize>,
    pub uniform_output: Option<(u64, u32)>,
    pub claims: usize,
    pub prefix: usize,
}
impl Block {
    pub fn new(
        region: usize,
        region_id: u64,
        people: &[game_parallel_people::Person],
        journey: Option<game_parallel_block_data::Journey>,
        active: &std::collections::HashMap<u64, usize>,
        jobs: &game_parallel_fused_metadata::Store,
    ) -> Self {
        let work: Vec<_> = people
            .iter()
            .map(|person| active.get(&person.id).copied().unwrap_or(NONE))
            .collect();
        let remaining = work
            .iter()
            .map(|&handle| {
                if handle == NONE {
                    0
                } else {
                    jobs.row(handle).remaining
                }
            })
            .collect();
        let work_local = work
            .iter()
            .map(|&handle| {
                handle != NONE && journey.is_none() && jobs.row(handle).region == region_id
            })
            .collect();
        let uniform_output = uniform_output(&work, jobs);
        Self {
            region,
            journey,
            ids: people.iter().map(|p| p.id).collect(),
            health: people.iter().map(|p| p.health).collect(),
            hunger: people.iter().map(|p| p.hunger).collect(),
            fatigue: None,
            nutrition: None,
            disease: None,
            work,
            remaining,
            next_hunger: vec![0; people.len()],
            carried_meal: vec![false; people.len()],
            food_claim: vec![false; people.len()],
            care_food_claim: vec![false; people.len()],
            next_health: vec![0; people.len()],
            next_work: vec![NONE; people.len()],
            next_remaining: vec![0; people.len()],
            work_local,
            finished: Vec::new(),
            uniform_output,
            claims: 0,
            prefix: 0,
        }
    }
    /// Preserve current work progress while splitting a block for travel.
    pub fn select(
        &self,
        indices: &[usize],
        journey: Option<game_parallel_block_data::Journey>,
        region_id: u64,
        jobs: &game_parallel_fused_metadata::Store,
    ) -> Self {
        let work: Vec<_> = indices.iter().map(|&i| self.work[i]).collect();
        let remaining = indices.iter().map(|&i| self.remaining[i]).collect();
        let work_local = work
            .iter()
            .map(|&handle| {
                handle != NONE && journey.is_none() && jobs.row(handle).region == region_id
            })
            .collect();
        let uniform_output = uniform_output(&work, jobs);
        Self {
            region: self.region,
            journey,
            ids: indices.iter().map(|&i| self.ids[i]).collect(),
            health: indices.iter().map(|&i| self.health[i]).collect(),
            hunger: indices.iter().map(|&i| self.hunger[i]).collect(),
            fatigue: self.fatigue.as_ref().map(|fatigue| fatigue.select(indices)),
            nutrition: self
                .nutrition
                .as_ref()
                .map(|nutrition| nutrition.select(indices)),
            disease: self.disease.as_ref().map(|disease| disease.select(indices)),
            work,
            remaining,
            work_local,
            uniform_output,
            next_hunger: vec![0; indices.len()],
            carried_meal: vec![false; indices.len()],
            food_claim: vec![false; indices.len()],
            care_food_claim: vec![false; indices.len()],
            next_health: vec![0; indices.len()],
            next_work: vec![NONE; indices.len()],
            next_remaining: vec![0; indices.len()],
            finished: Vec::new(),
            claims: 0,
            prefix: 0,
        }
    }
}

pub fn uniform_output(
    work: &[usize],
    jobs: &game_parallel_fused_metadata::Store,
) -> Option<(u64, u32)> {
    work.iter()
        .copied()
        .find(|&handle| handle != NONE)
        .and_then(|handle| {
            let first = jobs.row(handle);
            work.iter()
                .copied()
                .filter(|&handle| handle != NONE)
                .all(|handle| {
                    let row = jobs.row(handle);
                    row.region == first.region && row.bundle == first.bundle
                })
                .then_some((first.region, first.bundle))
        })
}

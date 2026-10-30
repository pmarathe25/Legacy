//! Individual life history and hereditary state. Workers return drafts, never callbacks.
mod phase;
pub use game_parallel_family_model::{
    Birth, FamilyEvent, FamilyTotals, LifeInput, Member, Pregnancy,
};
pub use phase::Draft;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyRules {
    pub adult_age: u64,
    /// Minimum age for production and farming work; zero permits all ages.
    #[serde(default)]
    pub min_work_age: u64,
    pub fertile_until: u64,
    pub gestation: u64,
    pub birth_spacing: u64,
    pub lifespan_min: u64,
    pub lifespan_max: u64,
    pub conception_one_in: u64,
    pub minimum_health: u32,
    pub maximum_hunger: u32,
    pub newborn_health: u32,
    pub mutation_per_million: u32,
    pub mutation_step: u16,
    /// Enroll actual newborns in configured social, knowledge, and adult autonomy owners.
    #[serde(default)]
    pub enroll_births: bool,
}
impl FamilyRules {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.adult_age == 0
            || self.fertile_until <= self.adult_age
            || self.gestation == 0
            || self.birth_spacing == 0
            || self.lifespan_min <= self.fertile_until
            || self.lifespan_max < self.lifespan_min
            || self.conception_one_in == 0
            || self.minimum_health == 0
            || self.newborn_health == 0
            || self.mutation_per_million > 1_000_000
            || self.mutation_step > 1000
        {
            return Err("invalid family rules");
        }
        Ok(())
    }
}
pub(crate) use game_parallel_genetics::random;
pub use game_parallel_genetics::{Genome, Sex};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Families {
    pub rules: FamilyRules,
    pub members: Vec<Member>,
    pub next_id: u64,
    #[serde(default)]
    pub last_tick: u64,
    #[serde(default)]
    pub totals: FamilyTotals,
    /// Only this boundary's canonical receipts; a runner can stream the history.
    #[serde(default)]
    pub events: Vec<FamilyEvent>,
}
use game_parallel_family_index::{MemberIndex, PhysicalIndex};
impl Families {
    pub fn validate(&self, inputs: &[LifeInput], now: u64) -> Result<(), &'static str> {
        self.validate_with_index(inputs, now).map(|_| ())
    }
    /// Validate a committed family state against physical life and health.
    pub fn validate_committed(
        &self,
        inputs: &[LifeInput],
        now: u64,
        max_health: u32,
    ) -> Result<(), &'static str> {
        let people = self.validate_with_index(inputs, now)?;
        if self.rules.newborn_health > max_health
            || self
                .members
                .par_iter()
                .any(|m| m.alive != (people[&m.id].health > 0))
        {
            return Err("family life state or newborn health differs");
        }
        Ok(())
    }
    fn validate_with_index<'a>(
        &self,
        inputs: &'a [LifeInput],
        now: u64,
    ) -> Result<PhysicalIndex<'a>, &'static str> {
        self.rules.validate()?;
        let people = PhysicalIndex::new(inputs)?;
        let members = MemberIndex::new(&self.members)?;
        if self.next_id == 0
            || self.last_tick != now
            || members.rows.len() != self.members.len()
            || members.rows.len() != inputs.len()
            || members
                .rows
                .par_iter()
                .zip(people.rows.par_iter())
                .any(|(m, p)| m.id != p.id)
            || members.rows.par_iter().any(|m| m.id >= self.next_id)
        {
            return Err("family identity set or clock differs");
        }
        if self.members.par_iter().any(|m| {
            m.id == 0
                || m.born
                    .is_some_and(|at| at > now || now - at != m.age && m.alive)
                || m.genome.alleles.iter().flatten().any(|a| *a > 1000)
                || !(m.parents.is_empty() || m.parents.len() == 2 && m.parents[0] != m.parents[1])
                || m.parents
                    .iter()
                    .any(|p| *p >= m.id || !members.contains_key(p))
                || m.partner.is_some_and(|p| {
                    p == m.id
                        || members
                            .get(&p)
                            .is_none_or(|other| other.partner != Some(m.id))
                })
                || m.pregnancy.as_ref().is_some_and(|p| {
                    p.due <= now
                        || !members.contains_key(&p.father)
                        || members
                            .get(&p.father)
                            .is_some_and(|f| f.genome.sex != Sex::Male)
                        || p.father == m.id
                        || m.genome.sex != Sex::Female
                        || m.age < self.rules.adult_age
                        || !m.alive
                })
        }) {
            return Err("invalid ancestry, genome or family commitment");
        }
        Ok(people)
    }
}

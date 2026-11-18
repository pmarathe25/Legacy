//! Individual life history and hereditary state. Workers return drafts, never callbacks.
mod lifetimes;
mod phase;
pub use game_family_model::{
    Birth, FamilyEvent, FamilyTotals, LifeInput, MatePreference, Member, Pregnancy,
};
pub use lifetimes::ElapsedLifetimes;
pub use phase::{Draft, ElapsedFamilyDraft};
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
    /// Enable independent subjective reproductive encounters.
    #[serde(default)]
    pub subjective_mating: bool,
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
pub(crate) use game_genetics::random;
pub use game_genetics::{Genome, Sex};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Families {
    pub rules: FamilyRules,
    pub members: Vec<Member>,
    pub next_id: u64,
    #[serde(default)]
    pub last_tick: u64,
    /// Optional absolute natural mortality calendar for interval hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_lifetimes: Option<ElapsedLifetimes>,
    #[serde(default)]
    pub totals: FamilyTotals,
    /// Only this boundary's canonical receipts; a runner can stream the history.
    #[serde(default)]
    pub events: Vec<FamilyEvent>,
}

/// The exact children supplied by an elapsed delivery or the children from a
/// legacy settled generation.
#[derive(Clone, Copy)]
pub struct BirthBatch<'a> {
    pub tick: u64,
    people: Option<&'a [u64]>,
}

impl<'a> BirthBatch<'a> {
    pub fn at_tick(tick: u64) -> Self {
        Self { tick, people: None }
    }

    pub fn for_people(tick: u64, people: &'a [u64]) -> Self {
        Self {
            tick,
            people: Some(people),
        }
    }

    pub fn contains(self, member: &Member) -> bool {
        self.people.map_or_else(
            || member.born == Some(self.tick),
            |people| people.binary_search(&member.id).is_ok(),
        )
    }
}
use game_family_index::{MemberIndex, PhysicalIndex};
impl Families {
    /// Convert authored pregnancy times at the scenario epoch and validate saved deadlines.
    pub fn initialize_elapsed_gestation(&mut self, now: u128) -> Result<(), &'static str> {
        let timed = self
            .elapsed_lifetimes
            .as_mut()
            .filter(|calendar| calendar.elapsed_gestation);
        if let Some(calendar) = timed {
            self.members.par_iter_mut().try_for_each(|member| {
                if let Some(pregnancy) = &mut member.pregnancy {
                    if now == 0 && pregnancy.due_at_millis.is_none() {
                        pregnancy.due_at_millis =
                            Some(u128::from(pregnancy.due) * u128::from(calendar.unit_millis));
                    }
                    if pregnancy
                        .due_at_millis
                        .is_none_or(|deadline| deadline <= now)
                    {
                        return Err("invalid elapsed pregnancy deadline");
                    }
                }
                Ok(())
            })?;
            calendar.refresh_deliveries(&self.members);
        } else if self.members.par_iter().any(|member| {
            member
                .pregnancy
                .as_ref()
                .is_some_and(|pregnancy| pregnancy.due_at_millis.is_some())
        }) {
            return Err("elapsed pregnancies require a gestation clock");
        }
        Ok(())
    }

    /// Upcoming deliveries, ordered by absolute time and then maternal identity.
    pub fn delivery_deadlines(&self) -> &[(u128, u64)] {
        self.elapsed_lifetimes
            .as_ref()
            .map_or(&[], |timeline| timeline.deliveries.as_slice())
    }

    /// Present age using this family's own time unit when configured.
    pub fn age_at(&self, member: &Member, now: u128) -> u64 {
        self.elapsed_lifetimes
            .as_ref()
            .map_or(member.age, |calendar| {
                calendar.age_at(&self.rules, member, now)
            })
    }

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
    /// Validate the mortal cohort while reserving IDs for every physical person.
    /// Excluded identities must be present exactly once and have no family member.
    pub fn validate_committed_cohort(
        &self,
        inputs: &[LifeInput],
        excluded: &[u64],
        now: u64,
        max_health: u32,
    ) -> Result<(), &'static str> {
        if excluded.is_empty() {
            return self.validate_committed(inputs, now, max_health);
        }
        self.validate_committed(&self.cohort_inputs(inputs, excluded)?, now, max_health)
    }
    /// Validate staged physical values without treating health as committed life state.
    pub fn validate_cohort(
        &self,
        inputs: &[LifeInput],
        excluded: &[u64],
        now: u64,
    ) -> Result<(), &'static str> {
        if excluded.is_empty() {
            return self.validate(inputs, now);
        }
        self.validate(&self.cohort_inputs(inputs, excluded)?, now)
    }
    fn cohort_inputs(
        &self,
        inputs: &[LifeInput],
        excluded: &[u64],
    ) -> Result<Vec<LifeInput>, &'static str> {
        let physical = PhysicalIndex::new(inputs)?;
        let excluded_ids: std::collections::HashSet<_> = excluded.iter().copied().collect();
        if excluded_ids.len() != excluded.len()
            || excluded_ids.contains(&0)
            || excluded_ids.iter().any(|id| {
                physical
                    .rows
                    .binary_search_by_key(id, |person| person.id)
                    .is_err()
            })
            || inputs.par_iter().any(|person| person.id >= self.next_id)
            || self
                .members
                .par_iter()
                .any(|member| excluded_ids.contains(&member.id))
        {
            return Err("invalid non-family identities or next person ID");
        }
        Ok(inputs
            .par_iter()
            .filter(|person| !excluded_ids.contains(&person.id))
            .copied()
            .collect())
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
                || m.born.is_some_and(|at| {
                    at > now || (self.elapsed_lifetimes.is_none() && now - at != m.age && m.alive)
                })
                || m.genome.alleles.iter().flatten().any(|a| *a > 1000)
                || m.genome.mate_desire.iter().any(|a| *a > 1000)
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
                    (p.due_at_millis.is_none() && p.due <= now)
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

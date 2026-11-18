use super::{FamilyRules, Member};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedLifetimes {
    /// Milliseconds per authored lifespan and starting-age unit.
    pub unit_millis: u64,
    /// Natural mortality deadlines ordered by time, then individual identity.
    #[serde(default)]
    deadlines: Arc<Vec<(u128, u64)>>,
    #[serde(skip)]
    by_person: Arc<HashMap<u64, u128>>,
}

impl PartialEq for ElapsedLifetimes {
    fn eq(&self, other: &Self) -> bool {
        self.unit_millis == other.unit_millis && self.deadlines == other.deadlines
    }
}

impl Eq for ElapsedLifetimes {}

pub(super) fn lifespan(rules: &FamilyRules, member: &Member) -> u64 {
    let inherited = ((u128::from(rules.lifespan_max - rules.lifespan_min)
        * u128::from(member.genome.expressed(3)))
        / 1000) as u64;
    (rules.lifespan_min + inherited).saturating_sub(member.lifespan_loss)
}

impl ElapsedLifetimes {
    fn index(deadlines: &[(u128, u64)]) -> Arc<HashMap<u64, u128>> {
        Arc::new(deadlines.par_iter().map(|&(time, id)| (id, time)).collect())
    }

    fn deadline(
        &self,
        rules: &FamilyRules,
        member: &Member,
        now: u128,
    ) -> Result<(u128, u64), &'static str> {
        let remaining = u128::from(lifespan(rules, member).saturating_sub(member.age));
        let delay = (remaining * u128::from(self.unit_millis)).max(1);
        Ok((
            now.checked_add(delay)
                .ok_or("natural death deadline overflow")?,
            member.id,
        ))
    }

    pub fn initialize(
        &mut self,
        rules: &FamilyRules,
        members: &[Member],
        now: u128,
    ) -> Result<(), &'static str> {
        if self.unit_millis == 0 {
            return Err("natural mortality time unit must be positive");
        }
        if now == 0 && self.deadlines.is_empty() {
            let mut deadlines: Vec<_> = members
                .par_iter()
                .filter(|member| member.alive)
                .map(|member| self.deadline(rules, member, now))
                .collect::<Result<_, _>>()?;
            deadlines.par_sort_unstable();
            self.deadlines = Arc::new(deadlines);
        }
        self.by_person = Self::index(&self.deadlines);
        Ok(())
    }

    /// Whole units of age at an absolute instant. Cohort records for people
    /// already confirmed dead retain their saved age.
    pub(super) fn age_at(&self, rules: &FamilyRules, member: &Member, now: u128) -> u64 {
        let Some(&deadline) = self.by_person.get(&member.id) else {
            return member.age;
        };
        let remaining = deadline
            .saturating_sub(now)
            .div_ceil(u128::from(self.unit_millis));
        let remaining = u64::try_from(remaining).unwrap_or(u64::MAX);
        lifespan(rules, member)
            .saturating_sub(remaining)
            .max(member.age)
    }

    pub fn validate(
        &self,
        members: &[Member],
        pending_deaths: &[u64],
        now: u128,
    ) -> Result<(), &'static str> {
        if self.unit_millis == 0 || self.deadlines.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid natural mortality calendar");
        }
        let mut scheduled: Vec<_> = self.deadlines.par_iter().map(|&(_, id)| id).collect();
        scheduled.par_sort_unstable();
        let mut living: Vec<_> = members
            .par_iter()
            .filter(|member| member.alive)
            .map(|member| member.id)
            .collect();
        if living.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            living.par_sort_unstable();
        }
        if scheduled != living
            || self.deadlines.par_iter().any(|&(time, id)| {
                time == 0 || (time <= now && pending_deaths.binary_search(&id).is_err())
            })
        {
            return Err("natural mortality calendar differs from living cohort");
        }
        Ok(())
    }

    /// Use the first future deadline. Elapsed fatalities remain in the calendar
    /// until the family phase consumes the pending death.
    pub fn next_after(&self, now: u128) -> Option<u128> {
        let first = self.deadlines.partition_point(|&(time, _)| time <= now);
        self.deadlines.get(first).map(|&(time, _)| time - now)
    }

    pub fn deaths_at(&self, now: u128) -> &[(u128, u64)] {
        let start = self.deadlines.partition_point(|&(time, _)| time < now);
        let end = self.deadlines.partition_point(|&(time, _)| time <= now);
        &self.deadlines[start..end]
    }

    pub(super) fn advance(
        &self,
        rules: &FamilyRules,
        deaths: &BTreeSet<u64>,
        newborns: &[Member],
        now: u128,
    ) -> Result<Self, &'static str> {
        let due = self.deadlines.partition_point(|&(time, _)| time <= now);
        if self.deadlines[..due]
            .par_iter()
            .any(|&(_, id)| !deaths.contains(&id))
        {
            return Err("natural mortality deadline passed without death");
        }
        if deaths.is_empty() && newborns.is_empty() {
            return Ok(self.clone());
        }
        let mut deadlines: Vec<_> = self
            .deadlines
            .par_iter()
            .copied()
            .filter(|&(_, id)| !deaths.contains(&id))
            .collect();
        let added: Vec<_> = newborns
            .par_iter()
            .map(|member| self.deadline(rules, member, now))
            .collect::<Result<_, _>>()?;
        if !added.is_empty() {
            deadlines.extend(added);
            deadlines.par_sort_unstable();
        }
        Ok(Self {
            unit_millis: self.unit_millis,
            by_person: Self::index(&deadlines),
            deadlines: Arc::new(deadlines),
        })
    }
}

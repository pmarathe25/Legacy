use super::{FamilyRules, Member};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElapsedLifetimes {
    /// Milliseconds per authored lifespan and starting-age unit.
    pub unit_millis: u64,
    /// Natural mortality deadlines ordered by time, then individual identity.
    #[serde(default)]
    deadlines: Arc<Vec<(u128, u64)>>,
}

pub(super) fn lifespan(rules: &FamilyRules, member: &Member) -> u64 {
    let inherited = ((u128::from(rules.lifespan_max - rules.lifespan_min)
        * u128::from(member.genome.expressed(3)))
        / 1000) as u64;
    (rules.lifespan_min + inherited).saturating_sub(member.lifespan_loss)
}

impl ElapsedLifetimes {
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
        Ok(())
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
            deadlines: Arc::new(deadlines),
        })
    }
}

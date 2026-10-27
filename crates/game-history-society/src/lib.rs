//! Sparse personal relationships and individual cultural knowledge.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialPerson {
    pub id: u64,
    pub ideas: BTreeSet<u64>,
    pub ties: BTreeMap<u64, u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Society {
    people: Vec<SocialPerson>,
    encounters: u64,
}
impl Society {
    pub fn new(mut people: Vec<SocialPerson>) -> Result<Self, String> {
        people.sort_by_key(|p| p.id);
        if !people.windows(2).all(|p| p[0].id < p[1].id) {
            return Err("duplicate social identity".into());
        }
        Ok(Self {
            people,
            encounters: 0,
        })
    }
    pub fn people(&self) -> &[SocialPerson] {
        &self.people
    }
    pub fn encounters(&self) -> u64 {
        self.encounters
    }
    /// Simultaneous knowledge exchange reads pre-phase ideas, preventing order cascades.
    pub fn meet(&mut self, pairs: &[(u64, u64)], pool: &rayon::ThreadPool) -> Result<(), String> {
        let mut partners = BTreeMap::new();
        for &(a, b) in pairs {
            if a == b || partners.insert(a, b).is_some() || partners.insert(b, a).is_some() {
                return Err("overlapping social participants".into());
            }
        }
        let ideas: BTreeMap<_, _> = self
            .people
            .iter()
            .map(|p| (p.id, p.ideas.iter().next().copied()))
            .collect();
        if partners.keys().any(|id| !ideas.contains_key(id)) {
            return Err("unknown social participant".into());
        }
        self.encounters = self
            .encounters
            .checked_add(pairs.len() as u64)
            .ok_or("encounter overflow")?;
        pool.install(|| {
            self.people.par_iter_mut().for_each(|p| {
                if let Some(other) = partners.get(&p.id) {
                    let tie = p.ties.entry(*other).or_default();
                    *tie = tie.saturating_add(1);
                    if let Some(idea) = ideas[other] {
                        p.ideas.insert(idea);
                    }
                }
            })
        });
        Ok(())
    }
    pub fn insert(&mut self, id: u64, parents: &[u64]) -> Result<(), String> {
        let index = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .err()
            .ok_or("duplicate social birth")?;
        let ideas = self
            .people
            .iter()
            .filter(|p| parents.contains(&p.id))
            .flat_map(|p| p.ideas.iter().copied())
            .collect();
        self.people.insert(
            index,
            SocialPerson {
                id,
                ideas,
                ties: BTreeMap::new(),
            },
        );
        Ok(())
    }
    pub fn innovate(&mut self, id: u64, idea: u64) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown inventor")?;
        self.people[i].ideas.insert(idea);
        Ok(())
    }
}

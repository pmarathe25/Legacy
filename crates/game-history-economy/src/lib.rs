//! Settlement inventories own stock allocation and production accounting.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settlement {
    pub id: u64,
    pub name: String,
    pub food: u64,
    pub capacity: u64,
    pub produced: u64,
    pub consumed: u64,
    pub spoiled: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Economy {
    settlements: Vec<Settlement>,
}
#[derive(Clone, Debug)]
pub struct FoodClaim {
    pub area: u64,
    pub actor: u64,
    pub patient: u64,
    pub hunger: u64,
}
impl Economy {
    pub fn new(mut settlements: Vec<Settlement>) -> Result<Self, String> {
        settlements.sort_by_key(|s| s.id);
        if settlements.is_empty()
            || !settlements.windows(2).all(|s| s[0].id < s[1].id)
            || settlements.iter().any(|s| s.id == 0 || s.food > s.capacity)
        {
            return Err("invalid settlements".into());
        }
        Ok(Self { settlements })
    }
    pub fn settlements(&self) -> &[Settlement] {
        &self.settlements
    }
    pub fn get(&self, id: u64) -> Option<&Settlement> {
        self.settlements
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.settlements[i])
    }
    pub fn harvest(
        &mut self,
        work: &[(u64, u64)],
        pool: &rayon::ThreadPool,
    ) -> Result<Vec<(u64, u64)>, String> {
        let mut yields = std::collections::BTreeMap::<u64, u64>::new();
        for &(area, amount) in work {
            let n = yields.entry(area).or_default();
            *n = n.checked_add(amount).ok_or("yield overflow")?;
        }
        if yields.keys().any(|id| self.get(*id).is_none()) {
            return Err("unknown production area".into());
        }
        pool.install(|| {
            self.settlements
                .par_iter_mut()
                .map(|s| {
                    let amount = yields
                        .get(&s.id)
                        .copied()
                        .unwrap_or(0)
                        .min(s.capacity - s.food);
                    s.produced = s
                        .produced
                        .checked_add(amount)
                        .ok_or("production overflow")?;
                    s.food += amount;
                    Ok((s.id, amount))
                })
                .collect::<Result<Vec<_>, String>>()
        })
    }
    /// Stable urgency, patient and actor precedence; no negative stocks or shared participants.
    pub fn allocate(&mut self, mut claims: Vec<FoodClaim>) -> Result<Vec<FoodClaim>, String> {
        claims.sort_by_key(|c| (std::cmp::Reverse(c.hunger), c.patient, c.actor));
        let mut occupied = BTreeSet::new();
        let mut accepted = Vec::new();
        for claim in claims {
            if occupied.contains(&claim.actor) || occupied.contains(&claim.patient) {
                continue;
            }
            let i = self
                .settlements
                .binary_search_by_key(&claim.area, |s| s.id)
                .map_err(|_| "unknown meal area")?;
            let s = &mut self.settlements[i];
            if s.food == 0 {
                continue;
            }
            s.consumed = s.consumed.checked_add(1).ok_or("consumption overflow")?;
            s.food -= 1;
            occupied.insert(claim.actor);
            occupied.insert(claim.patient);
            accepted.push(claim);
        }
        Ok(accepted)
    }
}

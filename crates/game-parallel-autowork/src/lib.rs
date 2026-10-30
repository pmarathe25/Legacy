//! Data-defined autonomous production starts with conservative stock prefixes.
use game_parallel_production::{Inventory, Production, Recipe};
use game_parallel_production_admission::{Context, Draft as Reservation};
use game_parallel_production_model::{PersonSnapshot, Start, person_index};
pub use game_parallel_work_requests as requests;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
const CHUNK: usize = 1024;
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Autowork {
    pub next_id: u64,
    pub policies: Vec<Policy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<requests::Requests>,
}
impl Clone for Autowork {
    fn clone(&self) -> Self {
        Self {
            next_id: self.next_id,
            policies: self.policies.par_iter().cloned().collect(),
            requests: self.requests.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub recipe: u32,
    pub enabled: bool,
    /// A personal choice to use materials despite someone else's claim.
    #[serde(default)]
    pub disregard_claims: bool,
}
struct Intent {
    person: u64,
    region: u64,
    recipe: u32,
}
struct Claim {
    region: u64,
    good: u32,
    person: u64,
    intent: usize,
    quantity: u64,
    fits: bool,
}
pub struct Draft {
    pub reservations: Reservation,
    pub next_id: u64,
    pub actors: Vec<u64>,
}
pub struct Plan {
    pub requests: Vec<Start>,
    pub next_id: u64,
    pub actors: Vec<u64>,
}
impl Autowork {
    pub fn validate(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
    ) -> Result<(), String> {
        let recipes: HashSet<_> = production.recipes.iter().map(|r| r.id).collect();
        let mut people_ids: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        if people_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            people_ids.par_sort_unstable();
        }
        if people_ids.par_windows(2).any(|p| p[0] == p[1])
            || self.policies.par_iter().any(|policy| {
                person_index(people, policy.person).is_none() || !recipes.contains(&policy.recipe)
            })
        {
            return Err("invalid autonomous work policy".into());
        }
        if let Some(requests) = &self.requests {
            requests.validate(people, production)?;
        }
        Ok(())
    }
    /// A granted start spends this interval on setup; work begins next tick.
    pub fn prepare(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
        busy: &[u64],
    ) -> Result<Option<Draft>, String> {
        self.prepare_with_property(people, production, busy, None)
    }
    pub fn prepare_with_property(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
        busy: &[u64],
        property: Option<&game_parallel_property::Property>,
    ) -> Result<Option<Draft>, String> {
        self.validate(people, production)?;
        let max_id = production.jobs.par_iter().map(|job| job.id).max();
        let Some(plan) =
            self.plan_validated_with_property(people, production, busy, max_id, property)?
        else {
            return Ok(None);
        };
        let reservations = Context {
            jobs: &production.jobs,
            recipes: &production.recipes,
            inventories: &production.inventories,
            people,
        }
        .prepare(&plan.requests)?;
        Ok(Some(Draft {
            reservations,
            next_id: plan.next_id,
            actors: plan.actors,
        }))
    }
    /// Decide starts against the supplied current maximum job ID and physical snapshot.
    pub fn plan(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
        busy: &[u64],
        max_job_id: Option<u64>,
    ) -> Result<Option<Plan>, String> {
        self.validate(people, production)?;
        self.plan_validated(people, production, busy, max_job_id)
    }
    /// Plan against policy data validated when the private runtime was compiled.
    pub fn plan_validated(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
        busy: &[u64],
        max_job_id: Option<u64>,
    ) -> Result<Option<Plan>, String> {
        self.plan_validated_with_property(people, production, busy, max_job_id, None)
    }
    pub fn plan_validated_with_property(
        &self,
        people: &[PersonSnapshot],
        production: &Production,
        busy: &[u64],
        max_job_id: Option<u64>,
        property: Option<&game_parallel_property::Property>,
    ) -> Result<Option<Plan>, String> {
        let rights = property.map(|property| property.index());
        let recipes: HashMap<_, _> = production.recipes.iter().map(|r| (r.id, r)).collect();
        let inventories: HashMap<_, _> = production
            .inventories
            .iter()
            .map(|i| (i.region, i))
            .collect();
        let mut busy = busy.to_vec();
        busy.par_sort_unstable();
        let mut intents: Vec<_> = self
            .policies
            .par_iter()
            .filter_map(|policy| {
                if !policy.enabled || busy.binary_search(&policy.person).is_ok() {
                    return None;
                }
                let (_, region, health, hunger) =
                    people[*person_index(people, policy.person).as_ref()?];
                let region = region?;
                if health == 0 || hunger > production.max_work_hunger {
                    return None;
                }
                let recipe = recipes[&policy.recipe];
                if !policy.disregard_claims
                    && rights
                        .as_ref()
                        .is_some_and(|rights| !rights.allows_recipe(policy.person, region, recipe))
                {
                    return None;
                }
                let inventory = inventories.get(&region)?;
                if recipe
                    .inputs
                    .iter()
                    .any(|a| !inventory.stocks.iter().any(|stock| stock.good == a.good))
                    || recipe.outputs.iter().any(|a| {
                        !inventory
                            .stocks
                            .iter()
                            .any(|stock| stock.good == a.good && a.quantity <= stock.capacity)
                    })
                {
                    return None;
                }
                Some(Intent {
                    person: policy.person,
                    region,
                    recipe: policy.recipe,
                })
            })
            .collect();
        if intents
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            intents.par_sort_unstable_by_key(|intent| intent.person);
        }
        let granted = grant(intents, &recipes, &inventories);
        if granted.is_empty() {
            return Ok(None);
        }
        let after_live = max_job_id
            .map_or(Some(0), |id| id.checked_add(1))
            .ok_or("job id overflow")?;
        let first = self.next_id.max(after_live);
        let next_id = first
            .checked_add(u64::try_from(granted.len()).map_err(|_| "job count overflow")?)
            .ok_or("job id overflow")?;
        let requests: Vec<_> = granted
            .par_iter()
            .enumerate()
            .map(|(index, intent)| Start {
                id: first + index as u64,
                worker: intent.person,
                region: intent.region,
                recipe: intent.recipe,
                units: 1,
            })
            .collect();
        let actors = granted.par_iter().map(|intent| intent.person).collect();
        Ok(Some(Plan {
            requests,
            next_id,
            actors,
        }))
    }
}

/// Resolve all claims directly when stock is abundant; otherwise preserve the
/// deterministic per-resource prefix rule for contested atomic jobs.
fn grant(
    intents: Vec<Intent>,
    recipes: &HashMap<u32, &Recipe>,
    inventories: &HashMap<u64, &Inventory>,
) -> Vec<Intent> {
    let homogeneous = intents.first().is_some_and(|first| {
        intents
            .par_iter()
            .all(|intent| intent.region == first.region && intent.recipe == first.recipe)
    });
    if homogeneous {
        let first = &intents[0];
        // Every claimant consumes the same bundle, so each contested
        // resource grants exactly the first stock / quantity people. The
        // intersection of these prefixes is itself a prefix.
        let limit = recipes[&first.recipe]
            .inputs
            .iter()
            .map(|input| {
                let stock = inventories[&first.region]
                    .stocks
                    .iter()
                    .find(|stock| stock.good == input.good)
                    .expect("eligible input stock");
                usize::try_from(stock.quantity.checked_div(input.quantity).unwrap_or(0))
                    .unwrap_or(usize::MAX)
            })
            .min()
            .unwrap_or(usize::MAX)
            .min(intents.len());
        return intents.into_par_iter().take(limit).collect();
    } else {
        let totals: HashMap<(u64, u32), u128> = intents
            .par_iter()
            .fold(HashMap::new, |mut totals, intent| {
                for input in &recipes[&intent.recipe].inputs {
                    *totals.entry((intent.region, input.good)).or_insert(0) +=
                        u128::from(input.quantity);
                }
                totals
            })
            .reduce(HashMap::new, |mut left, right| {
                for (key, value) in right {
                    *left.entry(key).or_insert(0) += value;
                }
                left
            });
        if totals.iter().all(|(&(region, good), &quantity)| {
            let stock = inventories[&region]
                .stocks
                .iter()
                .find(|stock| stock.good == good)
                .expect("eligible input stock");
            quantity <= u128::from(stock.quantity)
        }) {
            return intents;
        }
    }
    let mut claims: Vec<_> = intents
        .par_iter()
        .enumerate()
        .flat_map_iter(|(index, intent)| {
            recipes[&intent.recipe]
                .inputs
                .iter()
                .map(move |item| Claim {
                    region: intent.region,
                    good: item.good,
                    person: intent.person,
                    intent: index,
                    quantity: item.quantity,
                    fits: false,
                })
        })
        .collect();
    if claims.par_windows(2).any(|pair| {
        (pair[0].region, pair[0].good, pair[0].person)
            > (pair[1].region, pair[1].good, pair[1].person)
    }) {
        claims.par_sort_unstable_by_key(|claim| (claim.region, claim.good, claim.person));
    }
    claims
        .par_chunk_by_mut(|a, b| (a.region, a.good) == (b.region, b.good))
        .for_each(|group| {
            let stock = inventories[&group[0].region]
                .stocks
                .iter()
                .find(|s| s.good == group[0].good)
                .unwrap()
                .quantity;
            let sums: Vec<u128> = group
                .par_chunks(CHUNK)
                .map(|chunk| chunk.iter().map(|c| u128::from(c.quantity)).sum())
                .collect();
            let mut total = 0u128;
            let offsets: Vec<_> = sums
                .into_iter()
                .map(|sum| {
                    let start = total;
                    total += sum;
                    start
                })
                .collect();
            group
                .par_chunks_mut(CHUNK)
                .zip(offsets.into_par_iter())
                .for_each(|(chunk, mut prefix)| {
                    for claim in chunk {
                        prefix += u128::from(claim.quantity);
                        claim.fits = prefix <= u128::from(stock);
                    }
                });
        });
    let denied: Vec<_> = (0..intents.len()).map(|_| AtomicBool::new(false)).collect();
    claims
        .par_iter()
        .filter(|claim| !claim.fits)
        .for_each(|claim| denied[claim.intent].store(true, Ordering::Relaxed));
    let granted: Vec<_> = intents
        .into_par_iter()
        .enumerate()
        .filter(|(i, _)| !denied[*i].load(Ordering::Relaxed))
        .map(|(_, intent)| intent)
        .collect();
    granted
}

//! Parallel cultivation over real seed and dated harvest custody.
pub mod content;
use game_parallel_economy_person::Person;
use game_parallel_lot_content::Catalog;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub id: u32,
    pub seed: u32,
    pub seed_quantity: u64,
    pub output: u32,
    pub output_quantity: u64,
    pub growth_ticks: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Fallow,
    Growing,
    Ripe,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: u64,
    pub region: u64,
    pub crop: u32,
    pub phase: Phase,
    pub remaining: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agriculture {
    /// Founders may pass on a personal inclination, not social pressure.
    #[serde(default)]
    pub inherit_births: bool,
    pub crops: Vec<Crop>,
    pub fields: Vec<Field>,
    pub policies: Vec<Policy>,
    pub max_hunger: u32,
}
impl Clone for Agriculture {
    fn clone(&self) -> Self {
        Self {
            inherit_births: self.inherit_births,
            crops: self.crops.par_iter().cloned().collect(),
            fields: self.fields.par_iter().cloned().collect(),
            policies: self.policies.par_iter().cloned().collect(),
            max_hunger: self.max_hunger,
        }
    }
}

/// Actual newborns may inherit a parent's inclination, never another person's pressure.
pub fn enroll(
    owner: Option<&mut Agriculture>,
    family: Option<&game_parallel_families::Families>,
    tick: u64,
) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|member| member.born == Some(tick))
        .collect();
    if newborns.is_empty() {
        return;
    }
    let mut policies: Vec<_> = owner.policies.par_iter().collect();
    policies.par_sort_unstable_by_key(|policy| policy.person);
    let newborns: Vec<_> = newborns
        .into_par_iter()
        .filter_map(|member| {
            if policies
                .binary_search_by_key(&member.id, |policy| policy.person)
                .is_ok()
            {
                return None;
            }
            let parent = member.parents.iter().find_map(|id| {
                policies
                    .binary_search_by_key(id, |policy| policy.person)
                    .ok()
                    .map(|i| policies[i])
            })?;
            Some(Policy {
                person: member.id,
                willing: parent.willing,
                pressured_by: None,
                pressure_threshold: None,
            })
        })
        .collect();
    owner.policies.par_extend(newborns);
}

impl Agriculture {
    pub fn set_choice(
        &self,
        people: &[Person],
        person: u64,
        willing: bool,
    ) -> Result<Self, String> {
        if people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .is_none_or(|index| people[index].health == 0)
        {
            return Err("unknown or dead person".into());
        }
        let mut next = self.clone();
        if let Some(policy) = next.policies.iter_mut().find(|row| row.person == person) {
            policy.willing = willing;
        } else {
            next.policies.push(Policy {
                person,
                willing,
                pressured_by: None,
                pressure_threshold: None,
            });
        }
        Ok(next)
    }
    pub fn validate(&self, people: &[Person], goods: &[Good], view: &View) -> Result<(), String> {
        let ids: HashSet<_> = people.par_iter().map(|p| p.id).collect();
        let goods: HashSet<_> = goods.par_iter().map(|g| g.id).collect();
        let mut crops: Vec<_> = self.crops.par_iter().collect();
        if crops.par_windows(2).any(|p| p[0].id > p[1].id) {
            crops.par_sort_unstable_by_key(|c| c.id);
        }
        let mut fields: Vec<_> = self.fields.par_iter().map(|f| f.id).collect();
        fields.par_sort_unstable();
        let mut policies: Vec<_> = self.policies.par_iter().map(|p| p.person).collect();
        policies.par_sort_unstable();
        if crops.first().is_some_and(|c| c.id == 0)
            || fields.first() == Some(&0)
            || crops.par_windows(2).any(|p| p[0].id == p[1].id)
            || fields.par_windows(2).any(|p| p[0] == p[1])
            || policies.par_windows(2).any(|p| p[0] == p[1])
            || self.crops.par_iter().any(|c| {
                !goods.contains(&c.seed)
                    || !goods.contains(&c.output)
                    || c.seed_quantity == 0
                    || c.output_quantity == 0
                    || c.growth_ticks == 0
            })
            || self.policies.par_iter().any(|p| {
                !ids.contains(&p.person)
                    || p.pressured_by
                        .is_some_and(|id| id == p.person || !ids.contains(&id))
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
            })
            || self.fields.par_iter().any(|f| {
                let crop = crops
                    .binary_search_by_key(&f.crop, |c| c.id)
                    .ok()
                    .map(|i| crops[i]);
                crop.is_none_or(|c| {
                    view.container(Place::Stock {
                        region: f.region,
                        good: c.seed,
                    })
                    .is_none()
                        || view
                            .container(Place::Stock {
                                region: f.region,
                                good: c.output,
                            })
                            .is_none()
                        || match f.phase {
                            Phase::Growing => f.remaining == 0 || f.remaining > c.growth_ticks,
                            _ => f.remaining != 0,
                        }
                })
            })
        {
            return Err("invalid cultivation state".into());
        }
        Ok(())
    }
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $physical:expr, $view:expr) => {
        if let Some(agriculture) = &$state.agriculture {
            agriculture.validate(&$state.people, $physical.goods(), &$view)?;
        }
    };
}
#[macro_export]
macro_rules! cultivation_methods {
    () => {
        pub fn set_farming_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
            let next = self
                .agriculture
                .as_ref()
                .ok_or("cultivation is disabled")?
                .set_choice(&self.people, person, willing)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agriculture = Some(next);
            self.epoch = epoch;
            Ok(())
        }
    };
}

pub struct Plan {
    pub actors: Vec<u64>,
    pub plant: Vec<Transaction>,
    pub harvest: Vec<Transaction>,
    pub uses: Vec<game_parallel_economy_claims::Use>,
    fields: Vec<Field>,
    pub changes: Vec<(u64, u64, Phase, u32)>,
}
pub fn append_planting(plan: &mut Option<Plan>, actions: &mut Vec<Transaction>) -> Vec<u64> {
    plan.as_mut().map_or_else(Vec::new, |plan| {
        let compact = plan.plant.par_iter().map(|row| row.id).collect();
        actions.append(&mut plan.plant);
        compact
    })
}
pub fn physical_growth<'a>(
    plan: Option<&'a Plan>,
    compact: &'a [u64],
) -> game_parallel_lot_runtime::Growth<'a> {
    game_parallel_lot_runtime::Growth {
        harvest: plan.map_or(&[], |plan| plan.harvest.as_slice()),
        compact_consumption: compact,
    }
}
pub fn publish_optional(
    plan: Option<Plan>,
    owner: Option<&Agriculture>,
    events: &Events,
) -> Result<Option<Agriculture>, String> {
    plan.map(|plan| plan.publish(owner.ok_or("missing cultivation owner")?, events))
        .transpose()
}
pub struct Context<'a> {
    pub agriculture: &'a Agriculture,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub busy: &'a [u64],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub tick_millis: u64,
    pub tick: u64,
    pub manual: Option<(u64, u64)>,
    pub season: Option<&'a game_parallel_environment::Season>,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let manual = if let Some((actor, field)) = self.manual {
            let person = self
                .people
                .binary_search_by_key(&actor, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let field = self
                .agriculture
                .fields
                .iter()
                .find(|f| f.id == field)
                .ok_or("unknown field")?;
            if !person.controlled
                || person.health == 0
                || person.region != Some(field.region)
                || person.hunger > self.agriculture.max_hunger
                || field.phase == Phase::Growing
                || self.season.is_some_and(|season| match field.phase {
                    Phase::Fallow => !season.planting_allowed,
                    Phase::Ripe => !season.harvest_allowed,
                    Phase::Growing => false,
                })
            {
                return Err("person cannot cultivate this field now".into());
            }
            Some((field, actor))
        } else {
            None
        };
        let catalog = Catalog::from_tick_millis(self.goods, self.tick_millis)?;
        let mut crops: Vec<_> = self.agriculture.crops.par_iter().collect();
        if crops.par_windows(2).any(|p| p[0].id > p[1].id) {
            crops.par_sort_unstable_by_key(|c| c.id);
        }
        let mut policies: Vec<_> = self.agriculture.policies.par_iter().collect();
        if policies.par_windows(2).any(|p| p[0].person > p[1].person) {
            policies.par_sort_unstable_by_key(|p| p.person);
        }
        let opinions = self.opinions.map(Opinions::index);
        let mut workers: Vec<_> = self
            .people
            .par_iter()
            .filter_map(|person| {
                if person.controlled
                    || person.health == 0
                    || person.hunger > self.agriculture.max_hunger
                    || self.busy.binary_search(&person.id).is_ok()
                {
                    return None;
                }
                let policy = policies
                    .binary_search_by_key(&person.id, |p| p.person)
                    .ok()
                    .map(|i| policies[i])?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (policy.willing || pressured).then_some((person.region?, person.id))
            })
            .collect();
        if workers.par_windows(2).any(|p| p[0] > p[1]) {
            workers.par_sort_unstable();
        }
        let mut candidates: Vec<_> = self
            .agriculture
            .fields
            .par_iter()
            .filter(|field| {
                if manual.is_some_and(|(chosen, _)| chosen.id == field.id) {
                    return false;
                }
                let crop = crops
                    .binary_search_by_key(&field.crop, |row| row.id)
                    .ok()
                    .map(|i| crops[i]);
                crop.is_some_and(|crop| match field.phase {
                    Phase::Growing => false,
                    Phase::Fallow if self.season.is_some_and(|s| !s.planting_allowed) => false,
                    Phase::Ripe if self.season.is_some_and(|s| !s.harvest_allowed) => false,
                    Phase::Fallow => self
                        .view
                        .container(Place::Stock {
                            region: field.region,
                            good: crop.seed,
                        })
                        .is_some_and(|stock| stock.quantity(crop.seed) >= crop.seed_quantity),
                    Phase::Ripe => self
                        .view
                        .container(Place::Stock {
                            region: field.region,
                            good: crop.output,
                        })
                        .is_some_and(|stock| stock.capacity - stock.used >= crop.output_quantity),
                })
            })
            .collect();
        let key = |f: &&Field| (f.region, f.phase != Phase::Ripe, f.id);
        if candidates.par_windows(2).any(|p| key(&p[0]) > key(&p[1])) {
            candidates.par_sort_unstable_by_key(key);
        }
        let mut picked: Vec<_> = candidates
            .par_chunk_by(|a, b| a.region == b.region)
            .flat_map(|group| {
                let first = workers.partition_point(|row| row.0 < group[0].region);
                let last = workers.partition_point(|row| row.0 <= group[0].region);
                if let Some(claims) = self.claims {
                    return game_parallel_economy_claims::match_pairs(
                        group,
                        &workers[first..last],
                        |field, worker| {
                            if field.phase != Phase::Fallow {
                                return true;
                            }
                            let crop = crops[crops
                                .binary_search_by_key(&field.crop, |c| c.id)
                                .expect("validated crop")];
                            claims.allows(worker.1, field.region, crop.seed)
                        },
                    )
                    .into_par_iter()
                    .map(|(field, worker)| (field, worker.1))
                    .collect::<Vec<_>>()
                    .into_par_iter();
                }
                group
                    .par_iter()
                    .zip(workers[first..last].par_iter())
                    .map(|(field, worker)| (*field, worker.1))
                    .collect::<Vec<_>>()
                    .into_par_iter()
            })
            .collect();
        picked.extend(manual);
        let count = u64::try_from(picked.len()).map_err(|_| "cultivation action overflow")?;
        base.checked_add(count)
            .ok_or("cultivation action overflow")?;
        let plans: Vec<_> = picked
            .par_iter()
            .enumerate()
            .map(|(index, &(field, worker))| -> Result<_, String> {
                let action = base + index as u64;
                let crop = crops[crops
                    .binary_search_by_key(&field.crop, |row| row.id)
                    .map_err(|_| "unknown crop")?];
                let (plant, harvest, phase, remaining) = match field.phase {
                    Phase::Fallow => (
                        Some(Transaction {
                            id: action,
                            movements: vec![Movement {
                                id: action,
                                from: Place::Stock {
                                    region: field.region,
                                    good: crop.seed,
                                },
                                to: None,
                                good: crop.seed,
                                quantity: crop.seed_quantity,
                            }],
                            creations: vec![],
                        }),
                        None,
                        Phase::Growing,
                        crop.growth_ticks,
                    ),
                    Phase::Ripe => (
                        None,
                        Some(Transaction {
                            id: action,
                            movements: vec![],
                            creations: vec![catalog.creation(
                                action,
                                Place::Stock {
                                    region: field.region,
                                    good: crop.output,
                                },
                                crop.output,
                                crop.output_quantity,
                                self.tick,
                            )?],
                        }),
                        Phase::Fallow,
                        0,
                    ),
                    Phase::Growing => return Err("growing field selected".into()),
                };
                Ok((worker, field.id, action, phase, remaining, plant, harvest))
            })
            .collect::<Result<_, _>>()?;
        let uses = if self.claims.is_some() {
            plans
                .par_iter()
                .filter_map(|(worker, _, action, _, _, plant, _)| {
                    let movement = plant.as_ref()?.movements.first()?;
                    let Place::Stock { region, good } = movement.from else {
                        return None;
                    };
                    Some(game_parallel_economy_claims::Use {
                        action: *action,
                        person: *worker,
                        region,
                        good,
                        quantity: movement.quantity,
                        job: false,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut fields: Vec<_> = self.agriculture.fields.par_iter().cloned().collect();
        fields
            .par_iter_mut()
            .filter(|f| f.phase == Phase::Growing)
            .for_each(|f| {
                let factor = self.season.map_or(1000, |s| {
                    if s.tending_allowed {
                        s.food_growth_per_thousand
                    } else {
                        0
                    }
                });
                let tick = u128::from(self.tick);
                let growth = ((tick * u128::from(factor) / 1000)
                    - (tick.saturating_sub(1) * u128::from(factor) / 1000))
                    as u32;
                f.remaining = f.remaining.saturating_sub(growth);
                if f.remaining == 0 {
                    f.phase = Phase::Ripe;
                }
            });
        let (actors, records): (Vec<_>, Vec<_>) = plans
            .into_par_iter()
            .map(
                |(worker, field, action, phase, remaining, plant, harvest)| {
                    (
                        worker,
                        ((field, action, phase, remaining), (plant, harvest)),
                    )
                },
            )
            .unzip();
        let (changes, outputs): (Vec<_>, Vec<_>) = records.into_par_iter().unzip();
        let (plant, harvest): (Vec<_>, Vec<_>) = outputs.into_par_iter().unzip();
        Ok(Plan {
            actors,
            plant: plant.into_par_iter().flatten().collect(),
            harvest: harvest.into_par_iter().flatten().collect(),
            uses,
            changes,
            fields,
        })
    }
}
impl Plan {
    pub fn publish(self, owner: &Agriculture, events: &Events) -> Result<Agriculture, String> {
        self.plant
            .into_par_iter()
            .chain(self.harvest.into_par_iter())
            .for_each(drop);
        let mut decisions: Vec<_> = events.physical_actions.par_iter().collect();
        if decisions.par_windows(2).any(|p| p[0].id > p[1].id) {
            decisions.par_sort_unstable_by_key(|r| r.id);
        }
        if decisions
            .par_windows(2)
            .any(|pair| pair[0].id == pair[1].id)
        {
            return Err("duplicate cultivation outcome".into());
        }
        let mut changes = self.changes;
        if changes.par_windows(2).any(|p| p[0].0 > p[1].0) {
            changes.par_sort_unstable_by_key(|row| row.0);
        }
        let rows: Vec<_> = self
            .fields
            .into_par_iter()
            .map(|mut field| -> Result<_, String> {
                if let Ok(i) = changes.binary_search_by_key(&field.id, |row| row.0) {
                    let (_, action, phase, remaining) = changes[i];
                    let decision = decisions
                        .binary_search_by_key(&action, |r| r.id)
                        .ok()
                        .map(|i| decisions[i])
                        .ok_or("missing cultivation outcome")?;
                    if decision.accepted {
                        field.phase = phase;
                        field.remaining = remaining;
                    }
                }
                Ok(field)
            })
            .collect::<Result<_, _>>()?;
        Ok(Agriculture {
            inherit_births: owner.inherit_births,
            crops: owner.crops.par_iter().cloned().collect(),
            fields: rows,
            policies: owner.policies.par_iter().cloned().collect(),
            max_hunger: owner.max_hunger,
        })
    }
}

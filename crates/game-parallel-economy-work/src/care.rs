//! Individual dependent care and physical meal delivery.
pub use game_parallel_dependent_care::DependentCare;
use game_parallel_economy_person::{
    Person, Rules,
    travel::{Colocation, Travel},
};
use game_parallel_families::Families;
use game_parallel_family_columns::Store;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{Good, PersonSnapshot};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn validate(
    care: &DependentCare,
    family: Option<&Families>,
    people: &HashSet<u64>,
    tick: u64,
) -> Result<(), String> {
    let family = family.ok_or("dependent care requires family")?;
    care.validate(people, family.rules.adult_age, tick)
        .map_err(str::to_owned)
}
pub fn dependents(care: &DependentCare, family: &Families) -> Vec<u64> {
    let mut ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.age < care.rules.dependent_until)
        .map(|m| m.id)
        .collect();
    ids.par_sort_unstable();
    ids
}
pub fn inherit(care: &mut DependentCare, family: &Families, newborns: &[u64]) {
    if !care.inherit_births || newborns.is_empty() {
        return;
    }
    let mut members: Vec<_> = family.members.par_iter().collect();
    members.par_sort_unstable_by_key(|m| m.id);
    let births: Vec<_> = newborns
        .par_iter()
        .map(|id| {
            let member = members[members
                .binary_search_by_key(id, |m| m.id)
                .expect("newborn member")];
            (member.id, [member.parents[0], member.parents[1]], 2)
        })
        .collect();
    care.inherit_preferences(&births);
}
pub struct Plan {
    pub actors: Vec<u64>,
    pub actions: Vec<Transaction>,
    pub uses: Vec<game_parallel_economy_claims::Use>,
    pub draft: game_parallel_dependent_care::Draft,
}
pub struct Context<'a> {
    pub care: &'a DependentCare,
    pub family: &'a Families,
    pub people: &'a [PersonSnapshot],
    pub rules: &'a Rules,
    pub goods: &'a [Good],
    pub view: &'a View,
    pub travel: Option<&'a Travel>,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a game_parallel_economy_claims::Index<'a>>,
    pub busy: &'a [u64],
    pub eating: &'a [(u64, u32)],
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let family = Store::compile(&self.family.members).map_err(str::to_owned)?;
        let opinions = self.opinions.map(Opinions::index);
        let locations = Colocation::new(self.travel);
        let colocated = |a: usize, b: usize| {
            locations
                .region(
                    (self.people[a].0, self.people[a].1),
                    (self.people[b].0, self.people[b].1),
                )
                .is_some()
        };
        let pressure = |parent, child| {
            opinions.as_ref().map_or(0, |index| {
                let (fear, obligation) = index.feelings(parent, child);
                fear.saturating_add(obligation)
            })
        };
        let draft = self.care.prepare(
            &family,
            self.family.rules.adult_age,
            self.people,
            self.busy,
            (self.seed, self.tick),
            (colocated, pressure),
        );
        let edible: HashMap<_, _> = self
            .goods
            .iter()
            .filter_map(|good| good.food.as_ref().map(|food| (good.id, food.portion)))
            .collect();
        let mut stocks: Vec<_> = self
            .view
            .containers
            .par_iter()
            .filter_map(|row| {
                let Place::Stock { region, good } = row.place else {
                    return None;
                };
                let portion = *edible.get(&good)?;
                (row.quantity(good) >= portion).then_some((region, good, portion))
            })
            .collect();
        stocks.par_sort_unstable();
        let choices: Vec<_> = draft
            .events
            .par_iter()
            .filter_map(|event| {
                let parent = event.caregiver?;
                let child = self
                    .people
                    .binary_search_by_key(&event.child, |row| row.0)
                    .ok()
                    .map(|i| self.people[i])?;
                if child.3.saturating_add(self.rules.hunger_per_tick) < self.rules.meal_threshold
                    || self
                        .eating
                        .binary_search_by_key(&child.0, |row| row.0)
                        .is_ok()
                {
                    return None;
                }
                let holder = self.view.container(Place::Person(child.0))?;
                let fits = |quantity| quantity <= holder.capacity - holder.used;
                let carried = self
                    .care
                    .rules
                    .feed_from_cargo
                    .then(|| self.view.container(Place::Person(parent)))
                    .flatten()
                    .and_then(|holder| {
                        holder.goods.iter().find_map(|item| {
                            edible
                                .get(&item.good)
                                .filter(|&&portion| item.quantity >= portion && fits(portion))
                                .map(|&portion| (Place::Person(parent), item.good, portion))
                        })
                    });
                let local = || {
                    let region = child.1?;
                    let first = stocks.partition_point(|row| row.0 < region);
                    let last = stocks.partition_point(|row| row.0 <= region);
                    stocks[first..last]
                        .iter()
                        .find(|row| {
                            fits(row.2)
                                && self
                                    .claims
                                    .is_none_or(|index| index.allows(parent, region, row.1))
                        })
                        .map(|row| {
                            (
                                Place::Stock {
                                    region,
                                    good: row.1,
                                },
                                row.1,
                                row.2,
                            )
                        })
                };
                let (from, good, quantity) =
                    carried.or_else(|| self.care.rules.fetch_local_food.then(local).flatten())?;
                Some((child.0, parent, from, good, quantity))
            })
            .collect();
        let count = u64::try_from(choices.len()).map_err(|_| "care action overflow")?;
        base.checked_add(count).ok_or("care action overflow")?;
        let actions = choices
            .par_iter()
            .enumerate()
            .map(|(index, &(child, _, from, good, quantity))| {
                let id = base + index as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from,
                        to: Some(Place::Person(child)),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let uses = if self.claims.is_some() {
            choices
                .par_iter()
                .enumerate()
                .filter_map(|(index, &(_, person, from, good, quantity))| {
                    let Place::Stock { region, .. } = from else {
                        return None;
                    };
                    Some(game_parallel_economy_claims::Use {
                        action: base + index as u64,
                        person,
                        region,
                        good,
                        quantity,
                        job: false,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Plan {
            actors: draft.actors.clone(),
            actions,
            uses,
            draft,
        })
    }
}
impl Plan {
    pub fn apply(self, care: &mut DependentCare, people: &mut [Person]) {
        let mut neglected: Vec<_> = self
            .draft
            .events
            .par_iter()
            .filter(|row| row.caregiver.is_none())
            .map(|row| row.child)
            .collect();
        neglected.par_sort_unstable();
        people.par_iter_mut().for_each(|person| {
            if neglected.binary_search(&person.id).is_ok() {
                person.health = person.health.saturating_sub(care.rules.neglect_damage);
            }
        });
        care.events = self.draft.events;
    }
}

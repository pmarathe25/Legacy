//! Individual dependent care and physical meal delivery.
pub use game_dependent_care::DependentCare;
use game_dependent_care::{FamilyView, Feelings};
use game_economy_person::{
    Person, Rules,
    travel::{Colocation, Travel},
};
use game_families::Families;
use game_lots::{Movement, Place, Transaction, View};
use game_opinions::Opinions;
use game_production_model::{Good, PersonSnapshot};
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
    dependents_at(care, family, None)
}

pub fn dependents_at(care: &DependentCare, family: &Families, now: Option<u128>) -> Vec<u64> {
    let mut ids: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| now.map_or(m.age, |now| family.age_at(m, now)) < care.rules.dependent_until)
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
    pub uses: Vec<game_economy_claims::Use>,
    pub draft: game_dependent_care::Draft,
}
/// Host-provided physical access, independent of social authority.
pub struct Access<'a> {
    pub contact: &'a (dyn Fn(u64, u64) -> bool + Sync),
    pub local_stock: &'a (dyn Fn(u64) -> bool + Sync),
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
    pub claims: Option<&'a game_economy_claims::Index<'a>>,
    pub busy: &'a [u64],
    pub access: Access<'a>,
    pub eating: &'a [(u64, u32)],
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        self.prepare_at(base, None)
    }

    pub fn prepare_at(&self, base: u64, now: Option<u128>) -> Result<Plan, String> {
        let current_age = |member: &game_families::Member| {
            now.map_or(member.age, |now| self.family.age_at(member, now))
        };
        let mut family = FamilyView::compile(&self.family.members).map_err(str::to_owned)?;
        if now.is_some() && self.family.elapsed_lifetimes.is_some() {
            family = family.with_current_age(&current_age);
        }
        let opinions = self.opinions.map(Opinions::index);
        let locations = Colocation::new(self.travel);
        let colocated = |a: usize, b: usize| {
            if !(self.access.contact)(self.people[a].0, self.people[b].0) {
                return false;
            }
            locations
                .region(
                    (self.people[a].0, self.people[a].1),
                    (self.people[b].0, self.people[b].1),
                )
                .is_some()
        };
        let feelings = |person, child| {
            opinions.as_ref().map_or_else(Feelings::default, |index| {
                let (fear, obligation) = index.feelings(person, child);
                Feelings {
                    affinity: index.affinity(person, child),
                    fear,
                    obligation,
                }
            })
        };
        let draft = self.care.prepare_with_preferences(
            &family,
            self.family.rules.adult_age,
            self.people,
            self.busy,
            (self.seed, self.tick),
            (colocated, feelings),
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
                let caregiver = event.caregiver?;
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
                    .then(|| self.view.container(Place::Person(caregiver)))
                    .flatten()
                    .and_then(|holder| {
                        holder.goods.iter().find_map(|item| {
                            edible
                                .get(&item.good)
                                .filter(|&&portion| item.quantity >= portion && fits(portion))
                                .map(|&portion| (Place::Person(caregiver), item.good, portion))
                        })
                    });
                let local = || {
                    if !(self.access.local_stock)(caregiver) {
                        return None;
                    }
                    let region = child.1?;
                    let first = stocks.partition_point(|row| row.0 < region);
                    let last = stocks.partition_point(|row| row.0 <= region);
                    stocks[first..last]
                        .iter()
                        .find(|row| {
                            fits(row.2)
                                && (draft.disregards_claims(child.0)
                                    || self
                                        .claims
                                        .is_none_or(|index| index.allows(caregiver, region, row.1)))
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
                Some((child.0, caregiver, from, good, quantity))
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
                        batch: None,
                        item: None,
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
                    Some(game_economy_claims::Use {
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
    /// Stage the health effects separately from publishing care receipts.
    pub fn apply_health(&self, neglect_damage: u32, people: &mut [Person]) {
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
                person.health = person.health.saturating_sub(neglect_damage);
            }
        });
    }

    pub fn publish(self, care: &mut DependentCare) {
        care.events = self.draft.events;
    }

    pub fn apply(self, care: &mut DependentCare, people: &mut [Person]) {
        self.apply_health(care.rules.neglect_damage, people);
        self.publish(care);
    }
}

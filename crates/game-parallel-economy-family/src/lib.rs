//! Apply birth and mortality to staged people and their existing travel.
use game_parallel_economy_person::{Person, travel::Travel};
pub use game_parallel_families::Families;
use game_parallel_families::LifeInput;
use game_parallel_family_model::PhysicalView;
use rayon::prelude::*;

#[macro_export]
macro_rules! validate_custody {
    ($people:expr, $view:expr, $ids:expr, $physical:expr) => {
        if $people.par_iter().any(|person| $view.container(game_parallel_lots::Place::Person(person.0)).is_none())
            || $view.containers.par_iter().any(|row| matches!(row.place, game_parallel_lots::Place::Person(id) if !$ids.contains(&id))) {
            return Err("personal custody differs from people".into());
        }
        if $physical.work().par_iter().any(|row| !$ids.contains(&row.job.worker)) {
            return Err("physical work references an unknown person".into());
        }
    };
}

#[macro_export]
macro_rules! prepare_provisioning {
    ($world:expr, $view:expr, $busy:expr, $base:expr, $claims:expr, $choice:expr) => {{
        let mut pickup_busy = $busy.clone();
        if let Some((care, family)) = $world.care.as_ref().zip($world.family.as_ref()) {
            game_parallel_economy_work::reserve(
                &mut pickup_busy,
                game_parallel_economy_work::care::dependents(care, family),
            );
        }
        let (pickup, mut uses) = game_parallel_economy_claims::prepare_pickup!(
            $world,
            $view,
            pickup_busy,
            $base,
            $claims
        );
        $base = $base
            .checked_add(pickup.actions.len() as u64)
            .ok_or("action count overflow")?;
        let transfer = game_parallel_economy_claims::prepare_manual_transfer!(
            $world, $choice, $view, $base, $claims, uses
        );
        game_parallel_economy_work::reserve(&mut $busy, pickup.actors.iter().copied());
        (pickup, uses, transfer)
    }};
}

fn inputs(people: &[Person]) -> Vec<LifeInput> {
    people
        .par_iter()
        .map(|p| LifeInput {
            id: p.id,
            region: p.region,
            health: p.health,
            hunger: p.hunger,
        })
        .collect()
}
pub fn validate(
    family: &Families,
    people: &[Person],
    tick: u64,
    max_health: u32,
) -> Result<(), String> {
    family
        .validate_committed(&inputs(people), tick, max_health)
        .map_err(str::to_owned)
}
#[macro_export]
macro_rules! validate_care {
    ($state:expr, $physical:expr, $ids:expr) => {
        if let Some(family) = &$state.family {
            $crate::validate(
                family,
                &$state.people,
                $physical.tick(),
                $state.rules.max_health,
            )?;
            game_parallel_economy_work::validate_age(
                &game_parallel_economy_work::underage(family),
                $physical.work(),
            )?;
        }
        if let Some(care) = &$state.care {
            game_parallel_economy_work::care::validate(
                care,
                $state.family.as_ref(),
                &$ids,
                $physical.tick(),
            )?;
        }
    };
}
#[macro_export]
macro_rules! prepare_care {
    ($world:expr, $people:expr, $view:expr, $claims:expr, $busy:expr, $meals:expr, $base:expr, $tick:expr) => {{
        let care = $world
            .care
            .as_ref()
            .zip($world.family.as_ref())
            .map(|(care, family)| {
                game_parallel_economy_work::care::Context {
                    care,
                    family,
                    people: &$people,
                    rules: &$world.rules,
                    goods: $world.physical.goods(),
                    view: &$view,
                    travel: $world.travel.as_ref(),
                    opinions: $world.opinions.as_ref(),
                    claims: $claims.as_ref(),
                    busy: &$busy,
                    eating: &$meals,
                    seed: $world.seed,
                    tick: $tick,
                }
                .prepare($base)
            })
            .transpose()?;
        if let Some(plan) = &care {
            $base = $base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
            game_parallel_economy_work::reserve(&mut $busy, plan.actors.iter().copied());
        }
        care
    }};
}
#[macro_export]
macro_rules! advance_world {
    ($world:expr, $person:expr, $travel:expr, $care:expr, $physical:expr, $dynamics:expr, $campaign:expr, $tick:expr, $skills:expr, $knowledge:expr) => {{
        game_parallel_economy_campaign::apply_strain($campaign.as_ref(), &mut $person.people);
        let cost_family = game_parallel_economy_campaign::family_with_cost(
            $campaign.as_ref(),
            $world.family.as_ref(),
        )?;
        let family = cost_family
            .as_ref()
            .or($world.family.as_ref())
            .map(|family| {
                let next = $crate::advance_validated(
                    family,
                    &mut $person.people,
                    &mut $travel,
                    $world.seed,
                    $tick,
                    $care.is_some(),
                )?;
                if let Some(care) = &mut $care {
                    game_parallel_economy_work::care::inherit(care, &next.family, &next.newborns);
                }
                $crate::enroll_dynamics(&next.family, &next.newborns, &mut $dynamics);
                game_parallel_economy_skills::inherit(
                    &mut $skills,
                    &next.family,
                    &next.newborns,
                    $tick,
                )?;
                game_parallel_economy_knowledge::inherit(
                    &mut $knowledge,
                    &next.family,
                    &next.newborns,
                );
                let holders = next
                    .newborns
                    .iter()
                    .map(|&id| (id, $world.rules.carrying_capacity))
                    .collect::<Vec<_>>();
                $world.physical.add_people_in(&mut $physical, &holders)?;
                Ok::<_, String>(next.family)
            })
            .transpose()?;
        let campaign = game_parallel_economy_campaign::finish(
            $campaign,
            family.as_ref(),
            &mut $person.people,
            $tick,
        );
        (family, campaign)
    }};
}
pub struct ResultFrame {
    pub family: Families,
    pub newborns: Vec<u64>,
}
/// New people inherit only a personal inclination, never another person's relationships.
pub fn enroll_dynamics(
    family: &Families,
    births: &[u64],
    dynamics: &mut Option<game_parallel_economy_dynamics::Dynamics>,
) {
    if !family.rules.enroll_births || births.is_empty() {
        return;
    }
    let Some(dynamics) = dynamics.as_mut() else {
        return;
    };
    let mut members: Vec<_> = family.members.par_iter().collect();
    members.par_sort_unstable_by_key(|member| member.id);
    if let Some(personality) = &mut dynamics.personality {
        let inherited: Vec<_> = births
            .par_iter()
            .filter_map(|id| {
                let member = members
                    .binary_search_by_key(id, |m| m.id)
                    .ok()
                    .map(|i| members[i])?;
                let mut parents = [0; 2];
                let count = member.parents.len().min(2);
                parents[..count].copy_from_slice(&member.parents[..count]);
                Some((*id, parents, count))
            })
            .collect();
        personality.enroll_births(&inherited);
    }
    let Some(social) = dynamics.social.as_mut() else {
        return;
    };
    let mut participants: Vec<_> = social
        .participants
        .par_iter()
        .map(|p| (p.person, p.willing))
        .collect();
    participants.par_sort_unstable_by_key(|p| p.0);
    let new: Vec<_> = births
        .par_iter()
        .map(|&person| {
            let willing = members
                .binary_search_by_key(&person, |m| m.id)
                .ok()
                .and_then(|i| {
                    members[i].parents.iter().find_map(|parent| {
                        participants
                            .binary_search_by_key(parent, |p| p.0)
                            .ok()
                            .map(|i| participants[i].1)
                    })
                })
                .unwrap_or(true);
            game_parallel_economy_dynamics::participant(person, willing)
        })
        .collect();
    social.participants.par_extend(new);
}
struct View<'a> {
    people: &'a [Person],
    dense: Option<u64>,
}
impl<'a> View<'a> {
    fn new(people: &'a [Person]) -> Self {
        let dense = people.first().and_then(|first| {
            people.last().and_then(|last| {
                (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64))
                    .then_some(first.id)
            })
        });
        Self { people, dense }
    }
    fn index(&self, id: u64) -> Option<usize> {
        match self.dense {
            Some(first) => id
                .checked_sub(first)
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < self.people.len()),
            None => self.people.binary_search_by_key(&id, |p| p.id).ok(),
        }
    }
}
impl PhysicalView for View<'_> {
    fn read(&self, id: u64) -> LifeInput {
        let p = &self.people[self.index(id).expect("validated family person")];
        LifeInput {
            id: p.id,
            region: p.region,
            health: p.health,
            hunger: p.hunger,
        }
    }
    fn controlled(&self, id: u64) -> bool {
        self.index(id).is_some_and(|i| self.people[i].controlled)
    }
}
/// Newborn custody is staged separately by the physical owner before commit.
pub fn advance(
    family: &Families,
    people: &mut Vec<Person>,
    travel: &mut Option<Travel>,
    seed: u64,
    tick: u64,
    newborns_seek_meals: bool,
) -> Result<ResultFrame, String> {
    family
        .validate(
            &inputs(people),
            tick.checked_sub(1).ok_or("invalid family tick")?,
        )
        .map_err(str::to_owned)?;
    advance_validated(family, people, travel, seed, tick, newborns_seek_meals)
}
/// The host has already validated its committed family and physical identities.
pub fn advance_validated(
    family: &Families,
    people: &mut Vec<Person>,
    travel: &mut Option<Travel>,
    seed: u64,
    tick: u64,
    newborns_seek_meals: bool,
) -> Result<ResultFrame, String> {
    let next = family
        .advance_validated(&View::new(people), seed, tick)
        .map_err(str::to_owned)?;
    let mut grown: Vec<_> = next
        .families
        .members
        .par_iter()
        .filter(|m| m.born.is_some() && m.age == family.rules.adult_age)
        .map(|m| m.id)
        .collect();
    grown.par_sort_unstable();
    people.par_iter_mut().for_each(|person| {
        if next.deaths.contains(&person.id) {
            person.health = 0;
        }
        if !person.controlled && grown.binary_search(&person.id).is_ok() {
            person.seek_meals = true;
        }
    });
    let newborns: Vec<_> = next.births.par_iter().map(|birth| birth.id).collect();
    for birth in next.births {
        let willingness = people
            .binary_search_by_key(&birth.mother, |p| p.id)
            .ok()
            .is_some_and(|i| people[i].willing_to_work);
        if birth.region.is_none() {
            let travel = travel.as_mut().ok_or("traveling newborn has no journey")?;
            let index = travel
                .journeys
                .binary_search_by_key(&birth.mother, |j| j.person)
                .map_err(|_| "traveling mother has no journey")?;
            let mut journey = travel.journeys[index].clone();
            journey.person = birth.id;
            travel.journeys.push(journey);
        }
        people.push(Person {
            id: birth.id,
            region: birth.region,
            health: birth.health,
            hunger: 0,
            willing_to_work: willingness,
            seek_meals: newborns_seek_meals,
            controlled: false,
        });
    }
    people.par_sort_unstable_by_key(|p| p.id);
    if let Some(travel) = travel {
        travel.journeys.par_sort_unstable_by_key(|j| j.person);
    }
    Ok(ResultFrame {
        family: next.families,
        newborns,
    })
}

#[macro_export]
macro_rules! prepare_people {
    ($world:expr, $view:expr, $request:expr, $work:expr, $meal:expr, $physical:expr) => {{
        game_parallel_economy_combat::validate_manual!(
            $world.combat,
            $request,
            $work,
            $meal,
            $physical
        );
        game_parallel_economy_interaction::validate_work_world!($world, $work);
        let fighters = $world
            .combat
            .as_ref()
            .map_or(&[][..], |combat| combat.actors());
        let people: Vec<game_parallel_production_model::PersonSnapshot> = $world
            .people
            .par_iter()
            .map(|row| (row.id, row.region, row.health, row.hunger))
            .collect();
        let (candidates, meals) = game_parallel_economy_person::meal::prepare(
            &$world.people,
            &$world.rules,
            $world.physical.goods(),
            &$view,
            $meal,
            fighters,
        )?;
        let mut busy = game_parallel_economy_work::initial_busy(
            &$world.people,
            &candidates,
            $world.physical.work(),
        );
        let resting =
            game_parallel_economy_fatigue::resting($world.fatigue.as_ref(), &$world.people);
        if !resting.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, resting.iter().copied());
        }
        if !fighters.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, fighters.iter().copied());
        }
        (fighters, resting, people, candidates, meals, busy)
    }};
}
#[macro_export]
macro_rules! work_rates {
    ($world:expr, $meals:expr, $work:expr, $fighters:expr, $resting:expr) => {{
        let (mut rates, mut abandon) =
            game_parallel_economy_skills::continuing_work!($world, $meals);
        game_parallel_economy_interaction::work_rate!($world, $work, rates);
        game_parallel_economy_combat::interrupt(
            $fighters,
            $world.physical.work(),
            &mut rates,
            &mut abandon,
        );
        game_parallel_economy_fatigue::pause_rates(&$resting, $world.physical.work(), &mut rates);
        (rates, abandon)
    }};
}
#[macro_export]
macro_rules! stage_combat {
    ($world:expr, $people:expr, $person:expr, $tick:expr, $view:expr) => {{
        let combat = $world
            .combat
            .as_ref()
            .map(|combat| combat.prepare(&$people, &$world.people, $tick, $world.seed, &$view));
        if let Some(combat) = &combat {
            combat.apply(&mut $person.people);
        }
        combat
    }};
}

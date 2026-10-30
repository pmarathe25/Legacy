//! Atomic staging of family-enabled physical and lifecycle owners.
pub mod enrollment;
use game_parallel_agriculture::{self as agriculture, Agriculture};
use game_parallel_cargo::Cargo;
use game_parallel_families::{Families, LifeInput};
use game_parallel_logistics::{self as logistics_phase, Shipment};
use game_parallel_people::{Person, Region, Rules, ecology, people};
use game_parallel_transport::{self as transport, Traveler};
use rayon::prelude::*;
use std::collections::BTreeMap;
pub struct Context<'a> {
    pub regions: &'a mut Vec<Region>,
    pub travelers: &'a mut Vec<Traveler>,
    pub shipments: &'a mut Vec<Shipment>,
    pub agriculture: &'a mut Option<Agriculture>,
    pub cargo: &'a mut Option<Cargo>,
    pub families: &'a mut Option<Families>,
    pub rules: &'a Rules,
    pub seed: u64,
    pub tick: u64,
}
fn inputs(regions: &[Region], travelers: &[Traveler]) -> Vec<LifeInput> {
    regions
        .par_iter()
        .flat_map(|r| {
            r.people.par_iter().map(|p| LifeInput {
                id: p.id,
                region: Some(r.id),
                health: p.health,
                hunger: p.hunger,
            })
        })
        .chain(travelers.par_iter().map(|t| LifeInput {
            id: t.person.id,
            region: None,
            health: t.person.health,
            hunger: t.person.hunger,
        }))
        .collect()
}
pub fn validate(
    families: Option<&Families>,
    regions: &[Region],
    travelers: &[Traveler],
    tick: u64,
    max_health: u32,
) -> Result<(), String> {
    if let Some(families) = families {
        families.validate_committed(&inputs(regions, travelers), tick, max_health)?;
    }
    Ok(())
}
pub fn step(world: &mut Context<'_>, next: u64, busy: &[u64]) -> Result<(), &'static str> {
    if world.families.is_none() {
        if let Some(agriculture) = world.agriculture {
            agriculture::advance_excluding(
                world.regions,
                agriculture,
                world.rules.food_capacity,
                busy,
            );
        }
        game_parallel_physical_host::Context {
            regions: world.regions,
            travelers: world.travelers,
            shipments: world.shipments,
            rules: world.rules,
            cargo: world.cargo.as_mut(),
            seed: world.seed,
            tick: world.tick,
        }
        .advance();
        return Ok(());
    }
    // Prepare mutable physical owner buffers without cloning immutable rules,
    // routes, or the family owner. Nothing is published on an error.
    let mut regions: Vec<Region> = world.regions.par_iter().cloned().collect();
    let mut travelers = world.travelers.clone();
    let mut shipments = world.shipments.clone();
    let mut farming = world.agriculture.clone();
    let mut cargo = world
        .cargo
        .as_ref()
        .filter(|c| c.auto_eat_travel && !travelers.is_empty())
        .cloned();
    if let Some(agriculture) = &mut farming {
        agriculture::advance_excluding(&mut regions, agriculture, world.rules.food_capacity, busy);
    }
    regions.par_iter_mut().for_each(|region| {
        ecology::grow(region, world.rules);
        people::advance(region, world.rules, world.seed, world.tick);
    });
    transport::advance(&mut regions, &mut travelers, world.rules, cargo.as_mut());
    logistics_phase::advance(&mut regions, &mut shipments, world.rules);
    let families = world.families.as_ref().unwrap();
    if families.rules.newborn_health > world.rules.max_health {
        return Err("newborn health exceeds maximum");
    }
    let draft = families.advance(&inputs(&regions, &travelers), world.seed, next)?;
    let indices: BTreeMap<_, _> = regions
        .par_iter()
        .enumerate()
        .map(|(i, r)| (r.id, i))
        .collect();
    let mut newborns: Vec<Vec<Person>> = vec![Vec::new(); regions.len()];
    let traveling: BTreeMap<_, _> = travelers.iter().map(|t| (t.person.id, t)).collect();
    let mut newborn_travelers = Vec::new();
    for birth in draft.births {
        let person = Person {
            id: birth.id,
            hunger: 0,
            health: birth.health,
        };
        if let Some(region) = birth.region {
            newborns[*indices.get(&region).ok_or("birth region missing")?].push(person);
        } else {
            let mother = traveling
                .get(&birth.mother)
                .ok_or("traveling mother missing")?;
            newborn_travelers.push(Traveler {
                person,
                source: mother.source,
                destination: mother.destination,
                ticks_remaining: mother.ticks_remaining,
            });
        }
    }
    regions
        .par_iter_mut()
        .zip(newborns.into_par_iter())
        .for_each(|(r, newborns)| {
            for p in &mut r.people {
                if draft.deaths.contains(&p.id) {
                    p.health = 0;
                }
            }
            r.people.extend(newborns);
        });
    travelers.par_iter_mut().for_each(|t| {
        if draft.deaths.contains(&t.person.id) {
            t.person.health = 0;
        }
    });
    travelers.extend(newborn_travelers);
    travelers.sort_unstable_by_key(|t| t.person.id);
    let people = inputs(&regions, &travelers);
    draft
        .families
        .validate_committed(&people, next, world.rules.max_health)
        .map_err(|_| "invalid staged family state")?;
    *world.regions = regions;
    *world.travelers = travelers;
    *world.shipments = shipments;
    *world.agriculture = farming;
    if let Some(cargo) = cargo {
        *world.cargo = Some(cargo);
    }
    *world.families = Some(draft.families);
    Ok(())
}

/// Enroll births only after all owner drafts for the same tick have published.
#[macro_export]
macro_rules! enroll_births {
    ($world:expr) => {{
        let world = $world;
        $crate::enrollment::Context {
            families: world.families.as_ref(),
            social: &mut world.social,
            knowledge: &mut world.knowledge,
            autonomy: &mut world.autonomy,
            tick: world.tick,
        }
        .enroll();
    }};
}
/// Reject underage work requests against editable family data.
pub fn check_work_age(
    families: Option<&Families>,
    workers: impl Iterator<Item = u64>,
) -> Result<(), String> {
    let Some(families) = families.filter(|families| families.rules.min_work_age > 0) else {
        return Ok(());
    };
    let underage: std::collections::HashSet<_> = families
        .members
        .iter()
        .filter(|member| member.age < families.rules.min_work_age)
        .map(|member| member.id)
        .collect();
    if workers.into_iter().any(|person| underage.contains(&person)) {
        return Err("person is too young for work".into());
    }
    Ok(())
}

/// Host method for an individual's changeable relationship disposition.
#[macro_export]
macro_rules! partnership_preference_method {
    () => {
        pub fn set_partnership_reconsideration(
            &mut self,
            person: u64,
            enabled: bool,
        ) -> Result<(), String> {
            self.edit_world(|world| {
                if world
                    .lineage
                    .as_ref()
                    .and_then(|lineage| lineage.suspended_person(world.tick))
                    == Some(person)
                {
                    return Err("player control is suspended".into());
                }
                let family = world.families.as_mut().ok_or("families are disabled")?;
                let member = family
                    .members
                    .iter_mut()
                    .find(|member| member.id == person && member.alive)
                    .ok_or("person must be alive")?;
                member.reconsider_partnership = enabled;
                Ok(())
            })
        }
    };
}

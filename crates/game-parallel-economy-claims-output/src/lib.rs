//! Personal claims prompted only by physical outputs that actually arrive.
use game_parallel_economy_agriculture::Plan;
use game_parallel_economy_claims::{Claim, Claims};
use game_parallel_economy_person::Person;
use game_parallel_lot_runtime::{Events, Work};
use game_parallel_lots::{Place, View};
use rayon::prelude::*;

#[macro_export]
macro_rules! prepare_farming {
    ($world:expr, $busy:expr, $view:expr, $tick:expr, $base:expr, $claims:expr, $choice:expr) => {{
        let manual = match $choice {
            Some(game_parallel_economy_interaction::PhysicalChoice::Farm { person, field }) => {
                Some((person, field))
            }
            _ => None,
        };
        if let Some((person, _)) = manual {
            if $world.agriculture.is_none()
                || $world.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                })
                || $world.physical.work().iter().any(|w| {
                    w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
                })
            {
                return Err("person cannot cultivate now".into());
            }
        }
        let plan = $world
            .agriculture
            .as_ref()
            .map(|agriculture| {
                game_parallel_economy_agriculture::Context {
                    agriculture,
                    people: &$world.people,
                    opinions: $world.opinions.as_ref(),
                    claims: $claims.as_ref(),
                    busy: &$busy,
                    goods: $world.physical.goods(),
                    view: &$view,
                    tick_millis: $world.physical.tick_millis(),
                    tick: $tick,
                    manual,
                    season: $world.environment.as_ref().and_then(|e| e.season($tick)),
                }
                .prepare($base)
            })
            .transpose()?;
        if let Some(plan) = &plan {
            game_parallel_economy_work::reserve(&mut $busy, plan.actors.iter().copied());
        }
        let output = $crate::prepare(
            plan.as_ref(),
            $world.physical.work(),
            &$view,
            &$world.people,
            $world.dynamics.as_ref().and_then(|d| d.claims.as_ref()),
        );
        (plan, output)
    }};
}
#[macro_export]
macro_rules! publish {
    ($dynamics:expr, $output:expr, $events:expr) => {
        if let Some(claims) = $dynamics.as_mut().and_then(|d| d.claims.as_mut()) {
            $crate::publish(claims, $output, $events);
        }
    };
}
pub struct Assertion {
    id: u64,
    job: bool,
    claim: Claim,
}
/// A gathered output is claimable only by the person who actually gathered it.
pub fn gather_outputs(
    outputs: &[(u64, u64, u64, u32)],
    people: &[Person],
    claims: Option<&Claims>,
) -> Vec<Assertion> {
    let Some(claims) = claims else {
        return Vec::new();
    };
    let mut willing: Vec<_> = claims
        .preferences
        .par_iter()
        .filter(|p| p.assert_outputs)
        .map(|p| p.person)
        .collect();
    willing.par_sort_unstable();
    outputs
        .par_iter()
        .filter_map(|&(id, person, region, good)| {
            (willing.binary_search(&person).is_ok()
                && people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0 && people[i].region == Some(region)))
            .then_some(Assertion {
                id,
                job: false,
                claim: Claim {
                    person,
                    region,
                    good,
                },
            })
        })
        .collect()
}
pub fn prepare(
    farming: Option<&Plan>,
    work: &[Work],
    view: &View,
    people: &[Person],
    claims: Option<&Claims>,
) -> Vec<Assertion> {
    let Some(claims) = claims else {
        return Vec::new();
    };
    let mut willing: Vec<_> = claims
        .preferences
        .par_iter()
        .filter(|p| p.assert_outputs)
        .map(|p| p.person)
        .collect();
    if willing.is_empty() {
        return Vec::new();
    }
    willing.par_sort_unstable();
    let eligible = |person: u64, region: u64| {
        willing.binary_search(&person).is_ok()
            && people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_some_and(|i| people[i].health > 0 && people[i].region == Some(region))
    };
    let mut actors: Vec<_> = farming.map_or_else(Vec::new, |plan| {
        plan.changes
            .par_iter()
            .zip(plan.actors.par_iter())
            .map(|(change, &actor)| (change.1, actor))
            .collect()
    });
    actors.par_sort_unstable_by_key(|a| a.0);
    let mut prepared: Vec<_> = farming.map_or_else(Vec::new, |plan| {
        plan.harvest
            .par_iter()
            .filter_map(|action| {
                let actor = actors
                    .binary_search_by_key(&action.id, |a| a.0)
                    .ok()
                    .map(|i| actors[i].1)?;
                let creation = action.creations.first()?;
                let Place::Stock { region, good } = creation.to else {
                    return None;
                };
                eligible(actor, region).then_some(Assertion {
                    id: action.id,
                    job: false,
                    claim: Claim {
                        person: actor,
                        region,
                        good,
                    },
                })
            })
            .collect()
    });
    prepared.par_extend(work.par_iter().flat_map_iter(|row| {
        let job = &row.job;
        let holder = view.container(Place::JobOutput(job.id));
        holder.into_iter().flat_map(move |holder| {
            holder.goods.iter().filter_map(move |balance| {
                eligible(job.worker, job.region).then_some(Assertion {
                    id: job.id,
                    job: true,
                    claim: Claim {
                        person: job.worker,
                        region: job.region,
                        good: balance.good,
                    },
                })
            })
        })
    }));
    prepared
}
pub fn publish(claims: &mut Claims, prepared: Vec<Assertion>, events: &Events) {
    let additions: Vec<_> = prepared
        .into_par_iter()
        .filter(|row| {
            if row.job {
                events.delivered.binary_search(&row.id).is_ok()
            } else {
                events
                    .physical_actions
                    .binary_search_by_key(&row.id, |r| r.id)
                    .ok()
                    .is_some_and(|i| events.physical_actions[i].accepted)
            }
        })
        .map(|row| row.claim)
        .collect();
    if additions.is_empty() {
        return;
    }
    let mut all = std::mem::take(&mut claims.claims);
    all.par_extend(additions);
    all.par_sort_unstable_by_key(|c| (c.region, c.good, c.person));
    claims.claims = all
        .par_chunk_by(|a, b| (a.region, a.good, a.person) == (b.region, b.good, b.person))
        .map(|group| group[0].clone())
        .collect();
}

#[macro_export]
macro_rules! farm_method {
    () => {
        /// Attempt one chosen local cultivation action against real inputs and capacity.
        pub fn farm_field(&mut self, person: u64, field: u64) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let before = self
                .agriculture
                .as_ref()
                .and_then(|a| a.fields.iter().find(|f| f.id == field))
                .map(|f| f.phase)
                .ok_or("unknown field")?;
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some(game_parallel_economy_interaction::PhysicalChoice::Farm { person, field }),
            )?;
            self.commit(draft)?;
            Ok(self
                .agriculture
                .as_ref()
                .and_then(|a| a.fields.iter().find(|f| f.id == field))
                .is_some_and(|f| f.phase != before))
        }
    };
}

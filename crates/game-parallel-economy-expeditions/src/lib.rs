//! Personal expeditions use the same travel and physical people as the economy.
use game_parallel_economy_person::{
    Person,
    travel::{Departure, Travel},
};
use game_parallel_lot_runtime::Work;
use game_parallel_lots::{Place, View};
use game_parallel_missions::{Draft, Event, EventKind, Phase, routing::Routing};
pub use game_parallel_missions::{Mission, Missions, Objective, Phase as MissionPhase, Start};
use game_parallel_production_model::PersonSnapshot;
use game_parallel_transport::Route;
use rayon::prelude::*;
use std::collections::HashSet;

fn routes(travel: &Travel) -> Result<Vec<Route>, String> {
    travel
        .routes
        .par_iter()
        .map(|r| {
            Ok(Route {
                source: r.source,
                destination: r.destination,
                ticks: u32::try_from(r.ticks).map_err(|_| "expedition route is too long")?,
            })
        })
        .collect()
}
pub fn validate(
    owner: Option<&Missions>,
    travel: Option<&Travel>,
    people: &HashSet<u64>,
    view: &View,
    adults: Option<&HashSet<u64>>,
    tick: u64,
) -> Result<(), String> {
    let Some(owner) = owner else {
        return Ok(());
    };
    let travel = travel.ok_or("expeditions require travel")?;
    let routes = routes(travel)?;
    let regions = view
        .containers
        .par_iter()
        .filter_map(|row| match row.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect();
    owner
        .validate(people, &regions, &routes, tick)
        .map_err(str::to_owned)?;
    if let Some(adults) = adults {
        owner.validate_adults(adults).map_err(str::to_owned)?;
    }
    Ok(())
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $physical:expr, $view:expr, $ids:expr) => {{
        let adults = $state.family.as_ref().map(|family| {
            family
                .members
                .iter()
                .filter(|m| m.age >= family.rules.adult_age)
                .map(|m| m.id)
                .collect()
        });
        $crate::validate(
            $state.missions.as_ref(),
            $state.travel.as_ref(),
            &$ids,
            &$view,
            adults.as_ref(),
            $physical.tick(),
        )?;
    }};
}
pub fn start(
    owner: &Missions,
    travel: &Travel,
    people: &[Person],
    request: Start,
) -> Result<(Missions, u64), String> {
    let person = people
        .binary_search_by_key(&request.person, |p| p.id)
        .ok()
        .map(|i| &people[i])
        .ok_or("unknown person")?;
    let origin = person
        .region
        .filter(|_| person.health > 0)
        .ok_or("person must be living and local")?;
    if request.visit_ticks == 0 || origin == request.destination {
        return Err("invalid expedition destination or visit".into());
    }
    let routes = routes(travel)?;
    let paths = Routing::new(&routes, &[origin, request.destination]);
    if paths.next(origin, request.destination).is_none()
        || paths.next(request.destination, origin).is_none()
    {
        return Err("expedition requires routes both ways".into());
    }
    if owner
        .entries
        .iter()
        .any(|m| m.person == request.person && !matches!(m.phase, Phase::Completed | Phase::Failed))
    {
        return Err("person already has an active expedition".into());
    }
    let mut next = owner.clone();
    let id = next.next_id;
    next.next_id = id.checked_add(1).ok_or("expedition ID overflow")?;
    next.entries.push(Mission {
        id,
        person: request.person,
        party: None,
        party_opt_in: false,
        party_compliant: false,
        origin,
        destination: request.destination,
        visit_ticks: request.visit_ticks,
        remaining: 0,
        phase: Phase::Ready,
        objective: None,
        objective_met: false,
    });
    Ok((next, id))
}
pub fn cancel(owner: &Missions, person: u64, id: u64, tick: u64) -> Result<Missions, String> {
    let mut next = owner.clone();
    let mission = next
        .entries
        .iter_mut()
        .find(|m| m.id == id && m.person == person)
        .ok_or("unknown personal expedition")?;
    if matches!(mission.phase, Phase::Completed | Phase::Failed) {
        return Err("expedition is already finished".into());
    }
    mission.phase = Phase::Failed;
    mission.remaining = 0;
    next.events.push(Event {
        tick,
        mission: id,
        person,
        kind: EventKind::Cancelled,
    });
    Ok(next)
}
pub struct Invitation<'a> {
    pub owner: &'a Missions,
    pub travel: &'a Travel,
    pub people: &'a [Person],
    pub work: &'a [Work],
    pub social: Option<&'a game_parallel_social::Social>,
    pub opinions: Option<&'a game_parallel_opinions::Opinions>,
}
impl Invitation<'_> {
    /// A person's own willingness or directed fear and obligation determines their response.
    pub fn invite(
        &self,
        actor: u64,
        id: u64,
        target: u64,
    ) -> Result<Option<(Missions, u64)>, String> {
        if actor == target {
            return Err("cannot invite oneself".into());
        }
        let mission = self
            .owner
            .entries
            .iter()
            .find(|m| m.id == id && m.person == actor)
            .ok_or("unknown personal expedition")?;
        if mission.phase != Phase::Ready || mission.party.is_some() {
            return Err("invitation requires a ready individual expedition".into());
        }
        let find = |id| {
            self.people
                .binary_search_by_key(&id, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
        };
        let a = find(actor).ok_or("unknown inviter")?;
        let b = find(target).ok_or("unknown invitee")?;
        let working = game_parallel_economy_person::travel::working(self.work);
        if a.health == 0
            || b.health == 0
            || b.controlled
            || a.region != Some(mission.origin)
            || b.region != Some(mission.origin)
            || working.binary_search(&actor).is_ok()
            || working.binary_search(&target).is_ok()
        {
            return Err("participants must be living, local and available".into());
        }
        let willing = self.social.is_some_and(|social| {
            b.health >= social.rules.min_health
                && b.hunger <= social.rules.max_hunger
                && social
                    .participants
                    .iter()
                    .any(|p| p.person == target && p.willing)
        });
        let pressured = self
            .owner
            .pressure_threshold
            .zip(self.opinions)
            .is_some_and(|(threshold, opinions)| {
                let (fear, obligation) = opinions.index().feelings(target, actor);
                u128::from(fear) + u128::from(obligation) >= u128::from(threshold)
            });
        if !willing && !pressured {
            return Ok(None);
        }
        let (mut next, target_id) = start(
            self.owner,
            self.travel,
            self.people,
            Start {
                person: target,
                destination: mission.destination,
                visit_ticks: mission.visit_ticks,
            },
        )?;
        let party = id.min(target_id);
        for row in &mut next.entries {
            if row.id == id || row.id == target_id {
                row.party = Some(party);
                row.party_opt_in = row.id == id || willing;
                row.party_compliant = row.id == target_id && !willing;
            }
        }
        Ok(Some((next, target_id)))
    }
}
#[macro_export]
macro_rules! mission_methods {
    () => {
        pub fn start_mission(&mut self, request: $crate::Start) -> Result<u64, String> {
            if self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .find(|m| m.id == request.person)
                    .is_none_or(|m| m.age < f.rules.adult_age)
            }) {
                return Err("a child cannot undertake an unaccompanied expedition".into());
            }
            let (missions, id) = $crate::start(
                self.missions.as_ref().ok_or("expeditions are disabled")?,
                self.travel.as_ref().ok_or("travel is disabled")?,
                &self.people,
                request,
            )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.missions = Some(missions);
            self.epoch = epoch;
            Ok(id)
        }
        pub fn cancel_mission(&mut self, person: u64, id: u64) -> Result<(), String> {
            let missions = $crate::cancel(
                self.missions.as_ref().ok_or("expeditions are disabled")?,
                person,
                id,
                self.tick(),
            )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.missions = Some(missions);
            self.epoch = epoch;
            Ok(())
        }
        pub fn set_mission_meeting(
            &mut self,
            person: u64,
            id: u64,
            target: u64,
        ) -> Result<(), String> {
            if person == target
                || !self.people.iter().any(|p| p.id == target)
                || self
                    .dynamics
                    .as_ref()
                    .and_then(|d| d.social.as_ref())
                    .is_none()
            {
                return Err("meeting requires another person and social encounters".into());
            }
            let missions = self.missions.as_mut().ok_or("expeditions are disabled")?;
            let mission = missions
                .entries
                .iter_mut()
                .find(|m| m.id == id && m.person == person)
                .ok_or("unknown personal expedition")?;
            if mission.phase != $crate::MissionPhase::Ready {
                return Err("objective must be set before departure".into());
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            mission.objective = Some($crate::Objective::Meet { person: target });
            mission.objective_met = false;
            self.epoch = epoch;
            Ok(())
        }
        pub fn invite_mission(
            &mut self,
            actor: u64,
            id: u64,
            target: u64,
        ) -> Result<Option<u64>, String> {
            if self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .find(|m| m.id == target)
                    .is_none_or(|m| m.age < f.rules.adult_age)
            }) {
                return Err("a child cannot undertake an unaccompanied expedition".into());
            }
            let invitation = $crate::Invitation {
                owner: self.missions.as_ref().ok_or("expeditions are disabled")?,
                travel: self.travel.as_ref().ok_or("travel is disabled")?,
                people: &self.people,
                work: self.physical.work(),
                social: self.dynamics.as_ref().and_then(|d| d.social.as_ref()),
                opinions: self.opinions.as_ref(),
            };
            let Some((missions, target_id)) = invitation.invite(actor, id, target)? else {
                return Ok(None);
            };
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.missions = Some(missions);
            self.epoch = epoch;
            Ok(Some(target_id))
        }
    };
}
pub struct Context<'a> {
    pub owner: Option<&'a Missions>,
    pub travel: Option<&'a Travel>,
    pub people: &'a [Person],
    pub snapshots: &'a [PersonSnapshot],
    pub busy: &'a [u64],
    pub work: &'a [Work],
    pub social: Option<&'a game_parallel_social::Social>,
    pub opinions: Option<&'a game_parallel_opinions::Opinions>,
    pub tick: u64,
}
pub struct Prepared {
    pub draft: Option<Draft>,
    owner: Missions,
    stopped: Vec<Event>,
}
impl Context<'_> {
    pub fn prepare(&self) -> Result<Option<Prepared>, String> {
        let Some(owner) = self.owner else {
            return Ok(None);
        };
        if owner
            .entries
            .par_iter()
            .all(|m| matches!(m.phase, Phase::Completed | Phase::Failed))
        {
            return Ok(None);
        }
        let mut next = game_parallel_missions::clone_parallel(owner);
        let stopped = if next.entries.par_iter().any(|m| {
            m.party.is_some_and(|party| party != m.id)
                && !matches!(m.phase, Phase::Completed | Phase::Failed)
        }) {
            let mut leaders: Vec<_> = owner.entries.par_iter().map(|m| (m.id, m.person)).collect();
            leaders.par_sort_unstable_by_key(|row| row.0);
            let mut willing: Vec<_> = self.social.map_or_else(Vec::new, |social| {
                social
                    .participants
                    .par_iter()
                    .filter(|p| p.willing)
                    .map(|p| p.person)
                    .collect()
            });
            willing.par_sort_unstable();
            let feelings = self.opinions.map(|o| o.index());
            next.entries
                .par_iter_mut()
                .filter_map(|mission| {
                    if mission.party.is_none_or(|party| party == mission.id)
                        || matches!(mission.phase, Phase::Completed | Phase::Failed)
                        || (self.social.is_none() && !mission.party_compliant)
                    {
                        return None;
                    }
                    let person = self
                        .people
                        .binary_search_by_key(&mission.person, |p| p.id)
                        .ok()
                        .map(|i| &self.people[i])?;
                    if person.controlled || person.health == 0 {
                        return None;
                    }
                    if willing.binary_search(&mission.person).is_ok() {
                        mission.party_compliant = false;
                        mission.party_opt_in = true;
                        return None;
                    }
                    let leader = mission
                        .party
                        .and_then(|party| leaders.binary_search_by_key(&party, |r| r.0).ok())
                        .map(|i| leaders[i].1);
                    let pressured = owner
                        .pressure_threshold
                        .zip(feelings.as_ref())
                        .zip(leader)
                        .is_some_and(|((threshold, feelings), leader)| {
                            let (fear, obligation) = feelings.feelings(mission.person, leader);
                            u128::from(fear) + u128::from(obligation) >= u128::from(threshold)
                        });
                    if pressured {
                        mission.party_opt_in = false;
                        mission.party_compliant = true;
                        return None;
                    }
                    mission.phase = Phase::Failed;
                    mission.remaining = 0;
                    Some(Event {
                        tick: self.tick,
                        mission: mission.id,
                        person: mission.person,
                        kind: EventKind::Cancelled,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let routes = routes(self.travel.ok_or("expeditions require travel")?)?;
        let working = game_parallel_economy_person::travel::working(self.work);
        let mut controlled: Vec<_> = owner
            .entries
            .par_iter()
            .filter(|m| !matches!(m.phase, Phase::Completed | Phase::Failed))
            .filter_map(|m| {
                self.people
                    .binary_search_by_key(&m.person, |p| p.id)
                    .ok()
                    .filter(|&i| {
                        self.people[i].controlled && working.binary_search(&m.person).is_err()
                    })
                    .map(|_| m.person)
            })
            .collect();
        controlled.par_sort_unstable();
        let busy: Vec<_> = self
            .busy
            .par_iter()
            .filter(|id| controlled.binary_search(id).is_err())
            .copied()
            .collect();
        let draft = next.prepare(self.snapshots, &busy, &routes, self.tick);
        Ok(Some(Prepared {
            owner: next,
            draft,
            stopped,
        }))
    }
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr, $people:expr, $busy:expr, $tick:expr) => {{
        let plan = $crate::Context {
            owner: $world.missions.as_ref(),
            travel: $world.travel.as_ref(),
            people: &$world.people,
            snapshots: &$people,
            busy: &$busy,
            work: $world.physical.work(),
            social: $world.dynamics.as_ref().and_then(|d| d.social.as_ref()),
            opinions: $world.opinions.as_ref(),
            tick: $tick,
        }
        .prepare()?;
        if let Some(plan) = plan.as_ref().and_then(|p| p.draft.as_ref()) {
            game_parallel_economy_work::reserve(&mut $busy, plan.actors.iter().copied());
        }
        plan
    }};
}
pub fn finish(
    owner: Option<&Missions>,
    prepared: Option<Prepared>,
    travel: &mut Option<Travel>,
    people: &mut Vec<Person>,
    encounters: &[game_parallel_social::Encounter],
    tick: u64,
) -> Option<Missions> {
    let Some(prepared) = prepared else {
        return owner.map(|owner| {
            let mut next = game_parallel_missions::clone_parallel(owner);
            next.events.clear();
            next
        });
    };
    let mut next = prepared.owner;
    next.events.clear();
    let Some(draft) = prepared.draft else {
        next.events = prepared.stopped;
        return Some(next);
    };
    let mut available: Vec<_> = draft
        .departures
        .par_iter()
        .map(|d| {
            people
                .binary_search_by_key(&d.person, |p| p.id)
                .ok()
                .is_some_and(|i| people[i].health > 0 && people[i].region == Some(d.source))
        })
        .collect();
    let mut groups: Vec<_> = draft
        .departures
        .par_iter()
        .zip(available.par_iter())
        .filter_map(|(d, &ok)| d.party.map(|party| (party, ok)))
        .collect();
    groups.par_sort_unstable_by_key(|g| g.0);
    let blocked: Vec<_> = groups
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|g| g.iter().any(|r| !r.1).then_some(g[0].0))
        .collect();
    available
        .par_iter_mut()
        .zip(draft.departures.par_iter())
        .for_each(|(ok, d)| {
            if d.party
                .is_some_and(|party| blocked.binary_search(&party).is_ok())
            {
                *ok = false;
            }
        });
    let requests: Vec<_> = draft
        .departures
        .par_iter()
        .zip(available.par_iter())
        .filter_map(|(d, &ok)| {
            ok.then_some(Departure {
                person: d.person,
                destination: d.destination,
            })
        })
        .collect();
    let accepted = if requests.is_empty() {
        true
    } else if let Some(current) = travel.as_ref() {
        if let Ok((next_travel, next_people)) = current.depart(people, &requests, &[]) {
            *travel = Some(next_travel);
            *people = next_people;
            true
        } else {
            false
        }
    } else {
        false
    };
    let mut meetings = game_parallel_mission_objectives::meetings(&mut next, encounters, tick);
    draft.publish(&mut next, &available, accepted, tick);
    next.events.append(&mut meetings);
    next.events.extend(prepared.stopped);
    Some(next)
}
#[macro_export]
macro_rules! finish_world {
    ($world:expr, $draft:expr, $person:expr, $travel:expr, $dynamics:expr, $tick:expr) => {{
        let encounters = $dynamics
            .as_ref()
            .and_then(|d| d.social.as_ref())
            .map_or(&[][..], |social| social.events.as_slice());
        $crate::finish(
            $world.missions.as_ref(),
            $draft,
            &mut $travel,
            &mut $person.people,
            encounters,
            $tick,
        )
    }};
}

//! Stage and publish real personal expedition departures.
use game_parallel::{MissionPhase, Missions, Move, World};
use game_parallel_fused_data::{Block, NONE};
pub use game_parallel_missions::routing;
use rayon::prelude::*;
pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    reserved: &[u64],
    tick: u64,
) -> Option<game_parallel_missions::Draft> {
    let missions = world.missions.as_ref()?;
    let controlled = world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.controlled_person(world.tick));
    let authorized = controlled.filter(|person| {
        missions.entries.iter().any(|mission| {
            mission.person == *person
                && !matches!(
                    mission.phase,
                    MissionPhase::Completed | MissionPhase::Failed
                )
        })
    });
    let mut skip_controlled = authorized;
    let mut busy: Vec<_> = reserved
        .iter()
        .copied()
        .filter(|person| {
            if skip_controlled == Some(*person) {
                skip_controlled = None;
                false
            } else {
                true
            }
        })
        .collect();
    busy.extend(
        blocks
            .par_iter()
            .flat_map_iter(|block| {
                block
                    .ids
                    .iter()
                    .zip(&block.work)
                    .filter_map(|(&id, &work)| (work != NONE).then_some(id))
            })
            .collect::<Vec<_>>(),
    );
    let dense = positions.first().and_then(|first| {
        (positions.last()?.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
            .then_some(first.0)
    });
    missions.prepare_with(
        |person| {
            let index = dense
                .and_then(|first| person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < positions.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| positions.binary_search_by_key(&person, |row| row.0).ok())
                        .flatten()
                })?;
            let (_, block, offset) = positions[index];
            let block = &blocks[block];
            Some((
                person,
                block
                    .journey
                    .is_none()
                    .then_some(world.regions[block.region].id),
                block.health[offset],
                block.hunger[offset],
            ))
        },
        &busy,
        &world.routes,
        tick,
    )
}
pub fn requests(
    draft: &game_parallel_missions::Draft,
    blocks: &[Block],
    regions: &[game_parallel::Region],
    positions: &[(u64, usize, usize)],
) -> (Vec<Move>, Vec<bool>) {
    let dense = positions.first().and_then(|first| {
        (positions.last()?.0.checked_sub(first.0) == Some((positions.len() - 1) as u64))
            .then_some(first.0)
    });
    let mut available: Vec<_> = draft
        .departures
        .par_iter()
        .map(|departure| {
            let i = dense
                .and_then(|first| departure.person.checked_sub(first))
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&i| i < positions.len())
                .or_else(|| {
                    if dense.is_none() {
                        positions
                            .binary_search_by_key(&departure.person, |row| row.0)
                            .ok()
                    } else {
                        None
                    }
                });
            i.is_some_and(|i| {
                let (_, block, offset) = positions[i];
                let block = &blocks[block];
                block.health[offset] > 0
                    && block.journey.is_none()
                    && regions[block.region].id == departure.source
            })
        })
        .collect();
    let mut groups: Vec<_> = draft
        .departures
        .par_iter()
        .zip(available.par_iter())
        .filter_map(|(departure, &ok)| departure.party.map(|party| (party, ok)))
        .collect();
    if groups.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
        groups.par_sort_unstable_by_key(|row| row.0);
    }
    let blocked: Vec<_> = groups
        .par_chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| group.iter().any(|row| !row.1).then_some(group[0].0))
        .collect();
    available
        .par_iter_mut()
        .zip(draft.departures.par_iter())
        .for_each(|(ok, departure)| {
            if departure
                .party
                .is_some_and(|party| blocked.binary_search(&party).is_ok())
            {
                *ok = false;
            }
        });
    let moves = draft
        .departures
        .par_iter()
        .zip(available.par_iter())
        .filter_map(|(departure, &yes)| {
            yes.then_some(Move {
                person: departure.person,
                destination: departure.destination,
            })
        })
        .collect();
    (moves, available)
}
pub fn commit(
    target: &mut Option<Missions>,
    draft: game_parallel_missions::Draft,
    available: &[bool],
    result: &Result<(), String>,
    encounters: &[game_parallel::Encounter],
    tick: u64,
) {
    let state = target.as_mut().expect("prepared missions");
    let mut met = game_parallel_mission_objectives::meetings(state, encounters, tick);
    draft.publish(state, available, result.is_ok(), tick);
    state.events.append(&mut met);
}
/// Publish movement only after all fallible tick staging has succeeded.
#[macro_export]
macro_rules! publish {
    ($kernel:ident, $draft:expr) => {{
        if let Some(mission) = $draft {
            let (requests, available) = game_parallel_fused_missions::requests(
                &mission,
                &$kernel.blocks,
                &$kernel.template.regions,
                &$kernel.positions,
            );
            let result = if requests.is_empty() {
                Ok(())
            } else {
                $kernel.move_people_autonomous(&requests)
            };
            game_parallel_fused_missions::commit(
                &mut $kernel.template.missions,
                mission,
                &available,
                &result,
                $kernel
                    .template
                    .social
                    .as_ref()
                    .map_or(&[], |social| social.events.as_slice()),
                $kernel.template.tick,
            );
        } else if let Some(missions) = &mut $kernel.template.missions {
            missions.events.clear();
        }
    }};
}
/// Attach checked expedition commands to the fused host.
#[macro_export]
macro_rules! mission_methods {
    () => {
        pub fn start_mission(
            &mut self,
            request: game_parallel::MissionStart,
        ) -> Result<u64, String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(request.person)
            {
                return Err("player control is suspended".into());
            }
            game_parallel_fused_conflict::check(&self.template, &[request.person])?;
            let index = self
                .positions
                .binary_search_by_key(&request.person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            let block = &self.blocks[block];
            if block.health[slot] == 0 || block.journey.is_some() || request.visit_ticks == 0 {
                return Err("person must be living and local, with a positive visit".into());
            }
            if let Some(family) = &self.owners.family {
                let index = family
                    .store
                    .locate(request.person)
                    .ok_or("person has no family record")?;
                if family.store.state(index, false).age < family.rules.adult_age {
                    return Err("a child cannot undertake an unaccompanied expedition".into());
                }
            }
            let origin = self.template.regions[block.region].id;
            let routing = $crate::routing::Routing::new(
                &self.template.routes,
                &[origin, request.destination],
            );
            if origin == request.destination
                || routing.next(origin, request.destination).is_none()
                || routing.next(request.destination, origin).is_none()
            {
                return Err("expedition requires routes both ways".into());
            }
            let missions = self
                .template
                .missions
                .as_mut()
                .ok_or("missions are disabled")?;
            if missions.entries.iter().any(|mission| {
                mission.person == request.person
                    && !matches!(
                        mission.phase,
                        game_parallel::MissionPhase::Completed
                            | game_parallel::MissionPhase::Failed
                    )
            }) {
                return Err("person already has an active expedition".into());
            }
            let id = missions.next_id;
            let next_id = id.checked_add(1).ok_or("mission ID overflow")?;
            missions.entries.push(game_parallel::Mission {
                id,
                person: request.person,
                party: None,
                party_opt_in: false,
                party_compliant: false,
                objective: None,
                objective_met: false,
                origin,
                destination: request.destination,
                visit_ticks: request.visit_ticks,
                remaining: 0,
                phase: game_parallel::MissionPhase::Ready,
            });
            missions.next_id = next_id;
            self.revision = revision;
            Ok(id)
        }
        /// Set a personal meeting goal before leaving; meeting requires ordinary consent.
        pub fn set_mission_meeting(
            &mut self,
            person: u64,
            id: u64,
            target: u64,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if person == target
                || self
                    .positions
                    .binary_search_by_key(&target, |row| row.0)
                    .is_err()
            {
                return Err("meeting requires another known person".into());
            }
            if self.template.social.is_none() {
                return Err("meeting requires social encounters".into());
            }
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let mission = self
                .template
                .missions
                .as_mut()
                .ok_or("missions are disabled")?
                .entries
                .iter_mut()
                .find(|mission| mission.id == id && mission.person == person)
                .ok_or("unknown personal expedition")?;
            if mission.phase != game_parallel::MissionPhase::Ready {
                return Err("objective must be set before departure".into());
            }
            mission.objective = Some(game_parallel::MissionObjective::Meet { person: target });
            mission.objective_met = false;
            self.revision = revision;
            Ok(())
        }
        /// Grant or withdraw a traveler's consent before a party is formed.
        pub fn set_mission_party_consent(
            &mut self,
            person: u64,
            id: u64,
            consent: bool,
        ) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let mission = self
                .template
                .missions
                .as_mut()
                .ok_or("missions are disabled")?
                .entries
                .iter_mut()
                .find(|mission| mission.id == id && mission.person == person)
                .ok_or("unknown personal expedition")?;
            if mission.phase != game_parallel::MissionPhase::Ready || mission.party.is_some() {
                return Err("consent can be changed only before forming a party".into());
            }
            mission.party_opt_in = consent;
            self.revision = revision;
            Ok(())
        }
        /// Invite a local, willing person to join an existing ready expedition.
        /// Acceptance creates their own expedition and links both parties atomically.
        pub fn invite_mission(
            &mut self,
            actor: u64,
            mission_id: u64,
            target: u64,
        ) -> Result<Option<u64>, String> {
            if actor == target {
                return Err("cannot invite oneself".into());
            }
            if self.template.lineage.as_ref().is_some_and(|lineage| {
                lineage.current == Some(target)
                    || lineage.suspended_person(self.template.tick) == Some(actor)
            }) {
                return Err("invitation cannot control a ruler".into());
            }
            let mission = self
                .template
                .missions
                .as_ref()
                .ok_or("missions are disabled")?
                .entries
                .iter()
                .find(|mission| mission.id == mission_id && mission.person == actor)
                .ok_or("unknown personal expedition")?;
            if mission.phase != game_parallel::MissionPhase::Ready || mission.party.is_some() {
                return Err("invitation requires an unformed ready expedition".into());
            }
            let (origin, destination, visit_ticks) =
                (mission.origin, mission.destination, mission.visit_ticks);
            game_parallel_fused_conflict::check(&self.template, &[actor, target])?;
            let locate = |person| -> Result<_, String> {
                let index = self
                    .positions
                    .binary_search_by_key(&person, |row| row.0)
                    .map_err(|_| "unknown person")?;
                let (_, block, slot) = self.positions[index];
                let block = &self.blocks[block];
                if block.health[slot] == 0
                    || block.journey.is_some()
                    || self.template.regions[block.region].id != origin
                    || block.work[slot] != game_parallel_fused_data::NONE
                {
                    return Err("participants must be living, local and available".into());
                }
                Ok((block.health[slot], block.hunger[slot]))
            };
            locate(actor)?;
            let (health, hunger) = locate(target)?;
            let social = self
                .template
                .social
                .as_ref()
                .ok_or("social decisions are disabled")?;
            let willing = social
                .participants
                .iter()
                .any(|row| row.person == target && row.willing)
                && health >= social.rules.min_health
                && hunger <= social.rules.max_hunger
                && self
                    .template
                    .personality
                    .as_ref()
                    .is_none_or(|personality| {
                        personality.index().social(
                            target,
                            0,
                            self.template.seed,
                            self.template.tick,
                        )
                    });
            let pressured = self
                .template
                .missions
                .as_ref()
                .and_then(|missions| missions.pressure_threshold)
                .zip(self.template.opinions.as_ref())
                .is_some_and(|(threshold, opinions)| {
                    let index = opinions.index();
                    u128::from(index.fear(target, actor))
                        + u128::from(index.obligation(target, actor))
                        >= u128::from(threshold)
                });
            if !willing && !pressured {
                return Ok(None);
            }
            let target_id = self.start_mission(game_parallel::MissionStart {
                person: target,
                destination,
                visit_ticks,
            })?;
            let party = mission_id.min(target_id);
            for mission in &mut self
                .template
                .missions
                .as_mut()
                .expect("enabled missions")
                .entries
            {
                if mission.id == mission_id || mission.id == target_id {
                    mission.party_opt_in = mission.id == mission_id || willing;
                    mission.party_compliant = mission.id == target_id && !willing;
                    mission.party = Some(party);
                }
            }
            Ok(Some(target_id))
        }
        /// Link independently accepted ready expeditions into a traveling party.
        pub fn form_mission_party(&mut self, ids: &[u64]) -> Result<u64, String> {
            use rayon::prelude::*;
            if ids.len() < 2 {
                return Err("a party requires at least two expeditions".into());
            }
            let mut ids = ids.to_vec();
            ids.par_sort_unstable();
            if ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
                return Err("duplicate expedition".into());
            }
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            let suspended = self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick));
            let missions = self
                .template
                .missions
                .as_mut()
                .ok_or("missions are disabled")?;
            let indices: Vec<_> = missions
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, mission)| ids.binary_search(&mission.id).is_ok().then_some(i))
                .collect();
            if indices.len() != ids.len() {
                return Err("unknown expedition".into());
            }
            let first = &missions.entries[indices[0]];
            let route = (first.origin, first.destination, first.visit_ticks);
            if indices.iter().any(|&i| {
                let mission = &missions.entries[i];
                mission.phase != game_parallel::MissionPhase::Ready
                    || !mission.party_opt_in
                    || mission.party.is_some()
                    || (mission.origin, mission.destination, mission.visit_ticks) != route
                    || suspended == Some(mission.person)
            }) {
                return Err("party members must have accepted matching ready expeditions".into());
            }
            let party = ids[0];
            for i in indices {
                missions.entries[i].party = Some(party);
            }
            self.revision = revision;
            Ok(party)
        }
        pub fn cancel_mission(&mut self, person: u64, id: u64) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let missions = self
                .template
                .missions
                .as_mut()
                .ok_or("missions are disabled")?;
            let mission = missions
                .entries
                .iter_mut()
                .find(|mission| mission.id == id && mission.person == person)
                .ok_or("unknown personal expedition")?;
            if matches!(
                mission.phase,
                game_parallel::MissionPhase::Completed | game_parallel::MissionPhase::Failed
            ) {
                return Err("expedition is already finished".into());
            }
            mission.phase = game_parallel::MissionPhase::Failed;
            mission.remaining = 0;
            missions.events.push(game_parallel::MissionEvent {
                tick: self.template.tick,
                mission: id,
                person,
                kind: game_parallel::MissionEventKind::Cancelled,
            });
            self.revision = revision;
            Ok(())
        }
    };
}

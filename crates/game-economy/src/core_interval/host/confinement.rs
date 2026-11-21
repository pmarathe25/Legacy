//! Individual choices about physical barriers and people held behind them.
use super::*;
use game_economy_confinement::{Capture, Confinement, Event, EventKind, GuardContext};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BarrierAction {
    Capture { target: u64, site: u64 },
    Guard { site: u64 },
    Release { target: u64, site: u64 },
    Escape { site: u64 },
}

impl BarrierAction {
    pub(super) fn choice_key(self) -> u64 {
        match self {
            Self::Capture { .. } => 1,
            Self::Guard { .. } => 2,
            Self::Release { .. } => 3,
            Self::Escape { .. } => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreConfinement {
    pub owner: Confinement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_action_millis: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_round_millis: Option<u128>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occupied: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<(u64, u128)>,
}

impl CoreConfinement {
    pub(super) fn new(mut owner: Confinement) -> Self {
        owner.normalize();
        Self {
            owner,
            last_action_millis: None,
            last_round_millis: None,
            occupied: Vec::new(),
            guards: Vec::new(),
        }
    }

    fn begin_action(&mut self, now: u128) {
        if self.last_action_millis != Some(now) {
            self.last_action_millis = Some(now);
            self.occupied.clear();
            self.guards.clear();
            self.owner.events.clear();
        }
    }

    fn reserve(&mut self, people: &[u64]) -> Result<(), String> {
        if people
            .iter()
            .any(|person| self.occupied.binary_search(person).is_ok())
        {
            return Err("a participant has already acted at this instant".into());
        }
        self.occupied.extend_from_slice(people);
        self.occupied.par_sort_unstable();
        self.occupied.dedup();
        Ok(())
    }

    fn spend_guard(&mut self, site: u64) -> u128 {
        self.guards
            .binary_search_by_key(&site, |row| row.0)
            .ok()
            .map_or(0, |index| self.guards.remove(index).1)
    }

    pub(super) fn validate(
        &self,
        core: &CoreInterval,
        sites: &crate::WorkSites,
        walking: Option<&game_economy_local_transport::walking::Walking>,
    ) -> Result<(), String> {
        self.owner
            .validate(&core.people, sites, core.physical.tick())?;
        if self
            .last_action_millis
            .is_some_and(|at| at > core.simulated_millis())
            || self
                .last_round_millis
                .is_some_and(|at| self.last_action_millis.is_none_or(|action| at > action))
            || (self.last_action_millis.is_none()
                && (!self.occupied.is_empty() || !self.guards.is_empty()))
            || self.occupied.par_windows(2).any(|pair| pair[0] >= pair[1])
            || self
                .guards
                .par_windows(2)
                .any(|pair| pair[0].0 >= pair[1].0)
            || self
                .occupied
                .par_iter()
                .any(|id| core.people.binary_search_by_key(id, |row| row.id).is_err())
            || self.guards.par_iter().any(|&(site, effort)| {
                effort == 0
                    || self
                        .owner
                        .cells
                        .binary_search_by_key(&site, |row| row.site)
                        .is_err()
            })
            || self.owner.holds.par_iter().any(|hold| {
                let Ok(index) = core.people.binary_search_by_key(&hold.person, |row| row.id) else {
                    return true;
                };
                let person = &core.people[index];
                person.region.is_none_or(|region| {
                    walking.is_some_and(|walking| !walking.stock_reachable(person.id, region))
                }) || (person.health == 0 && person.immortal.is_none())
            })
            || core.physical.work().par_iter().any(|row| {
                row.phase == game_lot_runtime::Phase::Working && self.owner.held(row.job.worker)
            })
        {
            return Err("invalid elapsed confinement state".into());
        }
        Ok(())
    }
}

impl CoreHost {
    pub(super) fn barrier_choices(&self, busy: &[u64]) -> Vec<(u64, BarrierAction)> {
        let (Some(owner), Some(sites)) = (self.confinement(), self.sites.as_ref()) else {
            return Vec::new();
        };
        let find = |id| {
            self.core
                .people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|index| &self.core.people[index])
        };
        let available = |id| {
            find(id).is_some_and(|actor| {
                !actor.controlled && actor.health > 0 && busy.binary_search(&id).is_err()
            })
        };
        let at_barrier = |person, site| {
            self.free_at_stock(person)
                && sites
                    .facilities
                    .sites
                    .binary_search_by_key(&site, |row| row.id)
                    .ok()
                    .is_some_and(|index| {
                        find(person).and_then(|p| p.region)
                            == Some(sites.facilities.sites[index].region)
                    })
                && owner
                    .cells
                    .binary_search_by_key(&site, |row| row.site)
                    .ok()
                    .is_some_and(|i| owner.cells[i].resistance > 0)
        };
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let motivated = |actor, willing, pressured_by: Option<u64>, threshold: Option<u64>| {
            willing
                || pressured_by
                    .zip(threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(actor, other);
                            fear.saturating_add(obligation) >= threshold
                        })
                    })
        };
        let captures = owner.capture_policies.par_iter().filter_map(|row| {
            (available(row.person)
                && at_barrier(row.person, row.site)
                && at_barrier(row.target, row.site)
                && busy.binary_search(&row.target).is_err()
                && motivated(
                    row.person,
                    row.willing,
                    row.pressured_by,
                    row.pressure_threshold,
                ))
            .then_some((
                row.person,
                BarrierAction::Capture {
                    target: row.target,
                    site: row.site,
                },
            ))
        });
        let guards = owner.guard_policies.par_iter().filter_map(|row| {
            (available(row.person)
                && at_barrier(row.person, row.site)
                && motivated(
                    row.person,
                    row.willing,
                    row.pressured_by,
                    row.pressure_threshold,
                ))
            .then_some((row.person, BarrierAction::Guard { site: row.site }))
        });
        let releases = owner.release_policies.par_iter().filter_map(|row| {
            let site = owner.site_of(row.target)?;
            (available(row.person)
                && at_barrier(row.person, site)
                && motivated(
                    row.person,
                    row.willing,
                    row.pressured_by,
                    row.pressure_threshold,
                ))
            .then_some((
                row.person,
                BarrierAction::Release {
                    target: row.target,
                    site,
                },
            ))
        });
        let escapes = owner.escape.par_iter().filter_map(|row| {
            let site = owner.site_of(row.person)?;
            (row.willing && available(row.person))
                .then_some((row.person, BarrierAction::Escape { site }))
        });
        captures
            .chain(guards)
            .chain(releases)
            .chain(escapes)
            .collect()
    }

    pub(super) fn act_on_barriers(
        &mut self,
        blocked: &[u64],
        selected: &[(u64, BarrierAction)],
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        let Some(opening) = self.confinement() else {
            return Ok((Vec::new(), Vec::new()));
        };
        if opening.capture_policies.is_empty()
            && opening.guard_policies.is_empty()
            && opening.release_policies.is_empty()
            && opening.holds.is_empty()
        {
            return Ok((Vec::new(), Vec::new()));
        }
        let (sites, mut staged, tick) = self.confinement_opening()?;
        let now = self.core.simulated_millis();
        if staged.last_round_millis == Some(now) {
            return Ok((Vec::new(), Vec::new()));
        }
        staged.last_round_millis = Some(now);
        let mut previous = std::mem::take(&mut staged.owner.events);
        let mut busy = self.local_walk_exclusions(blocked);
        extend_busy(&mut busy, staged.occupied.iter().copied());
        let profiles = self
            .schedule
            .as_ref()
            .and_then(|schedule| schedule.personal_choices.as_deref());
        let chooses_through_profile = |id| {
            profiles
                .and_then(|profiles| {
                    profiles
                        .binary_search_by_key(&id, |profile| profile.id)
                        .ok()
                        .map(|index| &profiles[index])
                })
                .is_some_and(|profile| {
                    profile
                        .preferences
                        .binary_search_by_key(&personal_choice::CONFINEMENT, |row| row.domain)
                        .is_ok()
                })
        };
        let choices: Vec<_> = self
            .barrier_choices(&busy)
            .into_par_iter()
            .filter(|&(id, action)| {
                if chooses_through_profile(id) {
                    selected
                        .binary_search_by_key(&id, |choice| choice.0)
                        .ok()
                        .is_some_and(|index| selected[index].1 == action)
                } else {
                    true
                }
            })
            .collect();
        let [captures, guards, openings, escapes]: [Vec<u64>; 4] = std::array::from_fn(|i| {
            let mut actors: Vec<_> = choices
                .par_iter()
                .filter_map(|&(actor, action)| {
                    (action.choice_key() == i as u64 + 1).then_some(actor)
                })
                .collect();
            actors.par_sort_unstable();
            actors
        });
        let outside = self
            .block_away_from_stock(&busy)
            .unwrap_or_else(|| busy.clone());
        let capture = game_economy_confinement::AutonomousContext {
            owner: &mut staged.owner,
            people: &self.core.people,
            sites,
            busy: &outside,
            controlled_unavailable: &outside,
            opinions: self.opinions.as_ref(),
            seed: self.core.seed,
            tick,
        }
        .prepare_selected(Some(&captures));
        extend_busy(&mut busy, capture.participants.iter().copied());
        let mut outside = self
            .block_away_from_stock(&busy)
            .unwrap_or_else(|| busy.clone());
        outside.extend_from_slice(&capture.captured);
        outside.par_sort_unstable();
        outside.dedup();
        let guards = GuardContext {
            owner: &staged.owner,
            people: &self.core.people,
            sites,
            busy: &outside,
            opinions: self.opinions.as_ref(),
            manual: None,
            tick,
        }
        .prepare_selected(Some(&guards))?;
        extend_busy(&mut busy, guards.actors.iter().copied());
        extend_busy(&mut outside, guards.actors.iter().copied());
        let mut current = std::mem::take(&mut staged.owner.events);
        current.extend(guards.events);
        let mut opening_strengths = staged.guards.clone();
        add_guard_effort(&mut opening_strengths, &guards.opening_strengths);
        let releases = game_economy_confinement::ReleaseContext {
            owner: &mut staged.owner,
            people: &self.core.people,
            sites,
            busy: &outside,
            reserved_targets: &[],
            opening_guards: &opening_strengths,
            opinions: self.opinions.as_ref(),
            seed: self.core.seed,
            tick,
        }
        .prepare_selected(Some(&openings));
        extend_busy(&mut busy, releases.iter().copied());
        let mut contests: Vec<_> = staged
            .owner
            .events
            .par_iter()
            .filter_map(|event| {
                Some((
                    event.site,
                    u128::from(staged.owner.person_effort(event.actor?)),
                ))
            })
            .collect();
        contests.par_sort_unstable();
        let spent: Vec<_> = contests
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| (group[0].0, group.iter().map(|row| row.1).sum::<u128>()))
            .collect();
        for (site, effort) in &mut opening_strengths {
            if let Ok(index) = spent.binary_search_by_key(site, |row| row.0) {
                *effort = effort.saturating_sub(spent[index].1);
            }
        }
        let mut escape_strengths = opening_strengths;
        add_guard_effort(&mut escape_strengths, &guards.escape_only_strengths);
        let denied_escapes: Vec<_> = staged
            .owner
            .holds
            .par_iter()
            .filter_map(|hold| {
                escapes
                    .binary_search(&hold.person)
                    .is_err()
                    .then_some(hold.person)
            })
            .collect();
        extend_busy(&mut busy, denied_escapes);
        let (mut next, attempts, _) = staged.owner.escape_attempts(
            &self.core.people,
            &busy,
            tick,
            None,
            &escape_strengths,
        )?;
        let freed: Vec<_> = staged
            .owner
            .holds
            .par_iter()
            .filter(|hold| !next.held(hold.person))
            .map(|hold| Event {
                tick,
                person: hold.person,
                actor: None,
                site: hold.site,
                kind: EventKind::Released,
            })
            .collect();
        next.events.extend(freed);
        current.append(&mut staged.owner.events);
        current.append(&mut next.events);
        let mut actors = capture.participants;
        extend_busy(
            &mut actors,
            guards.actors.into_iter().chain(releases).chain(attempts),
        );
        staged.reserve(&actors)?;
        staged.guards.clear();
        previous.extend_from_slice(&current);
        next.events = previous;
        staged.owner = next;
        self.publish_confinement(staged, &current, tick);
        Ok((actors, current))
    }
    pub(super) fn confinement(&self) -> Option<&Confinement> {
        Some(&self.personal_conflict.as_ref()?.confinement.as_ref()?.owner)
    }

    pub(super) fn enroll_confined_births(&mut self, births: &[(u64, u64)]) {
        let Some(state) = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.confinement.as_mut())
        else {
            return;
        };
        let births: Vec<_> = births
            .par_iter()
            .map(|&(child, mother)| game_economy_confinement::Birth { child, mother })
            .collect();
        state.owner.enroll_births(&births);
    }

    pub(super) fn remove_deceased_holds(&mut self) {
        let Some(state) = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.confinement.as_mut())
        else {
            return;
        };
        state.owner.holds.retain(|hold| {
            self.core
                .people
                .binary_search_by_key(&hold.person, |row| row.id)
                .ok()
                .is_some_and(|index| {
                    let person = &self.core.people[index];
                    person.health > 0 || person.immortal.is_some()
                })
        });
    }

    pub fn held_at(&self, person: u64) -> Option<u64> {
        self.confinement()?.site_of(person)
    }

    pub(super) fn block_held(&self, busy: &[u64]) -> Option<Vec<u64>> {
        let held = &self.confinement()?.holds;
        if held.is_empty() {
            return None;
        }
        let mut excluded = busy.to_vec();
        excluded.par_extend(held.par_iter().map(|hold| hold.person));
        excluded.par_sort_unstable();
        excluded.dedup();
        Some(excluded)
    }

    pub(super) fn enroll_built_barriers(&mut self, built: &[game_economy_building::Event]) {
        let Some(state) = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.confinement.as_mut())
        else {
            return;
        };
        let Some(sites) = &self.sites else {
            return;
        };
        let built: Vec<_> = built.par_iter().filter_map(|event| event.site).collect();
        state.owner.enroll_sites(&built, &sites.facilities);
    }

    pub(super) fn release_destroyed_barriers(&mut self, destroyed: &HashSet<u64>) -> Vec<Event> {
        let Some(sites) = &self.sites else {
            return Vec::new();
        };
        let Some(state) = self
            .personal_conflict
            .as_mut()
            .and_then(|owner| owner.confinement.as_mut())
        else {
            return Vec::new();
        };
        if !state
            .owner
            .cells
            .par_iter()
            .any(|cell| destroyed.contains(&cell.site))
        {
            return Vec::new();
        }
        let tick = self.core.physical.tick();
        let released: Vec<_> = state
            .owner
            .holds
            .par_iter()
            .filter_map(|hold| {
                destroyed.contains(&hold.site).then_some(Event {
                    tick,
                    person: hold.person,
                    actor: None,
                    site: hold.site,
                    kind: EventKind::Released,
                })
            })
            .collect();
        state.begin_action(self.core.simulated_millis());
        state.owner.retain_sites(&sites.facilities);
        state.guards.retain(|row| !destroyed.contains(&row.0));
        state.owner.events.extend(released.iter().cloned());
        released
    }

    fn confinement_opening(&self) -> Result<(&crate::WorkSites, CoreConfinement, u64), String> {
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before approaching a barrier".into());
        }
        let sites = &self.sites.as_ref().ok_or("no physical sites")?.facilities;
        let mut state = self
            .personal_conflict
            .as_ref()
            .and_then(|owner| owner.confinement.as_ref())
            .ok_or("confinement is disabled")?
            .clone();
        state.begin_action(self.core.simulated_millis());
        Ok((sites, state, self.core.physical.tick()))
    }

    fn free_at_stock(&self, person: u64) -> bool {
        self.core
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .and_then(|index| self.core.people[index].region)
            .is_some_and(|region| self.regional_stock_reachable(person, region))
    }

    fn publish_confinement(&mut self, state: CoreConfinement, events: &[Event], tick: u64) {
        if let Some(opinions) = &mut self.opinions {
            use game_opinions::{ConfinementExperience, ConfinementExperienceKind as Kind};
            let experiences: Vec<_> = events
                .par_iter()
                .filter_map(|event| {
                    let actor = event.actor?;
                    let kind = match event.kind {
                        EventKind::Confined => Kind::Captured,
                        EventKind::CaptureResisted => Kind::Resisted,
                        EventKind::Released => Kind::Released,
                        _ => return None,
                    };
                    Some(ConfinementExperience {
                        observer: event.person,
                        actor,
                        kind,
                    })
                })
                .collect();
            opinions.learn_confinement(&experiences, tick);
        }
        self.personal_conflict
            .as_mut()
            .expect("physical confinement owner")
            .confinement = Some(state);
        self.work_checked_revision = None;
    }

    pub fn confine_controlled_target(
        &mut self,
        actor: u64,
        target: u64,
        site: u64,
    ) -> Result<bool, String> {
        self.controlled_person(actor)?;
        let (sites, mut state, tick) = self.confinement_opening()?;
        let occupied = self.local_walk_exclusions(&[]);
        if actor == target
            || !self.free_at_stock(actor)
            || !self.free_at_stock(target)
            || occupied.binary_search(&actor).is_ok()
            || occupied.binary_search(&target).is_ok()
        {
            return Err("capture requires reachable available people".into());
        }
        state.reserve(&[actor, target])?;
        let pressured = self.opinions.as_ref().is_some_and(|opinions| {
            state
                .owner
                .capture_responses
                .binary_search_by_key(&target, |row| row.person)
                .ok()
                .and_then(|index| state.owner.capture_responses[index].pressure_threshold)
                .is_some_and(|threshold| {
                    let (fear, obligation) = opinions.index().feelings(target, actor);
                    fear.saturating_add(obligation) >= threshold
                })
        });
        let captured = state.owner.capture(
            Capture {
                actor,
                target,
                site,
            },
            &self.core.people,
            sites,
            &occupied,
            pressured,
            tick,
        )?;
        let events = [state.owner.events.last().expect("capture event").clone()];
        self.publish_confinement(state, &events, tick);
        Ok(captured)
    }

    pub fn guard_controlled_cell(&mut self, actor: u64, site: u64) -> Result<(), String> {
        self.controlled_person(actor)?;
        let (sites, mut state, tick) = self.confinement_opening()?;
        let busy = self.local_walk_exclusions(&[]);
        if !self.free_at_stock(actor) || busy.binary_search(&actor).is_ok() {
            return Err("cell is outside physical reach or person is occupied".into());
        }
        state.reserve(&[actor])?;
        let guards = GuardContext {
            owner: &state.owner,
            people: &self.core.people,
            sites,
            busy: &busy,
            opinions: self.opinions.as_ref(),
            manual: Some((actor, site)),
            tick,
        }
        .prepare_selected(Some(&[]))?;
        let strength = guards.strengths[0].1;
        match state.guards.binary_search_by_key(&site, |row| row.0) {
            Ok(index) => state.guards[index].1 = state.guards[index].1.saturating_add(strength),
            Err(index) => state.guards.insert(index, (site, strength)),
        }
        state.owner.events.extend(guards.events.iter().cloned());
        self.publish_confinement(state, &guards.events, tick);
        Ok(())
    }

    pub fn release_controlled_target(&mut self, actor: u64, target: u64) -> Result<bool, String> {
        self.controlled_person(actor)?;
        let (sites, mut state, tick) = self.confinement_opening()?;
        if !self.free_at_stock(actor)
            || self
                .local_walk_exclusions(&[])
                .binary_search(&actor)
                .is_ok()
        {
            return Err("cell is outside physical reach or person is occupied".into());
        }
        let site = state.owner.site_of(target).ok_or("person is not held")?;
        state.reserve(&[actor])?;
        let opposition = state.spend_guard(site);
        let opened =
            state
                .owner
                .release(actor, target, &self.core.people, sites, tick, opposition)?;
        let events = [state.owner.events.last().expect("release event").clone()];
        self.publish_confinement(state, &events, tick);
        Ok(opened)
    }

    pub fn attempt_controlled_escape(&mut self, actor: u64) -> Result<bool, String> {
        self.controlled_person(actor)?;
        let (_, mut state, tick) = self.confinement_opening()?;
        if self
            .local_walk_exclusions(&[])
            .binary_search(&actor)
            .is_ok()
        {
            return Err("person is occupied".into());
        }
        let site = state.owner.site_of(actor).ok_or("person is not held")?;
        state.reserve(&[actor])?;
        let opposing = [(site, state.spend_guard(site))];
        let busy: Vec<_> = state
            .owner
            .holds
            .par_iter()
            .map(|hold| hold.person)
            .collect();
        let (mut next, _, _) =
            state
                .owner
                .escape_attempts(&self.core.people, &busy, tick, Some(actor), &opposing)?;
        let escaped = !next.held(actor);
        let mut events = std::mem::take(&mut next.events);
        if escaped {
            events.par_extend(
                state
                    .owner
                    .holds
                    .par_iter()
                    .filter(|hold| hold.site == site)
                    .map(|hold| Event {
                        tick,
                        person: hold.person,
                        actor: Some(actor),
                        site,
                        kind: EventKind::Released,
                    }),
            );
        }
        next.events = state.owner.events;
        next.events.extend(events.iter().cloned());
        state.owner = next;
        self.publish_confinement(state, &events, tick);
        Ok(escaped)
    }
}

fn extend_busy(busy: &mut Vec<u64>, additional: impl IntoIterator<Item = u64>) {
    busy.extend(additional);
    busy.par_sort_unstable();
    busy.dedup();
}

fn add_guard_effort(strengths: &mut Vec<(u64, u128)>, added: &[(u64, u128)]) {
    strengths.extend_from_slice(added);
    strengths.par_sort_unstable_by_key(|row| row.0);
    *strengths = strengths
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            (
                group[0].0,
                group
                    .iter()
                    .fold(0u128, |sum, row| sum.saturating_add(row.1)),
            )
        })
        .collect();
}

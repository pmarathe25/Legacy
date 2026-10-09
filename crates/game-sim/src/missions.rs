//! Expedition commitments use real people, cargo, personal maps, and tactical bodies.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    pub person: PersonId,
    pub recruitment: RecruitmentId,
    pub status: CompanionStatus,
    pub directive: MissionObjective,
    pub received_at: SimTime,
    pub visited: BTreeSet<AreaId>,
    pub delivered: u64,
    pub cargo: BTreeMap<InventoryId, u64>,
    pub contacts: BTreeMap<PersonId, AreaId>,
    pub last_leg_from: Option<AreaId>,
    pub explanation: String,
    pub declared_explanation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub sponsor: PersonId,
    pub proposal: MissionProposal,
    pub status: MissionStatus,
    pub members: BTreeMap<PersonId, Companion>,
    pub created_at: SimTime,
    pub departed_at: Option<SimTime>,
    pub explanation: String,
    pub succeeded: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruitment {
    pub id: RecruitmentId,
    pub mission: MissionId,
    pub person: PersonId,
    pub compensation: Vec<AgreementTerm>,
    pub agreement: Option<AgreementId>,
    pub countered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conversation {
    Objective {
        mission: MissionId,
        leader: PersonId,
        objective: MissionObjective,
    },
    Report {
        recipient: PersonId,
        snapshot: Box<MissionView>,
    },
    Retreat {
        mission: MissionId,
        leader: PersonId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTalk {
    pub speaker: PersonId,
    pub listener: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub conversation: Conversation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub missions: BTreeMap<MissionId, Mission>,
    pub recruitments: BTreeMap<RecruitmentId, Recruitment>,
    pub known: BTreeMap<PersonId, BTreeMap<MissionId, MissionView>>,
    pub talks: BTreeMap<PersonId, PendingTalk>,
    pub last_event: u64,
    pub last_review: SimTime,
}

impl Simulation {
    pub fn mission_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "mission-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        let custodians = sim
            .state
            .inventories
            .values()
            .map(|stock| {
                (
                    stock.id,
                    match stock.location {
                        InventoryLocation::Carried(person) => person,
                        InventoryLocation::Area(_) => sim.state.ruler,
                    },
                )
            })
            .collect();
        sim.state.agreements = Some(agreements::World {
            knowledge: BTreeMap::new(),
            last_observation_event: sim.state.next_event - 1,
            agreements: BTreeMap::new(),
            obligations: BTreeMap::new(),
            amendments: BTreeMap::new(),
            custodians,
            last_event: sim.state.next_event - 1,
            next_search: BTreeMap::new(),
            pending_work: BTreeMap::new(),
        });
        sim.state.missions = Some(World {
            missions: BTreeMap::new(),
            recruitments: BTreeMap::new(),
            known: BTreeMap::new(),
            talks: BTreeMap::new(),
            last_event: sim.state.next_event - 1,
            last_review: sim.state.time,
        });
        Ok(sim)
    }
    fn mission_area(&self, actor: PersonId) -> Result<AreaId, Error> {
        let index = self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        let body = &self
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?
            .actors[&actor];
        if !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.state.combat.as_ref().is_some_and(|w| {
                w.captivity.contains_key(&actor) || body.vitals.health <= w.rules.incapacitation
            })
        {
            return Err(Error::Unavailable);
        }
        match self.state.people[*index].location {
            Location::AtArea(area) => Ok(area),
            _ => Err(Error::Unavailable),
        }
    }
    fn mission_terms(&self, sponsor: PersonId, recruit: PersonId, terms: &[AgreementTerm]) -> bool {
        terms.len() <= 8
            && terms.iter().all(|term| {
                let parties = term.parties();
                parties[0] != parties[1]
                    && parties
                        .iter()
                        .all(|person| *person == sponsor || *person == recruit)
                    && term.amount() > 0
                    && term.deadline().is_none_or(|d| d > self.state.time)
                    && match term {
                        AgreementTerm::Delivery {
                            source,
                            destination,
                            quantity,
                            ..
                        } => self
                            .state
                            .inventories
                            .get(source)
                            .zip(self.state.inventories.get(destination))
                            .is_some_and(|(a, b)| {
                                a.good == b.good && a.id != b.id && quantity.0 <= b.capacity.0
                            }),
                        AgreementTerm::Access {
                            grantor, inventory, ..
                        } => {
                            self.state
                                .agreements
                                .as_ref()
                                .is_some_and(|w| w.custodians.get(inventory) == Some(grantor))
                                || self.state.inventories.get(inventory).is_some_and(|i| {
                                    i.location == InventoryLocation::Carried(*grantor)
                                })
                        }
                        _ => false,
                    }
            })
    }
    fn mission_objective_valid(&self, objective: &MissionObjective) -> bool {
        let area = |id: &AreaId| self.state.areas.iter().any(|a| a.id == *id);
        match objective {
            MissionObjective::Visit { destination } => area(destination),
            MissionObjective::Deliver {
                source,
                destination,
                quantity,
            } => {
                quantity.0 > 0
                    && self
                        .state
                        .inventories
                        .get(source)
                        .zip(self.state.inventories.get(destination))
                        .is_some_and(|(a, b)| {
                            a.good == b.good && a.id != b.id && quantity.0 <= b.capacity.0
                        })
            }
            MissionObjective::Rescue { person, return_to } => {
                self.person_index.contains_key(person) && area(return_to)
            }
            MissionObjective::Defeat { person } => self.person_index.contains_key(person),
            MissionObjective::Patrol { areas, until } => {
                !areas.is_empty()
                    && areas.len() <= 16
                    && areas.iter().all(area)
                    && *until > self.state.time
            }
        }
    }
    pub(crate) fn mission_command(
        &mut self,
        actor: PersonId,
        action: &MissionAction,
    ) -> Result<(), Error> {
        let area = self.mission_area(actor)?;
        if self.in_combat(actor) {
            return Err(Error::Unavailable);
        }
        let world = self.state.missions.as_ref().ok_or(Error::Unavailable)?;
        self.ensure_event_capacity(64)?;
        match action {
            MissionAction::Propose { proposal } => {
                let people: BTreeSet<_> = proposal
                    .recruits
                    .iter()
                    .copied()
                    .chain([proposal.leader])
                    .collect();
                if !(1..=32).contains(&people.len())
                    || proposal.deadline <= self.state.time
                    || proposal.home != area
                    || people.iter().any(|id| {
                        self.mission_area(*id).ok() != Some(area)
                            || self.in_combat(*id)
                            || self.mission_membership(*id).is_some()
                    })
                    || !self.mission_objective_valid(&proposal.objective)
                    || matches!(&proposal.objective,MissionObjective::Defeat {person} if people.contains(person)||*person==actor)
                    || proposal.supplies.len() > 32
                    || proposal.compensation.iter().any(|(id, terms)| {
                        !people.contains(id) || !self.mission_terms(actor, *id, terms)
                    })
                    || proposal.supplies.iter().any(|s| {
                        !people.contains(&s.carrier)
                            || s.quantity.0 == 0
                            || s.quantity.0
                                > self
                                    .state
                                    .physical
                                    .as_ref()
                                    .unwrap()
                                    .rules
                                    .carrying_capacity
                                    .0
                            || !self.state.inventories.contains_key(&s.source)
                    })
                {
                    return Err(Error::Unavailable);
                }
                self.state
                    .next_id
                    .checked_add(1 + people.len() as u64)
                    .ok_or(Error::Overflow)?;
                let id = MissionId(allocate(&mut self.state)?);
                let mut members = BTreeMap::new();
                let contacts: BTreeMap<_, _> = self
                    .state
                    .people
                    .iter()
                    .filter(|p| p.location == Location::AtArea(area))
                    .map(|p| (p.id, area))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .take(64)
                    .collect();
                for person in people {
                    let recruitment = RecruitmentId(allocate(&mut self.state)?);
                    let compensation = proposal
                        .compensation
                        .get(&person)
                        .cloned()
                        .unwrap_or_default();
                    self.state.missions.as_mut().unwrap().recruitments.insert(
                        recruitment,
                        Recruitment {
                            id: recruitment,
                            mission: id,
                            person,
                            compensation,
                            agreement: None,
                            countered: false,
                        },
                    );
                    members.insert(
                        person,
                        Companion {
                            person,
                            recruitment,
                            status: if person == actor {
                                CompanionStatus::Accepted
                            } else {
                                CompanionStatus::Offered
                            },
                            directive: proposal.objective.clone(),
                            received_at: self.state.time,
                            visited: BTreeSet::from([area]),
                            delivered: 0,
                            cargo: BTreeMap::new(),
                            contacts: contacts.clone(),
                            last_leg_from: None,
                            declared_explanation: "Received an expedition proposal".into(),
                            explanation: "Considering the proposed expedition and compensation"
                                .into(),
                        },
                    );
                }
                self.state.missions.as_mut().unwrap().missions.insert(
                    id,
                    Mission {
                        id,
                        sponsor: actor,
                        proposal: proposal.clone(),
                        status: MissionStatus::Preparing,
                        members,
                        created_at: self.state.time,
                        departed_at: None,
                        explanation: "Preparing volunteers and real provisions".into(),
                        succeeded: false,
                    },
                );
                self.emit(EventKind::MissionProposed {
                    mission: id,
                    sponsor: actor,
                });
                self.publish_mission(id);
            }
            MissionAction::Accept { recruitment } => {
                let offer = world
                    .recruitments
                    .get(recruitment)
                    .ok_or(Error::Unavailable)?
                    .clone();
                let mission = &world.missions[&offer.mission];
                if offer.person != actor
                    || self.guardian_mission_duty(actor, offer.mission)
                    || self.state.decisions.as_ref().unwrap().minds[&actor]
                        .child_signals
                        .values()
                        .any(|signal| signal.urgency >= 20_000)
                    || self
                        .mission_membership(actor)
                        .is_some_and(|id| id != offer.mission)
                    || offer.countered
                    || mission.status != MissionStatus::Preparing
                    || mission.members[&actor].status != CompanionStatus::Offered
                    || self.mission_area(mission.sponsor).ok() != Some(area)
                    || !self.mission_terms(mission.sponsor, actor, &offer.compensation)
                {
                    return Err(Error::Unavailable);
                }
                let sponsor = mission.sponsor;
                let agreement = if offer.compensation.is_empty() {
                    None
                } else {
                    self.state
                        .next_id
                        .checked_add(1 + offer.compensation.len() as u64)
                        .ok_or(Error::Overflow)?;
                    let before = self.state.clone();
                    let adopted = (|| {
                        self.agreement_command(
                            sponsor,
                            &AgreementAction::Propose {
                                parties: BTreeSet::from([sponsor, actor]),
                                terms: offer.compensation.clone(),
                                procedure: None,
                            },
                        )?;
                        let id = *self
                            .state
                            .agreements
                            .as_ref()
                            .unwrap()
                            .agreements
                            .keys()
                            .next_back()
                            .unwrap();
                        self.agreement_command(actor, &AgreementAction::Accept { agreement: id })?;
                        Ok::<_, Error>(id)
                    })();
                    match adopted {
                        Ok(id) => Some(id),
                        Err(error) => {
                            self.state = before;
                            return Err(error);
                        }
                    }
                };
                let world = self.state.missions.as_mut().unwrap();
                world.recruitments.get_mut(recruitment).unwrap().agreement = agreement;
                let member = world
                    .missions
                    .get_mut(&offer.mission)
                    .unwrap()
                    .members
                    .get_mut(&actor)
                    .unwrap();
                member.status = CompanionStatus::Accepted;
                member.declared_explanation =
                    "Accepted the expedition objective and compensation".into();
                member.explanation = "Accepted the objective and compensation".into();
                self.emit(EventKind::MissionRecruitmentChanged {
                    mission: offer.mission,
                    person: actor,
                    status: CompanionStatus::Accepted,
                });
                self.publish_mission(offer.mission);
            }
            MissionAction::Refuse { recruitment } => {
                let offer = world
                    .recruitments
                    .get(recruitment)
                    .ok_or(Error::Unavailable)?;
                let mission = &world.missions[&offer.mission];
                if offer.person != actor
                    || mission.members[&actor].status != CompanionStatus::Offered
                    || self.mission_area(mission.sponsor).ok() != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                let id = offer.mission;
                let member = self
                    .state
                    .missions
                    .as_mut()
                    .unwrap()
                    .missions
                    .get_mut(&id)
                    .unwrap()
                    .members
                    .get_mut(&actor)
                    .unwrap();
                member.status = CompanionStatus::Refused;
                member.declared_explanation = "Declined the proposed service".into();
                member.explanation = "Declined the proposed service".into();
                self.emit(EventKind::MissionRecruitmentChanged {
                    mission: id,
                    person: actor,
                    status: CompanionStatus::Refused,
                });
                self.publish_mission(id);
            }
            MissionAction::AcceptCounteroffer { recruitment } => {
                let offer = world
                    .recruitments
                    .get(recruitment)
                    .ok_or(Error::Unavailable)?;
                let mission = &world.missions[&offer.mission];
                if mission.sponsor != actor
                    || !offer.countered
                    || self.mission_area(offer.person).ok() != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                let id = offer.mission;
                self.state
                    .missions
                    .as_mut()
                    .unwrap()
                    .recruitments
                    .get_mut(recruitment)
                    .unwrap()
                    .countered = false;
                self.publish_mission(id);
            }
            MissionAction::Counteroffer {
                recruitment,
                compensation,
            } => {
                let offer = world
                    .recruitments
                    .get(recruitment)
                    .ok_or(Error::Unavailable)?;
                let mission = &world.missions[&offer.mission];
                if offer.person != actor
                    || mission.members[&actor].status != CompanionStatus::Offered
                    || self.mission_area(mission.sponsor).ok() != Some(area)
                    || !self.mission_terms(mission.sponsor, actor, compensation)
                    || *compensation == offer.compensation
                {
                    return Err(Error::Unavailable);
                }
                let id = offer.mission;
                let offer = self
                    .state
                    .missions
                    .as_mut()
                    .unwrap()
                    .recruitments
                    .get_mut(recruitment)
                    .unwrap();
                offer.compensation = compensation.clone();
                offer.countered = true;
                self.state
                    .missions
                    .as_mut()
                    .unwrap()
                    .missions
                    .get_mut(&id)
                    .unwrap()
                    .members
                    .get_mut(&actor)
                    .unwrap()
                    .explanation =
                    "Asked for different compensation; the sponsor must accept the counteroffer"
                        .into();
                self.publish_mission(id);
            }
            MissionAction::Depart { mission } => {
                let m = world.missions.get(mission).ok_or(Error::Unavailable)?;
                if actor != m.sponsor && actor != m.proposal.leader
                    || m.status != MissionStatus::Preparing
                    || m.members[&m.proposal.leader].status != CompanionStatus::Accepted
                    || self.mission_area(m.proposal.leader).ok() != Some(area)
                    || m.proposal.home != area
                {
                    return Err(Error::Unavailable);
                }
                let m = self
                    .state
                    .missions
                    .as_mut()
                    .unwrap()
                    .missions
                    .get_mut(mission)
                    .unwrap();
                m.status = MissionStatus::Traveling;
                m.departed_at = Some(self.state.time);
                m.explanation = "Gathering supplies and setting out".into();
                self.emit(EventKind::MissionDeparted { mission: *mission });
                self.publish_mission(*mission);
            }
            MissionAction::Objective { mission, objective } => {
                let m = world.missions.get(mission).ok_or(Error::Unavailable)?;
                if actor != m.sponsor && actor != m.proposal.leader
                    || !matches!(
                        m.status,
                        MissionStatus::Preparing
                            | MissionStatus::Traveling
                            | MissionStatus::Operating
                    )
                    || !self.mission_objective_valid(objective)
                    || self.mission_area(m.proposal.leader).ok() != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                self.start_mission_talk(
                    actor,
                    m.proposal.leader,
                    Conversation::Objective {
                        mission: *mission,
                        leader: m.proposal.leader,
                        objective: objective.clone(),
                    },
                )?;
            }
            MissionAction::Retreat { mission } => {
                let m = world.missions.get(mission).ok_or(Error::Unavailable)?;
                if actor != m.proposal.leader && actor != m.sponsor
                    || !matches!(
                        m.status,
                        MissionStatus::Traveling | MissionStatus::Operating
                    )
                    || self.mission_area(m.proposal.leader).ok() != Some(area)
                {
                    return Err(Error::Unavailable);
                }
                self.start_mission_talk(
                    actor,
                    m.proposal.leader,
                    Conversation::Retreat {
                        mission: *mission,
                        leader: m.proposal.leader,
                    },
                )?;
            }
            MissionAction::Leave { mission } => {
                let m = world.missions.get(mission).ok_or(Error::Unavailable)?;
                if !m
                    .members
                    .get(&actor)
                    .is_some_and(|member| member.status == CompanionStatus::Accepted)
                {
                    return Err(Error::Unavailable);
                }
                let member = self
                    .state
                    .missions
                    .as_mut()
                    .unwrap()
                    .missions
                    .get_mut(mission)
                    .unwrap()
                    .members
                    .get_mut(&actor)
                    .unwrap();
                member.status = CompanionStatus::Withdrawn;
                member.declared_explanation = "Withdrew service".into();
                member.explanation = "Withdrew service; existing payment obligations remain".into();
                self.emit(EventKind::MissionRecruitmentChanged {
                    mission: *mission,
                    person: actor,
                    status: CompanionStatus::Withdrawn,
                });
                self.publish_mission(*mission);
            }
            MissionAction::Report { mission, recipient } => {
                if *recipient == actor || self.mission_area(*recipient).ok() != Some(area) {
                    return Err(Error::Unavailable);
                }
                let snapshot = world
                    .known
                    .get(&actor)
                    .and_then(|known| known.get(mission))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                self.start_mission_talk(
                    actor,
                    *recipient,
                    Conversation::Report {
                        recipient: *recipient,
                        snapshot: Box::new(snapshot),
                    },
                )?;
            }
        }
        Ok(())
    }

    fn mission_membership(&self, actor: PersonId) -> Option<MissionId> {
        self.state
            .missions
            .as_ref()?
            .missions
            .values()
            .find(|m| {
                !self
                    .state
                    .missions
                    .as_ref()
                    .unwrap()
                    .known
                    .get(&actor)
                    .and_then(|known| known.get(&m.id))
                    .is_some_and(|view| {
                        matches!(
                            view.status,
                            MissionStatus::Completed
                                | MissionStatus::Failed
                                | MissionStatus::Cancelled
                        )
                    })
                    && m.members
                        .get(&actor)
                        .is_some_and(|member| member.status == CompanionStatus::Accepted)
            })
            .map(|m| m.id)
    }
    pub(crate) fn mission_actor_busy(&self, actor: PersonId) -> bool {
        let Some(id) = self.mission_membership(actor) else {
            return false;
        };
        if self.guardian_mission_duty(actor, id) {
            return false;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        body.vitals.alive
            && body.vitals.health > self.state.physical.as_ref().unwrap().rules.max_health / 2
            && body.vitals.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief
            && body.vitals.fatigue < self.state.physical.as_ref().unwrap().rules.fatigue_limit
            && !self.combat_restricts(actor)
            && !self.state.decisions.as_ref().unwrap().minds[&actor]
                .child_signals
                .values()
                .any(|signal| signal.urgency >= 20_000)
    }
    fn start_mission_talk(
        &mut self,
        speaker: PersonId,
        listener: PersonId,
        conversation: Conversation,
    ) -> Result<(), Error> {
        let area = self.mission_area(speaker)?;
        if self.in_combat(speaker)
            || self.in_combat(listener)
            || self.mission_area(listener).ok() != Some(area)
        {
            return Err(Error::Unavailable);
        }
        let completes = SimTime(
            self.state
                .time
                .0
                .checked_add(60_000)
                .ok_or(Error::Overflow)?,
        );
        self.interrupt_physical(speaker);
        self.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&speaker)
            .unwrap()
            .activity = Activity::MissionTalk { completes };
        self.state.missions.as_mut().unwrap().talks.insert(
            speaker,
            PendingTalk {
                speaker,
                listener,
                area,
                completes,
                conversation,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_mission_talk(&mut self, actor: PersonId) {
        let Some(world) = &self.state.missions else {
            return;
        };
        let affected: Vec<_> = world
            .talks
            .values()
            .filter(|talk| talk.speaker == actor || talk.listener == actor)
            .cloned()
            .collect();
        for talk in affected {
            self.state
                .missions
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            for actor in [talk.speaker] {
                if matches!(self.state.physical.as_ref().unwrap().actors[&actor].activity,Activity::MissionTalk {completes}if completes==talk.completes)
                {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .activity = Activity::Idle;
                }
            }
        }
    }
    pub(crate) fn mission_boundary(&self, limit: SimTime) -> SimTime {
        let Some(w) = &self.state.missions else {
            return limit;
        };
        w.talks
            .values()
            .map(|talk| talk.completes)
            .chain(
                w.missions
                    .values()
                    .filter(|m| {
                        !matches!(
                            m.status,
                            MissionStatus::Completed
                                | MissionStatus::Failed
                                | MissionStatus::Cancelled
                        )
                    })
                    .map(|m| m.proposal.deadline),
            )
            .chain(
                w.missions
                    .values()
                    .filter_map(|m| match m.proposal.objective {
                        MissionObjective::Patrol { until, .. }
                            if !matches!(
                                m.status,
                                MissionStatus::Completed
                                    | MissionStatus::Failed
                                    | MissionStatus::Cancelled
                            ) =>
                        {
                            Some(until)
                        }
                        _ => None,
                    }),
            )
            .filter(|at| *at > self.state.time)
            .min()
            .map_or(limit, |next| next.min(limit))
    }

    pub(crate) fn mission_view(&self, actor: PersonId) -> Vec<MissionView> {
        self.state
            .missions
            .as_ref()
            .and_then(|w| w.known.get(&actor))
            .map_or_else(Vec::new, |known| {
                known.values().rev().take(64).cloned().collect()
            })
    }
    fn observed_mission(&self, observer: PersonId, m: &Mission) -> MissionView {
        let w = self.state.missions.as_ref().unwrap();
        let old = w.known.get(&observer).and_then(|known| known.get(&m.id));
        let area = match self.state.people[self.person_index[&observer]].location {
            Location::AtArea(area) => Some(area),
            _ => None,
        };
        let leader_present = area.is_some() && self.mission_area(m.proposal.leader).ok() == area;
        let companions = m
            .members
            .values()
            .filter_map(|member| {
                let location = &self.state.people[self.person_index[&member.person]].location;
                if member.person == observer
                    || area.is_some_and(|area| *location == Location::AtArea(area))
                {
                    Some(CompanionView {
                        person: member.person,
                        status: member.status,
                        location: Some(location.clone()),
                        explanation: if member.person == observer {
                            member.explanation.clone()
                        } else {
                            member.declared_explanation.clone()
                        },
                        observed_at: self.state.time,
                    })
                } else {
                    old.and_then(|old| {
                        old.companions
                            .iter()
                            .find(|person| person.person == member.person)
                    })
                    .cloned()
                }
            })
            .collect();
        let status = if leader_present || observer == m.proposal.leader {
            m.status
        } else {
            old.map_or(m.status, |old| old.status)
        };
        let explanation = if leader_present || observer == m.proposal.leader {
            m.explanation.clone()
        } else {
            old.map_or_else(|| m.explanation.clone(), |old| old.explanation.clone())
        };
        MissionView {
            id: m.id,
            sponsor: m.sponsor,
            leader: m.proposal.leader,
            objective: if leader_present || old.is_none() {
                m.proposal.objective.clone()
            } else {
                old.map_or_else(|| m.proposal.objective.clone(), |old| old.objective.clone())
            },
            status,
            home: m.proposal.home,
            deadline: m.proposal.deadline,
            companions,
            recruitments: m
                .members
                .values()
                .filter(|member| {
                    member.person == observer
                        || (observer == m.sponsor
                            && area.is_some()
                            && self.mission_area(member.person).ok() == area)
                })
                .map(|member| {
                    let offer = &w.recruitments[&member.recruitment];
                    RecruitmentView {
                        id: offer.id,
                        person: offer.person,
                        status: member.status,
                        compensation: offer.compensation.clone(),
                        payment_agreement: offer.agreement,
                        countered: offer.countered,
                    }
                })
                .collect(),
            supplies: m.proposal.supplies.clone(),
            explanation,
            observed_at: if leader_present || observer == m.proposal.leader {
                self.state.time
            } else {
                old.map_or(self.state.time, |old| old.observed_at)
            },
            reported_by: None,
        }
    }
    fn publish_mission(&mut self, id: MissionId) {
        let m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        let anchor = self.mission_area(m.proposal.leader).ok();
        let people: Vec<_> = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|actor| anchor.is_some() && self.mission_area(*actor).ok() == anchor)
            .collect();
        for observer in people {
            let view = self.observed_mission(observer, &m);
            self.state
                .missions
                .as_mut()
                .unwrap()
                .known
                .entry(observer)
                .or_default()
                .insert(id, view);
        }
    }
    fn observe_own_mission(&mut self, actor: PersonId, id: MissionId) {
        let m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        let view = self.observed_mission(actor, &m);
        self.state
            .missions
            .as_mut()
            .unwrap()
            .known
            .entry(actor)
            .or_default()
            .insert(id, view);
    }
    fn recruitment_score(&self, actor: PersonId, m: &Mission, offer: &Recruitment) -> i64 {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        if self.guardian_mission_duty(actor, m.id) {
            return -10_000;
        }
        let violence = matches!(m.proposal.objective, MissionObjective::Defeat { .. });
        let risk = if violence {
            1_500 - i64::from(mind.personality.risk_tolerance)
        } else {
            300
        };
        let trust = i64::from(mind.relationships.get(&m.sponsor).map_or(0, |r| r.trust));
        let social_disruption = if self
            .mission_area(actor)
            .ok()
            .is_some_and(|area| self.mission_area(m.sponsor).ok() == Some(area))
        {
            self.invocation_social_penalty(m.sponsor) / 3
        } else {
            0
        };
        let kin_need = if mind
            .child_signals
            .values()
            .any(|signal| signal.urgency >= 20_000)
        {
            3_000
        } else {
            0
        };
        let mut compensation = 0;
        for term in &offer.compensation {
            if term.parties()[1] == actor {
                compensation += term.amount().min(10_000) as i64;
            } else if term.parties()[0] == actor {
                compensation -= term.amount().min(10_000) as i64;
            }
        }
        i64::from(mind.personality.ambition) / 2
            + i64::from(mind.personality.sociability) / 2
            + trust
            + compensation
            - social_disruption
            - risk
            - kin_need
            - if body.vitals.hunger >= self.state.physical.as_ref().unwrap().rules.meal_relief {
                2_000
            } else {
                0
            }
            - if violence {
                i64::from(mind.personality.empathy) / 2
            } else {
                0
            }
    }
    fn mission_control_member(&mut self, id: MissionId, actor: PersonId) {
        if actor == self.state.ruler || self.in_combat(actor) {
            return;
        }
        let mut m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        if let Some(view) = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .known
            .get(&actor)
            .and_then(|known| known.get(&id))
        {
            m.status = view.status;
        }
        let member = &m.members[&actor];
        if member.status != CompanionStatus::Accepted {
            return;
        }
        let Ok(area) = self.mission_area(actor) else {
            return;
        };
        if self.guardian_mission_duty(actor, id) {
            let _ = self.mission_command(actor, &MissionAction::Leave { mission: id });
            self.mission_explain(
                id,
                actor,
                "Withdrew to search for and care for the new or unready incarnation",
            );
            return;
        }
        let body = &self.state.physical.as_ref().unwrap().actors[&actor];
        let rules = &self.state.physical.as_ref().unwrap().rules;
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        if matches!(
            body.activity,
            Activity::MissionTalk { .. }
                | Activity::Eat { .. }
                | Activity::Load { .. }
                | Activity::Tell { .. }
                | Activity::Care { .. }
                | Activity::Lesson { .. }
                | Activity::WritingMemory { .. }
                | Activity::ReadingMemory { .. }
                | Activity::TeachingMemory { .. }
                | Activity::MovingRecord { .. }
                | Activity::Escape { .. }
                | Activity::Restrain { .. }
                | Activity::Unbind { .. }
        ) {
            return;
        }
        if mind
            .child_signals
            .values()
            .any(|signal| signal.urgency >= 20_000)
        {
            return;
        }
        if body.vitals.hunger >= rules.meal_relief {
            if let Some(food)=mind.knowledge.values().filter(|k|k.good==Good::Food&&k.accessible&&k.quantity>=rules.meal_size&&k.area==area).min_by_key(|k|(!matches!(self.state.inventories[&k.inventory].location,InventoryLocation::Carried(carrier)if carrier==actor),k.inventory)){let source=food.inventory;let _=self.debug_physical_command(actor,Command::Eat {source});return;}
            return;
        }
        if body.vitals.fatigue >= rules.fatigue_limit / 2
            || body.vitals.health < rules.max_health / 2
        {
            let _ = self.debug_physical_command(actor, Command::Rest);
            return;
        }
        let defaulted = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .recruitments
            .get(&member.recruitment)
            .and_then(|offer| offer.agreement)
            .is_some_and(|agreement| {
                self.agreement_view(actor).iter().any(|view| {
                    view.id == agreement
                        && view
                            .obligations
                            .iter()
                            .any(|o| o.defaulted && o.term.parties()[1] == actor)
                })
            });
        let leader_trust = mind
            .relationships
            .get(&m.proposal.leader)
            .map_or(0, |r| r.trust);
        if (defaulted && mind.personality.honesty < 800) || leader_trust < -500 {
            let _ = self.mission_command(actor, &MissionAction::Leave { mission: id });
            return;
        }
        if matches!(
            m.status,
            MissionStatus::Preparing | MissionStatus::Traveling
        ) {
            for supply in m.proposal.supplies.iter().filter(|s| s.carrier == actor) {
                let Some(known) = mind
                    .knowledge
                    .get(&supply.source)
                    .filter(|known| known.accessible)
                else {
                    self.mission_explain(
                        id,
                        actor,
                        "Waiting for personally known supply permission",
                    );
                    return;
                };
                let good = known.good;
                let held = self
                    .inventory_index
                    .get(&InventoryLocation::Carried(actor))
                    .into_iter()
                    .flatten()
                    .filter(|id| self.state.inventories[id].good == good)
                    .map(|id| self.state.inventories[id].quantity.0)
                    .sum::<u64>();
                if held >= supply.quantity.0 {
                    continue;
                }
                if known.area != area {
                    let destination = known.area;
                    let _ = self.start_movement(actor, destination);
                    return;
                }
                let quantity = Quantity((supply.quantity.0 - held).min(known.quantity.0));
                if quantity.0 == 0 {
                    self.mission_explain(id, actor, "Expected provisions are absent or depleted");
                    return;
                }
                let _ = self.debug_physical_command(
                    actor,
                    Command::Carry {
                        source: supply.source,
                        quantity,
                    },
                );
                return;
            }
            if m.status == MissionStatus::Preparing {
                let _ = self.debug_physical_command(actor, Command::Rest);
                return;
            }
        }
        let destination = if m.status == MissionStatus::Returning {
            Some(m.proposal.home)
        } else {
            self.mission_destination(actor, member, &member.directive)
        };
        if let Some(destination) = destination {
            if destination != area {
                if let MissionObjective::Rescue { person, return_to } = member.directive
                    && destination == return_to
                    && self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                    && self.mission_area(person).is_ok()
                    && person != self.state.ruler
                {
                    let released = &self.state.decisions.as_ref().unwrap().minds[&person];
                    let v = &self.state.physical.as_ref().unwrap().actors[&person].vitals;
                    let trusts = released.relationships.get(&actor).map_or(0, |r| r.trust) >= 0;
                    if trusts
                        && released.personality.empathy >= 300
                        && v.hunger < self.state.physical.as_ref().unwrap().rules.meal_relief
                    {
                        let _ = self.start_movement(person, return_to);
                    }
                }
                if self.start_movement(actor, destination).is_err() {
                    self.mission_explain(
                        id,
                        actor,
                        "No personally known usable route to the objective",
                    );
                }
                return;
            }
            match member.directive {
                MissionObjective::Defeat { person } => {
                    if self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                        && self.state.physical.as_ref().unwrap().actors[&person]
                            .vitals
                            .alive
                        && self.can_fight(person)
                    {
                        self.start_mission_encounter(id, actor, person);
                    }
                }
                MissionObjective::Rescue { person, .. } => {
                    if self.state.people[self.person_index[&person]].location
                        == Location::AtArea(area)
                        && self
                            .state
                            .combat
                            .as_ref()
                            .is_some_and(|w| w.captivity.contains_key(&person))
                    {
                        let _ = self
                            .debug_physical_command(actor, Command::FreeCaptive { target: person });
                    }
                }
                MissionObjective::Deliver {
                    source,
                    destination,
                    quantity,
                } => {
                    let goal = mind.knowledge.get(&destination).map(|k| k.area);
                    let cargo = member
                        .cargo
                        .iter()
                        .find(|(_, quantity)| **quantity > 0)
                        .map(|(id, quantity)| (*id, *quantity));
                    if goal == Some(area)
                        && let Some((source, held)) = cargo
                    {
                        let _ = self.debug_physical_command(
                            actor,
                            Command::Load {
                                source,
                                destination,
                                quantity: Quantity(held.min(quantity.0 - member.delivered)),
                            },
                        );
                    } else if let Some(known) = mind
                        .knowledge
                        .get(&source)
                        .filter(|k| k.area == area && k.accessible)
                    {
                        let amount = Quantity(
                            known
                                .quantity
                                .0
                                .min(quantity.0.saturating_sub(member.delivered)),
                        );
                        let _ = self.debug_physical_command(
                            actor,
                            Command::Carry {
                                source,
                                quantity: amount,
                            },
                        );
                    }
                }
                _ => {
                    let _ = self.debug_physical_command(actor, Command::Rest);
                }
            }
        } else {
            self.mission_explain(
                id,
                actor,
                "The party lacks a credible observed location for its objective",
            );
        }
    }
    fn mission_explain(&mut self, id: MissionId, actor: PersonId, message: &str) {
        self.state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&id)
            .unwrap()
            .members
            .get_mut(&actor)
            .unwrap()
            .explanation = message.into();
    }
    fn mission_destination(
        &self,
        actor: PersonId,
        member: &Companion,
        objective: &MissionObjective,
    ) -> Option<AreaId> {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        match objective {
            MissionObjective::Visit { destination } => Some(*destination),
            MissionObjective::Deliver {
                source,
                destination,
                ..
            } => {
                if member.cargo.values().any(|q| *q > 0) {
                    mind.knowledge.get(destination).map(|k| k.area)
                } else {
                    mind.knowledge.get(source).map(|k| k.area)
                }
            }
            MissionObjective::Rescue { person, return_to } => {
                if self.state.people[self.person_index[person]].location
                    == self.state.people[self.person_index[&actor]].location
                    && self
                        .state
                        .combat
                        .as_ref()
                        .is_none_or(|w| !w.captivity.contains_key(person))
                {
                    Some(*return_to)
                } else {
                    member.contacts.get(person).copied()
                }
            }
            MissionObjective::Defeat { person } => member.contacts.get(person).copied(),
            MissionObjective::Patrol { areas, .. } => areas
                .iter()
                .find(|area| !member.visited.contains(area))
                .copied(),
        }
    }
    fn start_mission_encounter(&mut self, id: MissionId, actor: PersonId, target: PersonId) {
        let area = match self.state.people[self.person_index[&actor]].location {
            Location::AtArea(area) => area,
            _ => return,
        };
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let allies: Vec<_> = m
            .members
            .values()
            .filter(|member| {
                member.person != self.state.ruler
                    && member.status == CompanionStatus::Accepted
                    && self.state.people[self.person_index[&member.person]].location
                        == Location::AtArea(area)
                    && self.can_fight(member.person)
                    && !self.in_combat(member.person)
            })
            .map(|member| (member.person, 0))
            .chain([(target, 1)])
            .collect();
        let _ = self.debug_start_encounter(allies);
    }
    pub(crate) fn mission_combat_order(
        &mut self,
        actor: PersonId,
        id: MissionId,
        objective: MissionObjective,
    ) -> Result<(), Error> {
        self.validate_mission_combat_order(actor, id, &objective)?;
        let leader = self.state.missions.as_ref().unwrap().missions[&id]
            .proposal
            .leader;
        self.apply_mission_order(id, leader, objective);
        Ok(())
    }
    fn apply_mission_order(
        &mut self,
        id: MissionId,
        leader: PersonId,
        objective: MissionObjective,
    ) {
        let area = self.mission_area(leader).ok();
        let actors: Vec<_> = self.state.missions.as_ref().unwrap().missions[&id]
            .members
            .values()
            .filter(|member| {
                member.status == CompanionStatus::Accepted
                    && self.mission_area(member.person).ok() == area
            })
            .map(|member| member.person)
            .collect();
        let mut proposed = self.state.missions.as_ref().unwrap().missions[&id].clone();
        proposed.proposal.objective = objective.clone();
        let decisions: Vec<_> = actors
            .into_iter()
            .map(|actor| {
                let offer = &self.state.missions.as_ref().unwrap().recruitments
                    [&proposed.members[&actor].recruitment];
                (
                    actor,
                    actor == self.state.ruler
                        || self.recruitment_score(actor, &proposed, offer) >= 0,
                )
            })
            .collect();
        self.state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&id)
            .unwrap()
            .proposal
            .objective = objective.clone();
        for (actor, willing) in decisions {
            let member = self
                .state
                .missions
                .as_mut()
                .unwrap()
                .missions
                .get_mut(&id)
                .unwrap()
                .members
                .get_mut(&actor)
                .unwrap();
            member.received_at = self.state.time;
            if willing {
                member.directive = objective.clone();
                member.declared_explanation = "Accepted a locally communicated objective".into();
                member.delivered = 0;
                member.cargo.clear();
                member.explanation =
                    "Received and accepted the locally communicated objective".into();
            } else {
                member.status = CompanionStatus::Withdrawn;
                member.declared_explanation = "Declined the changed objective".into();
                member.explanation =
                    "Declined a changed objective that conflicts with personal needs or values"
                        .into();
                self.emit(EventKind::MissionRecruitmentChanged {
                    mission: id,
                    person: actor,
                    status: CompanionStatus::Withdrawn,
                });
            }
        }
        self.emit(EventKind::MissionObjectiveChanged {
            mission: id,
            leader,
        });
        self.publish_mission(id);
    }
}

impl Simulation {
    /// Read actual committed events. A promised delivery never satisfies an objective.
    pub(crate) fn reconcile_missions(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.missions else {
            return Ok(());
        };
        self.ensure_event_capacity(
            world
                .talks
                .len()
                .checked_add(
                    world
                        .missions
                        .len()
                        .checked_mul(36)
                        .ok_or(Error::Overflow)?,
                )
                .ok_or(Error::Overflow)?,
        )?;
        let events: Vec<_> = self
            .state
            .events
            .iter()
            .skip(world.last_event as usize)
            .map(|event| event.kind.clone())
            .collect();
        for event in events {
            match event {
                EventKind::Departed { actor, from, .. } => {
                    if let Some(id) = self.mission_membership(actor) {
                        self.state
                            .missions
                            .as_mut()
                            .unwrap()
                            .missions
                            .get_mut(&id)
                            .unwrap()
                            .members
                            .get_mut(&actor)
                            .unwrap()
                            .last_leg_from = Some(from);
                    }
                }
                EventKind::Arrived { actor, area } => {
                    let observers: Vec<_> = self
                        .state
                        .people
                        .iter()
                        .filter(|p| p.location == Location::AtArea(area))
                        .filter_map(|p| self.mission_membership(p.id).map(|id| (id, p.id)))
                        .collect();
                    for (id, observer) in observers {
                        self.state
                            .missions
                            .as_mut()
                            .unwrap()
                            .missions
                            .get_mut(&id)
                            .unwrap()
                            .members
                            .get_mut(&observer)
                            .unwrap()
                            .contacts
                            .insert(actor, area);
                    }
                    if let Some(id) = self.mission_membership(actor) {
                        let contacts: Vec<_> = self
                            .state
                            .people
                            .iter()
                            .filter(|p| p.location == Location::AtArea(area))
                            .map(|p| p.id)
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .take(64)
                            .collect();
                        let member = self
                            .state
                            .missions
                            .as_mut()
                            .unwrap()
                            .missions
                            .get_mut(&id)
                            .unwrap()
                            .members
                            .get_mut(&actor)
                            .unwrap();
                        member.visited.insert(area);
                        for person in contacts {
                            member.contacts.insert(person, area);
                        }
                    }
                }
                EventKind::Transferred {
                    actor,
                    source,
                    destination,
                    quantity,
                } => self.mission_transfer(actor, source, destination, quantity.0),
                EventKind::Ate {
                    source, quantity, ..
                }
                | EventKind::Consumed {
                    source, quantity, ..
                }
                | EventKind::Cared {
                    source, quantity, ..
                } => self.lose_mission_cargo(source, quantity.0),
                EventKind::Spoiled {
                    inventory,
                    quantity,
                } => self.lose_mission_cargo(inventory, quantity.0),
                _ => {}
            }
        }
        let talks: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .talks
            .values()
            .filter(|talk| talk.completes <= self.state.time)
            .cloned()
            .collect();
        for talk in talks {
            let valid = self.mission_area(talk.speaker).ok() == Some(talk.area)
                && self.mission_area(talk.listener).ok() == Some(talk.area)
                && self.state.physical.as_ref().unwrap().actors[&talk.speaker].activity
                    == Activity::MissionTalk {
                        completes: talk.completes,
                    };
            self.state
                .missions
                .as_mut()
                .unwrap()
                .talks
                .remove(&talk.speaker);
            if matches!(self.state.physical.as_ref().unwrap().actors[&talk.speaker].activity,Activity::MissionTalk { completes } if completes == talk.completes)
            {
                self.state
                    .physical
                    .as_mut()
                    .unwrap()
                    .actors
                    .get_mut(&talk.speaker)
                    .unwrap()
                    .activity = Activity::Idle;
            }
            if !valid {
                continue;
            }
            match talk.conversation {
                Conversation::Objective {
                    mission,
                    leader,
                    objective,
                } => {
                    if self
                        .validate_mission_combat_order(talk.speaker, mission, &objective)
                        .is_ok()
                    {
                        self.apply_mission_order(mission, leader, objective);
                    }
                }
                Conversation::Retreat { mission, leader } => {
                    if !matches!(
                        self.state.missions.as_ref().unwrap().missions[&mission].status,
                        MissionStatus::Traveling | MissionStatus::Operating
                    ) {
                        continue;
                    }
                    let home = self.state.missions.as_ref().unwrap().missions[&mission]
                        .proposal
                        .home;
                    self.apply_mission_order(
                        mission,
                        leader,
                        MissionObjective::Visit { destination: home },
                    );
                    let m = self
                        .state
                        .missions
                        .as_mut()
                        .unwrap()
                        .missions
                        .get_mut(&mission)
                        .unwrap();
                    m.status = MissionStatus::Returning;
                    m.succeeded = false;
                    m.explanation =
                        "Leader communicated a retreat; participants decide whether to return"
                            .into();
                    self.publish_mission(mission);
                }
                Conversation::Report {
                    recipient,
                    mut snapshot,
                } => {
                    snapshot.reported_by = Some(talk.speaker);
                    let mission = snapshot.id;
                    let known = self
                        .state
                        .missions
                        .as_mut()
                        .unwrap()
                        .known
                        .entry(recipient)
                        .or_default();
                    if known
                        .get(&mission)
                        .is_none_or(|old| old.observed_at <= snapshot.observed_at)
                    {
                        known.insert(mission, *snapshot);
                    }
                    self.emit(EventKind::MissionReported {
                        mission,
                        speaker: talk.speaker,
                        recipient,
                    });
                }
            }
        }
        let ids: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .copied()
            .collect();
        for id in ids {
            self.reconcile_mission_outcome(id);
            let actors: Vec<_> = self.state.missions.as_ref().unwrap().missions[&id]
                .members
                .keys()
                .copied()
                .collect();
            for actor in actors {
                self.observe_own_mission(actor, id);
            }
            self.publish_mission(id);
        }
        self.state.missions.as_mut().unwrap().last_event = self.state.next_event - 1;
        Ok(())
    }
    fn mission_transfer(
        &mut self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: u64,
    ) {
        // Remove the tagged portion first even if another person steals or consumes it.
        let Some(w) = &mut self.state.missions else {
            return;
        };
        for m in w.missions.values_mut() {
            if matches!(
                m.status,
                MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
            ) {
                continue;
            }
            for member in m.members.values_mut() {
                let tagged = member
                    .cargo
                    .get(&source)
                    .copied()
                    .unwrap_or(0)
                    .min(quantity);
                if tagged > 0 {
                    let left = member.cargo[&source] - tagged;
                    if left == 0 {
                        member.cargo.remove(&source);
                    } else {
                        member.cargo.insert(source, left);
                    }
                }
                if member.person != actor || member.status != CompanionStatus::Accepted {
                    continue;
                }
                let MissionObjective::Deliver {
                    source: origin,
                    destination: goal,
                    quantity: owed,
                } = member.directive
                else {
                    continue;
                };
                let picked = if source == origin {
                    quantity.min(owed.0.saturating_sub(member.delivered))
                } else {
                    tagged
                };
                if destination == goal && source != goal && picked > 0 {
                    member.delivered = member.delivered.saturating_add(picked).min(owed.0);
                    member.explanation = format!("Physically delivered {} units", member.delivered);
                } else if picked > 0
                    && self
                        .state
                        .inventories
                        .get(&destination)
                        .is_some_and(|i| i.location == InventoryLocation::Carried(actor))
                {
                    let held = member.cargo.entry(destination).or_default();
                    *held = held
                        .saturating_add(picked)
                        .min(owed.0.saturating_sub(member.delivered));
                }
            }
        }
    }
    fn lose_mission_cargo(&mut self, source: InventoryId, quantity: u64) {
        if let Some(w) = &mut self.state.missions {
            for m in w.missions.values_mut() {
                for member in m.members.values_mut() {
                    if let Some(held) = member.cargo.get_mut(&source) {
                        *held = held.saturating_sub(quantity);
                        if *held == 0 {
                            member.cargo.remove(&source);
                        }
                    }
                }
            }
        }
    }
    fn reconcile_mission_outcome(&mut self, id: MissionId) {
        let m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        if matches!(
            m.status,
            MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
        ) {
            return;
        }
        let changes: Vec<_> = m
            .members
            .values()
            .filter_map(|member| {
                if !matches!(
                    member.status,
                    CompanionStatus::Accepted | CompanionStatus::Captured
                ) {
                    return None;
                }
                let body = &self.state.physical.as_ref().unwrap().actors[&member.person];
                let status = if !body.vitals.alive {
                    CompanionStatus::Dead
                } else if self
                    .state
                    .combat
                    .as_ref()
                    .is_some_and(|w| w.captivity.contains_key(&member.person))
                {
                    CompanionStatus::Captured
                } else if member.status == CompanionStatus::Captured {
                    CompanionStatus::Accepted
                } else {
                    return None;
                };
                Some((member.person, status))
            })
            .collect();
        for (person, status) in changes {
            let member = self
                .state
                .missions
                .as_mut()
                .unwrap()
                .missions
                .get_mut(&id)
                .unwrap()
                .members
                .get_mut(&person)
                .unwrap();
            member.status = status;
            member.declared_explanation = match status {
                CompanionStatus::Dead => "Died",
                CompanionStatus::Captured => "Captured",
                _ => "Released",
            }
            .into();
            member.explanation = match status {
                CompanionStatus::Dead => "Died during the expedition",
                CompanionStatus::Captured => "Captured; unable to travel with the expedition",
                _ => "Released; may choose to resume the expedition",
            }
            .into();
            self.emit(EventKind::MissionRecruitmentChanged {
                mission: id,
                person,
                status,
            });
        }
        let m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        if !matches!(
            m.members[&m.proposal.leader].status,
            CompanionStatus::Accepted | CompanionStatus::Offered
        ) {
            self.finish_mission(id, false, "The leader can no longer lead this expedition");
            return;
        }
        if m.status == MissionStatus::Preparing {
            if self.state.time >= m.proposal.deadline {
                self.finish_mission(
                    id,
                    false,
                    "Recruitment or preparation missed the agreed deadline",
                );
            }
            return;
        }
        let leader_area = self.mission_area(m.proposal.leader).ok();
        let achieved = match &m.proposal.objective {
            MissionObjective::Visit { destination } => leader_area == Some(*destination),
            MissionObjective::Deliver { quantity, .. } => {
                m.members
                    .values()
                    .filter(|p| {
                        self.mission_area(p.person).ok() == leader_area && leader_area.is_some()
                    })
                    .map(|p| p.delivered)
                    .sum::<u64>()
                    >= quantity.0
            }
            MissionObjective::Rescue { person, return_to } => {
                leader_area == Some(*return_to)
                    && self.state.people[self.person_index[person]].location
                        == Location::AtArea(*return_to)
                    && !self.combat_restricts(*person)
            }
            MissionObjective::Defeat { person } => {
                leader_area.is_some_and(|area| {
                    self.state.people[self.person_index[person]].location == Location::AtArea(area)
                }) && (!self.state.physical.as_ref().unwrap().actors[person]
                    .vitals
                    .alive
                    || self.state.physical.as_ref().unwrap().actors[person]
                        .vitals
                        .health
                        <= self.state.combat.as_ref().unwrap().rules.incapacitation
                    || self.state.combat.as_ref().is_some_and(|w| {
                        w.captivity
                            .get(person)
                            .is_some_and(|c| m.members.contains_key(&c.captor))
                    }))
            }
            MissionObjective::Patrol { areas, until } => {
                self.state.time >= *until
                    && areas
                        .iter()
                        .all(|area| m.members[&m.proposal.leader].visited.contains(area))
            }
        };
        if m.status == MissionStatus::Traveling
            && !achieved
            && leader_area.is_some()
            && matches!(
                m.proposal.objective,
                MissionObjective::Defeat { .. }
                    | MissionObjective::Rescue { .. }
                    | MissionObjective::Patrol { .. }
            )
            && self.mission_destination(
                m.proposal.leader,
                &m.members[&m.proposal.leader],
                &m.proposal.objective,
            ) == leader_area
        {
            let mission = self
                .state
                .missions
                .as_mut()
                .unwrap()
                .missions
                .get_mut(&id)
                .unwrap();
            mission.status = MissionStatus::Operating;
            mission.explanation =
                "Leader is present at the objective and undertaking the local task".into();
        }
        if m.status != MissionStatus::Returning
            && (achieved || self.state.time >= m.proposal.deadline)
        {
            let m = self
                .state
                .missions
                .as_mut()
                .unwrap()
                .missions
                .get_mut(&id)
                .unwrap();
            m.status = MissionStatus::Returning;
            m.succeeded = achieved;
            m.explanation = if achieved {
                "Leader observed the objective achieved and is returning"
            } else {
                "The objective deadline passed; leader is returning without success"
            }
            .into();
        }
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        if m.status == MissionStatus::Returning && leader_area == Some(m.proposal.home) {
            self.finish_mission(
                id,
                m.succeeded,
                if m.succeeded {
                    "Leader returned after observing the objective achieved"
                } else {
                    "Leader returned after retreat or failure"
                },
            );
        }
    }
    fn finish_mission(&mut self, id: MissionId, success: bool, explanation: &str) {
        let m = self
            .state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&id)
            .unwrap();
        m.status = if success {
            MissionStatus::Completed
        } else {
            MissionStatus::Failed
        };
        m.explanation = explanation.into();
        let status = m.status;
        for member in m.members.values_mut() {
            member.cargo.clear();
        }
        self.emit(EventKind::MissionFinished {
            mission: id,
            status,
        });
        // Only a person receiving a local outcome learns a relationship consequence.
        let m = self.state.missions.as_ref().unwrap().missions[&id].clone();
        let home = self.mission_area(m.proposal.leader).ok();
        let people: Vec<_> = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|person| {
                *person != m.proposal.leader
                    && home.is_some()
                    && self.mission_area(*person).ok() == home
            })
            .collect();
        for person in people {
            let relationship = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&person)
                .unwrap()
                .relationships
                .entry(m.proposal.leader)
                .or_insert(decisions::Relationship {
                    trust: 0,
                    affection: 0,
                });
            relationship.trust =
                (relationship.trust + if success { 100 } else { -50 }).clamp(-1_000, 1_000);
        }
    }
    pub(crate) fn review_missions(&mut self) -> Result<(), Error> {
        let Some(w) = &self.state.missions else {
            return Ok(());
        };
        if self.state.time == w.last_review || !self.state.time.0.is_multiple_of(60_000) {
            return Ok(());
        }
        self.preflight_missions()?;
        self.state.missions.as_mut().unwrap().last_review = self.state.time;
        let offers: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .recruitments
            .values()
            .cloned()
            .collect();
        for offer in offers {
            if offer.person == self.state.ruler {
                continue;
            }
            let m = self.state.missions.as_ref().unwrap().missions[&offer.mission].clone();
            if m.status != MissionStatus::Preparing
                || m.members[&offer.person].status != CompanionStatus::Offered
                || offer.countered
                || self
                    .mission_area(offer.person)
                    .ok()
                    .is_none_or(|area| self.mission_area(m.sponsor).ok() != Some(area))
            {
                continue;
            }
            let score = self.recruitment_score(offer.person, &m, &offer);
            let counter = if (-500..0).contains(&score) && offer.compensation.is_empty() {
                let mind = &self.state.decisions.as_ref().unwrap().minds[&offer.person];
                let area = self.mission_area(offer.person).ok();
                mind.knowledge
                    .values()
                    .find(|stock| {
                        stock.good == Good::Food
                            && stock.accessible
                            && stock.quantity.0 >= 1_000
                            && Some(stock.area) == area
                            && stock.inventory
                                != self.state.people[self.person_index[&offer.person]].inventory
                    })
                    .map(|stock| AgreementTerm::Delivery {
                        debtor: m.sponsor,
                        creditor: offer.person,
                        source: stock.inventory,
                        destination: self.state.people[self.person_index[&offer.person]].inventory,
                        quantity: Quantity(1_000),
                        deadline: m.proposal.deadline,
                    })
            } else {
                None
            };
            let action = if score >= 0 {
                MissionAction::Accept {
                    recruitment: offer.id,
                }
            } else if let Some(term) = counter {
                MissionAction::Counteroffer {
                    recruitment: offer.id,
                    compensation: vec![term],
                }
            } else {
                MissionAction::Refuse {
                    recruitment: offer.id,
                }
            };
            // An unavailable material promise remains an unanswered offer; never invent stock.
            let _ = self.mission_command(offer.person, &action);
        }
        let missions: Vec<_> = self
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .values()
            .cloned()
            .collect();
        for m in missions {
            if m.status == MissionStatus::Preparing
                && m.proposal.leader != self.state.ruler
                && m.members[&m.proposal.leader].status == CompanionStatus::Accepted
                && m.members
                    .values()
                    .all(|person| person.status != CompanionStatus::Offered)
            {
                let _ = self
                    .mission_command(m.proposal.leader, &MissionAction::Depart { mission: m.id });
            }
            for person in m.members.keys() {
                if self.mission_membership(*person) == Some(m.id) {
                    self.mission_control_member(m.id, *person);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn mission_allocation_bound(&self) -> u64 {
        self.state.missions.as_ref().map_or(0, |w| {
            w.missions
                .values()
                .flat_map(|m| m.members.values())
                .filter(|member| {
                    matches!(
                        member.status,
                        CompanionStatus::Accepted | CompanionStatus::Offered
                    )
                })
                .count() as u64
                * 12
        })
    }
    pub(crate) fn mission_event_bound(&self) -> usize {
        self.state.missions.as_ref().map_or(0, |w| {
            w.missions
                .values()
                .map(|m| m.members.len() * 32 + 4)
                .sum::<usize>()
                + w.talks.len() * 33
        })
    }
    pub(crate) fn preflight_missions(&self) -> Result<(), Error> {
        let Some(w) = &self.state.missions else {
            return Ok(());
        };
        let people = w
            .missions
            .values()
            .filter(|m| {
                !matches!(
                    m.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                )
            })
            .map(|m| m.members.len() as u64)
            .sum::<u64>();
        self.state
            .next_id
            .checked_add(people.checked_mul(12).ok_or(Error::Overflow)?)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(
            people
                .checked_mul(32)
                .and_then(|n| n.checked_add(w.talks.len() as u64 * 2 + w.missions.len() as u64 * 4))
                .ok_or(Error::Overflow)?
                .try_into()
                .map_err(|_| Error::Overflow)?,
        )
    }
}

impl Simulation {
    pub(crate) fn available_mission_commands(&self, actor: PersonId) -> Vec<Command> {
        let Ok(area) = self.mission_area(actor) else {
            return Vec::new();
        };
        if self.in_combat(actor) {
            return Vec::new();
        }
        let Some(w) = &self.state.missions else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        for view in self.mission_view(actor) {
            let Some(m) = w.missions.get(&view.id) else {
                continue;
            };
            for offer in &view.recruitments {
                if offer.status != CompanionStatus::Offered {
                    continue;
                }
                if offer.person == actor && self.mission_area(m.sponsor).ok() == Some(area) {
                    if !offer.countered
                        && view.status == MissionStatus::Preparing
                        && !self.guardian_mission_duty(actor, m.id)
                        && !self.state.decisions.as_ref().unwrap().minds[&actor]
                            .child_signals
                            .values()
                            .any(|signal| signal.urgency >= 20_000)
                    {
                        actions.push(MissionAction::Accept {
                            recruitment: offer.id,
                        });
                    }
                    actions.push(MissionAction::Refuse {
                        recruitment: offer.id,
                    });
                } else if actor == m.sponsor
                    && offer.countered
                    && self.mission_area(offer.person).ok() == Some(area)
                {
                    actions.push(MissionAction::AcceptCounteroffer {
                        recruitment: offer.id,
                    });
                }
            }
            if m.members
                .get(&actor)
                .is_some_and(|member| member.status == CompanionStatus::Accepted)
            {
                actions.push(MissionAction::Leave { mission: m.id });
            }
            if self.mission_area(m.proposal.leader).ok() == Some(area)
                && (actor == m.sponsor || actor == m.proposal.leader)
            {
                if view.status == MissionStatus::Preparing
                    && m.members[&m.proposal.leader].status == CompanionStatus::Accepted
                {
                    actions.push(MissionAction::Depart { mission: m.id });
                }
                if matches!(
                    view.status,
                    MissionStatus::Traveling | MissionStatus::Operating
                ) {
                    actions.push(MissionAction::Retreat { mission: m.id });
                }
            }
            for recipient in self
                .state
                .people
                .iter()
                .filter(|p| p.id != actor && p.location == Location::AtArea(area))
                .map(|p| p.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
            {
                if self.mission_area(recipient).is_ok() {
                    actions.push(MissionAction::Report {
                        mission: m.id,
                        recipient,
                    });
                }
            }
        }
        actions
            .into_iter()
            .map(|action| Command::Mission { action })
            .collect()
    }
    /// A personal judgment may change sides; no mission sponsor decides this for a companion.
    pub(crate) fn review_mission_combat(&mut self, encounter: EncounterId, actor: PersonId) {
        if actor == self.state.ruler {
            return;
        }
        let Some(id) = self.mission_membership(actor) else {
            return;
        };
        let Some(e) = self
            .state
            .combat
            .as_ref()
            .and_then(|w| w.encounters.get(&encounter))
        else {
            return;
        };
        let Some(f) = e.fighters.get(&actor) else {
            return;
        };
        let map = &self.state.combat.as_ref().unwrap().maps[&e.area];
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
        if mind.personality.honesty >= 500 {
            return;
        }
        let leader_trust = mind
            .relationships
            .get(&m.proposal.leader)
            .map_or(0, |r| r.trust);
        let Some(other) = f
            .known
            .keys()
            .filter(|other| {
                e.fighters.get(other).is_some_and(|other| {
                    other.team != f.team
                        && !other.withdrawn
                        && !other.surrendered
                        && map.clear(f.position, other.position)
                })
            })
            .max_by_key(|other| {
                (
                    mind.relationships.get(other).map_or(-1_000, |r| r.trust),
                    **other,
                )
            })
            .copied()
        else {
            return;
        };
        let other_trust = mind.relationships.get(&other).map_or(-1_000, |r| r.trust);
        let payment_failed = self.state.missions.as_ref().unwrap().recruitments
            [&m.members[&actor].recruitment]
            .agreement
            .is_some_and(|agreement| {
                self.agreement_view(actor).iter().any(|a| {
                    a.id == agreement
                        && a.obligations
                            .iter()
                            .any(|o| o.defaulted && o.term.parties()[1] == actor)
                })
            });
        if other_trust < leader_trust + 500 || !(payment_failed || leader_trust < -500) {
            return;
        }
        let team = e.fighters[&other].team;
        self.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&encounter)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .team = team;
        let member = self
            .state
            .missions
            .as_mut()
            .unwrap()
            .missions
            .get_mut(&id)
            .unwrap()
            .members
            .get_mut(&actor)
            .unwrap();
        member.status = CompanionStatus::Defected;
        member.declared_explanation = "Changed allegiance during combat".into();
        member.explanation =
            "Chose a trusted opponent over a distrusted or defaulting leader".into();
        self.emit(EventKind::MissionRecruitmentChanged {
            mission: id,
            person: actor,
            status: CompanionStatus::Defected,
        });
        self.observe_own_mission(actor, id);
    }
    pub(crate) fn mission_combat_action(
        &self,
        encounter: EncounterId,
        actor: PersonId,
    ) -> Option<(Option<Point>, CombatAction)> {
        // The ruler takes personal turns; an invoked life retains its own controller.
        if actor == self.state.ruler {
            return None;
        }
        let id = self.mission_membership(actor)?;
        let m = &self.state.missions.as_ref()?.missions[&id];
        let member = &m.members[&actor];
        let w = self.state.combat.as_ref()?;
        let e = w.encounters.get(&encounter)?;
        let f = e.fighters.get(&actor)?;
        let map = &w.maps[&e.area];
        let mind = &self.state.decisions.as_ref()?.minds[&actor];
        let body = &self.state.physical.as_ref()?.actors[&actor];
        let retreat = body.vitals.health < self.state.physical.as_ref()?.rules.max_health / 2
            && mind.personality.risk_tolerance < 600
            || self
                .state
                .missions
                .as_ref()?
                .known
                .get(&actor)
                .and_then(|known| known.get(&id))
                .is_some_and(|view| view.status == MissionStatus::Returning);
        if retreat {
            let exit = map
                .exits
                .iter()
                .filter(|(area, _)| decisions::known_path(&mind.routes, e.area, *area).is_ok())
                .min_by_key(|(area, point)| (point_distance(f.position, *point), *area));
            if let Some((destination, exit)) = exit {
                let movement = mission_combat_step(f.position, *exit, w.rules.movement, map);
                let position = movement.unwrap_or(f.position);
                let action = if point_distance(position, *exit) <= u128::from(w.rules.reach).pow(2)
                {
                    CombatAction::Retreat {
                        destination: *destination,
                    }
                } else {
                    CombatAction::Defend
                };
                return Some((movement, action));
            }
            return Some((None, CombatAction::Surrender));
        }
        let target = match member.directive {
            MissionObjective::Defeat { person } | MissionObjective::Rescue { person, .. }
                if f.known.contains_key(&person) =>
            {
                Some(person)
            }
            _ => f
                .known
                .keys()
                .filter(|person| {
                    e.fighters.get(person).is_some_and(|other| {
                        other.team != f.team && map.clear(f.position, other.position)
                    })
                })
                .min_by_key(|person| (point_distance(f.position, f.known[person]), **person))
                .copied(),
        };
        let Some(target) = target else {
            return Some((None, CombatAction::Defend));
        };
        let point = f.known[&target];
        let distance = point_distance(f.position, point).isqrt() as u64;
        let movement = mission_combat_step(
            f.position,
            point,
            distance
                .saturating_sub(w.rules.reach.saturating_sub(2))
                .min(w.rules.movement),
            map,
        );
        let from = movement.unwrap_or(f.position);
        let reachable = point_distance(from, point) <= u128::from(w.rules.reach).pow(2)
            && map.clear(from, point);
        let action = if reachable
            && matches!(member.directive, MissionObjective::Rescue { person, .. } if person == target)
            && self.can_release(actor, target)
        {
            CombatAction::Release { target }
        } else if reachable
            && e.fighters[&target].position == point
            && map.clear(from, e.fighters[&target].position)
            && e.fighters[&target].team != f.team
            && !e.fighters[&target].surrendered
        {
            CombatAction::Attack { target }
        } else {
            CombatAction::Defend
        };
        Some((movement, action))
    }
    pub(crate) fn mission_reinforcements(&self) -> Vec<(EncounterId, PersonId, u32, AreaId)> {
        let Some(w) = &self.state.missions else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for m in w.missions.values() {
            for member in m.members.values().filter(|p| {
                p.status == CompanionStatus::Accepted
                    && self.mission_membership(p.person) == Some(m.id)
                    && p.person != self.state.ruler
                    && self.can_fight(p.person)
                    && !self.in_combat(p.person)
            }) {
                let Some(from) = member.last_leg_from else {
                    continue;
                };
                let Ok(area) = self.mission_area(member.person) else {
                    continue;
                };
                if !self.state.events.iter().rev().take_while(|event|event.time==self.state.time).any(|event|event.time==self.state.time && matches!(event.kind,EventKind::Arrived {actor,area:arrived} if actor==member.person && arrived==area)) {continue;}
                for encounter in self
                    .state
                    .combat
                    .as_ref()
                    .into_iter()
                    .flat_map(|w| w.encounters.values())
                    .filter(|e| e.active && e.area == area)
                {
                    if let Some(ally) = encounter.fighters.values().find(|f| {
                        m.members
                            .get(&f.actor)
                            .is_some_and(|p| p.status == CompanionStatus::Accepted)
                            && !f.withdrawn
                            && !f.surrendered
                    }) {
                        candidates.push((encounter.id, member.person, ally.team, from));
                        break;
                    }
                }
            }
        }
        candidates
    }
}
fn point_distance(a: Point, b: Point) -> u128 {
    u128::from(a.x.abs_diff(b.x)).pow(2) + u128::from(a.y.abs_diff(b.y)).pow(2)
}
fn mission_combat_step(from: Point, to: Point, limit: u64, map: &combat::Map) -> Option<Point> {
    let square = point_distance(from, to);
    let floor = square.isqrt();
    let d = (floor + u128::from(floor * floor < square)) as i64;
    if d == 0 || limit == 0 {
        return None;
    }
    let step = (limit as i64).min(d);
    let point = Point {
        x: from.x + (to.x - from.x) * step / d,
        y: from.y + (to.y - from.y) * step / d,
    };
    (point != from && map.clear(from, point)).then_some(point)
}

pub(crate) fn validate(s: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &s.missions else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent expedition commitments".into());
    let people: BTreeSet<_> = s.people.iter().map(|p| p.id).collect();
    let areas: BTreeSet<_> = s.areas.iter().map(|a| a.id).collect();
    let physical = s.physical.as_ref().ok_or_else(fail)?;
    let agreements = s.agreements.as_ref().ok_or_else(fail)?;
    if s.decisions.is_none()
        || s.combat.is_none()
        || w.last_event >= s.next_event
        || w.last_review > s.time
    {
        return Err(fail());
    }
    let objective_valid = |objective: &MissionObjective| match objective {
        MissionObjective::Visit { destination } => areas.contains(destination),
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => {
            quantity.0 > 0
                && s.inventories
                    .get(source)
                    .zip(s.inventories.get(destination))
                    .is_some_and(|(a, b)| {
                        a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                    })
        }
        MissionObjective::Defeat { person } => people.contains(person),
        MissionObjective::Rescue { person, return_to } => {
            people.contains(person) && areas.contains(return_to)
        }
        MissionObjective::Patrol {
            areas: selected, ..
        } => {
            !selected.is_empty()
                && selected.len() <= 16
                && selected.iter().all(|a| areas.contains(a))
        }
    };
    for (id, m) in &w.missions {
        let roster: BTreeSet<_> = m
            .proposal
            .recruits
            .iter()
            .copied()
            .chain([m.proposal.leader])
            .collect();
        if *id != m.id
            || id.0 == 0
            || id.0 >= s.next_id
            || !people.contains(&m.sponsor)
            || !areas.contains(&m.proposal.home)
            || !objective_valid(&m.proposal.objective)
            || !(1..=32).contains(&m.members.len())
            || roster != m.members.keys().copied().collect()
            || m.created_at > s.time
            || m.proposal.deadline <= m.created_at
            || m.departed_at
                .is_some_and(|at| at < m.created_at || at > s.time)
            || m.proposal.supplies.len() > 32
        {
            return Err(fail());
        }
        for (person, terms) in &m.proposal.compensation {
            if !m.members.contains_key(person)
                || terms.len() > 8
                || terms.iter().any(|term| {
                    term.amount() == 0
                        || term.parties()[0] == term.parties()[1]
                        || term
                            .parties()
                            .iter()
                            .any(|p| *p != m.sponsor && *p != *person)
                        || match term {
                            AgreementTerm::Delivery {
                                source,
                                destination,
                                quantity,
                                ..
                            } => !s
                                .inventories
                                .get(source)
                                .zip(s.inventories.get(destination))
                                .is_some_and(|(a, b)| {
                                    a.id != b.id && a.good == b.good && quantity.0 <= b.capacity.0
                                }),
                            AgreementTerm::Access { inventory, .. } => {
                                !s.inventories.contains_key(inventory)
                            }
                            _ => true,
                        }
                })
            {
                return Err(fail());
            }
        }
        for supply in &m.proposal.supplies {
            if !m.members.contains_key(&supply.carrier)
                || !s.inventories.contains_key(&supply.source)
                || supply.quantity.0 == 0
                || supply.quantity > physical.rules.carrying_capacity
            {
                return Err(fail());
            }
        }
        for (person, member) in &m.members {
            let offer = w.recruitments.get(&member.recruitment).ok_or_else(fail)?;
            if *person != member.person
                || !people.contains(person)
                || offer.mission != *id
                || offer.person != *person
                || member.received_at > s.time
                || !objective_valid(&member.directive)
                || member.visited.iter().any(|a| !areas.contains(a))
                || member
                    .contacts
                    .iter()
                    .any(|(p, a)| !people.contains(p) || !areas.contains(a))
                || member
                    .last_leg_from
                    .is_some_and(|area| !areas.contains(&area))
                || member.explanation.len() > 4096
                || member.declared_explanation.len() > 4096
            {
                return Err(fail());
            }
            if let MissionObjective::Deliver { quantity, .. } = member.directive
                && member.delivered > quantity.0
            {
                return Err(fail());
            }
            for (inventory, quantity) in &member.cargo {
                if *quantity == 0
                    || !s.inventories.get(inventory).is_some_and(|i| {
                        i.location == InventoryLocation::Carried(*person)
                            && i.quantity.0 >= *quantity
                    })
                {
                    return Err(fail());
                }
            }
        }
    }
    for (id, offer) in &w.recruitments {
        let m = w.missions.get(&offer.mission).ok_or_else(fail)?;
        if *id != offer.id
            || id.0 == 0
            || id.0 >= s.next_id
            || !m
                .members
                .get(&offer.person)
                .is_some_and(|person| person.recruitment == *id)
            || offer.compensation.len() > 8
            || offer.agreement.is_some_and(|id| {
                !agreements
                    .agreements
                    .get(&id)
                    .is_some_and(|a| a.parties == BTreeSet::from([m.sponsor, offer.person]))
            })
        {
            return Err(fail());
        }
        for term in &offer.compensation {
            if term.amount() == 0
                || term.parties()[0] == term.parties()[1]
                || term
                    .parties()
                    .iter()
                    .any(|p| *p != m.sponsor && *p != offer.person)
                || !matches!(
                    term,
                    AgreementTerm::Delivery { .. } | AgreementTerm::Access { .. }
                )
            {
                return Err(fail());
            }
            match term {
                AgreementTerm::Delivery {
                    source,
                    destination,
                    ..
                } if !s.inventories.contains_key(source)
                    || !s.inventories.contains_key(destination) =>
                {
                    return Err(fail());
                }
                AgreementTerm::Access { inventory, .. }
                    if !s.inventories.contains_key(inventory) =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
    }
    for (observer, views) in &w.known {
        if !people.contains(observer) {
            return Err(fail());
        }
        for (id, view) in views {
            if *id != view.id
                || !w.missions.contains_key(id)
                || view.observed_at > s.time
                || !people.contains(&view.sponsor)
                || !people.contains(&view.leader)
                || !areas.contains(&view.home)
                || !objective_valid(&view.objective)
                || view.companions.len() > 32
                || view.recruitments.len() > 32
                || view.reported_by.is_some_and(|p| !people.contains(&p))
            {
                return Err(fail());
            }
            for companion in &view.companions {
                if !people.contains(&companion.person) || companion.observed_at > s.time {
                    return Err(fail());
                }
            }
            for recruitment in &view.recruitments {
                if !w
                    .recruitments
                    .get(&recruitment.id)
                    .is_some_and(|offer| offer.person == recruitment.person && offer.mission == *id)
                {
                    return Err(fail());
                }
            }
        }
    }
    let mut occupied = BTreeSet::new();
    for (speaker, talk) in &w.talks {
        if *speaker != talk.speaker
            || !people.contains(speaker)
            || !people.contains(&talk.listener)
            || !areas.contains(&talk.area)
            || talk.completes <= s.time
        {
            return Err(fail());
        }
        if !occupied.insert(talk.speaker)
            || physical.actors[&talk.speaker].activity
                != (Activity::MissionTalk {
                    completes: talk.completes,
                })
            || ![talk.speaker, talk.listener].iter().all(|actor| {
                s.people
                    .iter()
                    .any(|p| p.id == *actor && p.location == Location::AtArea(talk.area))
            })
        {
            return Err(fail());
        }
        match &talk.conversation {
            Conversation::Objective {
                mission,
                leader,
                objective,
            } => {
                if !w.missions.get(mission).is_some_and(|m| {
                    m.proposal.leader == *leader && (m.sponsor == *speaker || *leader == *speaker)
                }) || !objective_valid(objective)
                {
                    return Err(fail());
                }
            }
            Conversation::Retreat { mission, leader } => {
                if !w.missions.get(mission).is_some_and(|m| {
                    m.proposal.leader == *leader && (m.sponsor == *speaker || *leader == *speaker)
                }) {
                    return Err(fail());
                }
            }
            Conversation::Report {
                recipient,
                snapshot,
            } => {
                if *recipient != talk.listener
                    || !w.missions.contains_key(&snapshot.id)
                    || snapshot.observed_at > s.time
                {
                    return Err(fail());
                }
            }
        }
    }
    for (actor, body) in &physical.actors {
        if matches!(body.activity, Activity::MissionTalk { .. }) != occupied.contains(actor) {
            return Err(fail());
        }
    }
    Ok(())
}

impl Simulation {
    pub(crate) fn validate_mission_combat_order(
        &self,
        actor: PersonId,
        id: MissionId,
        objective: &MissionObjective,
    ) -> Result<(), Error> {
        let area = self.mission_area(actor)?;
        let m = self
            .state
            .missions
            .as_ref()
            .and_then(|w| w.missions.get(&id))
            .ok_or(Error::Unavailable)?;
        if (actor != m.sponsor && actor != m.proposal.leader)
            || self.mission_area(m.proposal.leader).ok() != Some(area)
            || !matches!(
                m.status,
                MissionStatus::Preparing
                    | MissionStatus::Traveling
                    | MissionStatus::Operating
                    | MissionStatus::Returning
            )
            || m.members[&m.proposal.leader].status != CompanionStatus::Accepted
            || !self.mission_objective_valid(objective)
            || matches!(objective, MissionObjective::Defeat { person } if m.members.contains_key(person) || *person == m.sponsor)
        {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(m.members.len() + 1)
    }
    fn guardian_mission_duty(&self, actor: PersonId, id: MissionId) -> bool {
        if self.state.people[self.person_index[&actor]].role != Role::Guardian {
            return false;
        }
        let Some(lineage) = &self.state.lineage else {
            return false;
        };
        let m = &self.state.missions.as_ref().unwrap().missions[&id];
        lineage.current() != Some(m.sponsor)
            || self.state.decisions.as_ref().unwrap().minds[&actor]
                .child_signals
                .get(&m.sponsor)
                .is_some_and(|signal| signal.urgency > 0)
            || !lineage
                .guardians
                .get(&actor)
                .and_then(|g| g.assessments.get(&m.sponsor))
                .is_some_and(|a| a.ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Simulation {
        let mut sim = Simulation::mission_fixture(29, Execution::default()).unwrap();
        for person in sim.state.people.clone() {
            sim.debug_set_personality(
                person.id,
                decisions::Personality::restful(),
                decisions::Dream::Comfort,
            )
            .unwrap();
        }
        sim
    }
    fn propose(
        sim: &mut Simulation,
        leader: PersonId,
        objective: MissionObjective,
        supplies: Vec<MissionSupply>,
    ) -> MissionId {
        let home = sim.state.areas[0].id;
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::new(),
                    objective,
                    home,
                    supplies,
                    compensation: BTreeMap::new(),
                    deadline: SimTime(sim.state.time.0 + 720_000),
                },
            },
        )
        .unwrap();
        *sim.state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .next_back()
            .unwrap()
    }
    fn accept(sim: &mut Simulation, id: MissionId, person: PersonId) {
        let recruitment =
            sim.state.missions.as_ref().unwrap().missions[&id].members[&person].recruitment;
        sim.mission_command(person, &MissionAction::Accept { recruitment })
            .unwrap();
    }
    fn public_food(sim: &Simulation) -> InventoryId {
        sim.state
            .inventories
            .values()
            .find(|i| i.good == Good::Food && matches!(i.location, InventoryLocation::Area(_)))
            .unwrap()
            .id
    }
    #[test]
    fn supplies_and_route_legs_are_real_and_the_realm_keeps_working() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let worker = sim.state.people[6].id;
        sim.debug_set_personality(
            leader,
            decisions::Personality::industrious(),
            decisions::Dream::Independence,
        )
        .unwrap();
        sim.debug_set_personality(
            worker,
            decisions::Personality::industrious(),
            decisions::Dream::Independence,
        )
        .unwrap();
        let forest = sim.state.areas[2].id;
        let food = public_food(&sim);
        let carried = sim.state.people[sim.person_index[&leader]].inventory;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![MissionSupply {
                carrier: leader,
                source: food,
                quantity: Quantity(2_000),
            }],
        );
        accept(&mut sim, id, leader);
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(0));
        sim.advance(MINUTE).unwrap();
        assert!(matches!(
            sim.state.physical.as_ref().unwrap().actors[&leader].activity,
            Activity::Load { .. }
        ));
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(0));
        sim.advance(MINUTE).unwrap();
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(2_000));
        assert!(matches!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::InTransit { .. }
        ));
        sim.advance(SimDuration(600_000)).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Completed
        );
        assert!(
            sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Produced {actor,..}if actor==worker))
        );
        assert!(
            sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Departed {actor,..}if actor==leader))
        );
        assert!(sim.state.events.iter().any(
            |e| matches!(e.kind,EventKind::Arrived {actor,area}if actor==leader&&area==forest)
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn npc_refuses_danger_and_counteroffers_for_a_marginal_job() {
        let mut sim = fixture();
        let recruit = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        {
            let traits = &mut sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&recruit)
                .unwrap()
                .personality;
            traits.ambition = 0;
            traits.sociability = 400;
        }
        let id = propose(
            &mut sim,
            recruit,
            MissionObjective::Defeat { person: enemy },
            vec![],
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Refused
        );
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            recruit,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        sim.advance(MINUTE).unwrap();
        let offer = sim.state.missions.as_ref().unwrap().recruitments
            [&sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].recruitment]
            .clone();
        assert!(offer.countered);
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Offered
        );
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::AcceptCounteroffer {
                recruitment: offer.id,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        let offer = &sim.state.missions.as_ref().unwrap().recruitments[&offer.id];
        assert!(offer.agreement.is_some());
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&recruit].status,
            CompanionStatus::Accepted
        );
        let obligations = &sim.state.agreements.as_ref().unwrap().obligations;
        assert!(
            obligations
                .values()
                .any(|obligation| obligation.performed == 0)
        );
    }
    #[test]
    fn absent_sponsor_receives_no_telepathic_progress_and_reports_take_time() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        sim.debug_set_personality(
            leader,
            decisions::Personality::industrious(),
            decisions::Dream::Independence,
        )
        .unwrap();
        let forest = sim.state.areas[2].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.advance(SimDuration(240_000)).unwrap();
        let ruler = sim.state.ruler;
        assert_eq!(
            sim.state.missions.as_ref().unwrap().known[&ruler][&id].status,
            MissionStatus::Traveling
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Returning
        );
        let snapshot = sim.mission_view(leader)[0].clone();
        assert!(
            snapshot.observed_at
                > sim.state.missions.as_ref().unwrap().known[&ruler][&id].observed_at
        );
        assert!(
            sim.mission_command(
                leader,
                &MissionAction::Report {
                    mission: id,
                    recipient: ruler
                }
            )
            .is_err()
        );
        sim.advance(SimDuration(420_000)).unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        // A cached account can be reported to a bystander who never joined the party.
        let recipient = sim.state.people[8].id;
        assert!(sim.mission_view(recipient).is_empty());
        sim.mission_command(
            leader,
            &MissionAction::Report {
                mission: id,
                recipient,
            },
        )
        .unwrap();
        assert!(sim.mission_view(recipient).is_empty());
        let mut saved = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        saved.advance(MINUTE).unwrap();
        assert_eq!(saved.mission_view(recipient)[0].reported_by, Some(leader));
        assert_eq!(
            saved.mission_view(recipient)[0].observed_at,
            sim.mission_view(leader)[0].observed_at
        );
    }
    #[test]
    fn missing_personal_route_blocks_leader_without_moving_the_party() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let forest = sim
            .debug_add_disconnected_area("Unseen far clearing")
            .unwrap();
        let intermediary = sim.state.areas[2].id;
        sim.state.areas[2].links.push(Link {
            destination: forest,
            duration: MINUTE,
        });
        sim.state.areas.last_mut().unwrap().links.push(Link {
            destination: intermediary,
            duration: MINUTE,
        });
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
            .unwrap()
            .routes
            .clear();
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.advance(SimDuration(180_000)).unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        assert!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader]
                .explanation
                .contains("personally known")
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind,EventKind::Departed {actor,..}if actor==leader))
        );
    }
    #[test]
    fn delivery_credit_requires_the_conserved_origin_cargo() {
        let mut sim = fixture();
        let actor = sim.state.ruler;
        let area = sim.state.areas[0].id;
        let source = sim
            .state
            .inventories
            .values()
            .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Area(area))
            .unwrap()
            .id;
        let destination = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(area),
                Quantity(0),
                Quantity(10_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let carried = sim
            .state
            .inventories
            .values()
            .find(|i| i.good == Good::Timber && i.location == InventoryLocation::Carried(actor))
            .unwrap()
            .id;
        let id = propose(
            &mut sim,
            actor,
            MissionObjective::Deliver {
                source,
                destination,
                quantity: Quantity(1_000),
            },
            vec![],
        );
        sim.mission_command(actor, &MissionAction::Depart { mission: id })
            .unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            0
        );
        sim.debug_physical_command(
            actor,
            Command::Load {
                source,
                destination: carried,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            0
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].cargo[&carried],
            1_000
        );
        sim.debug_physical_command(
            actor,
            Command::Load {
                source: carried,
                destination,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].delivered,
            1_000
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].status,
            MissionStatus::Completed
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn changed_objectives_take_local_time_and_may_be_refused() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        let old = sim.state.missions.as_ref().unwrap().missions[&id].members[&leader]
            .directive
            .clone();
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::Objective {
                mission: id,
                objective: MissionObjective::Defeat { person: enemy },
            },
        )
        .unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].directive,
            old
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].status,
            CompanionStatus::Withdrawn
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|e| matches!(e.kind, EventKind::EncounterStarted { .. }))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn known_enemy_relationship_can_motivate_an_actual_defection() {
        let mut sim = fixture();
        let actor = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let leader = sim.state.people[7].id;
        let home = sim.state.areas[0].id;
        sim.mission_command(
            sim.state.ruler,
            &MissionAction::Propose {
                proposal: MissionProposal {
                    leader,
                    recruits: BTreeSet::from([actor]),
                    objective: MissionObjective::Defeat { person: enemy },
                    home,
                    supplies: vec![],
                    compensation: BTreeMap::new(),
                    deadline: SimTime(720_000),
                },
            },
        )
        .unwrap();
        let id = *sim
            .state
            .missions
            .as_ref()
            .unwrap()
            .missions
            .keys()
            .next_back()
            .unwrap();
        accept(&mut sim, id, leader);
        accept(&mut sim, id, actor);
        let mind = sim
            .state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&actor)
            .unwrap();
        mind.personality.honesty = 100;
        mind.relationships.insert(
            leader,
            decisions::Relationship {
                trust: -900,
                affection: 0,
            },
        );
        mind.relationships.insert(
            enemy,
            decisions::Relationship {
                trust: 900,
                affection: 0,
            },
        );
        let battle = sim
            .debug_start_encounter(vec![(sim.state.ruler, 0), (actor, 0), (enemy, 1)])
            .unwrap();
        // The root combat loop observes fighters before making each decision.
        let point =
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&enemy].position;
        sim.state
            .combat
            .as_mut()
            .unwrap()
            .encounters
            .get_mut(&battle)
            .unwrap()
            .fighters
            .get_mut(&actor)
            .unwrap()
            .known
            .insert(enemy, point);
        sim.review_mission_combat(battle, actor);
        assert_eq!(
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&actor].team,
            1
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&actor].status,
            CompanionStatus::Defected
        );
        assert_eq!(
            sim.state.people[sim.person_index[&actor]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
    }
    #[test]
    fn worker_order_saves_slices_and_passive_queries_preserve_mission_world() {
        let mut a = fixture();
        let leader = a.state.people[5].id;
        let farm = a.state.areas[1].id;
        let id = propose(
            &mut a,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut a, id, leader);
        let mut b = Simulation::load(
            &a.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        a.advance(SimDuration(540_000)).unwrap();
        for duration in [17_000, 43_000, 93_001, 11_999, 151_007, 223_993] {
            b.observe();
            b.mission_view(leader);
            b.advance(SimDuration(duration)).unwrap();
        }
        assert_eq!(a.state, b.state);
    }
    #[test]
    fn ruler_order_spends_own_action_and_companion_takes_its_own_turn() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let ruler = sim.state.ruler;
        let farm = sim.state.areas[1].id;
        {
            let traits = &mut sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&leader)
                .unwrap()
                .personality;
            traits.ambition = 900;
            traits.sociability = 900;
            traits.risk_tolerance = 900;
            traits.empathy = 100;
        }
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (leader, 0), (enemy, 1)])
            .unwrap();
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter: battle,
                movement: None,
                action: CombatAction::MissionObjective {
                    mission: id,
                    objective: MissionObjective::Defeat { person: enemy },
                },
            },
        })
        .unwrap();
        assert!(
            !sim.state
                .events
                .iter()
                .any(|event| matches!(event.kind,EventKind::Struck {actor,..} if actor==ruler))
        );
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::Struck {actor,target,..} if actor==leader && target==enemy)));
        assert!(sim.state.combat.as_ref().unwrap().encounters[&battle].cursor > 1);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn surrender_and_capture_stop_real_companion_travel() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let ruler = sim.state.ruler;
        let forest = sim.state.areas[2].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_command(ruler, &MissionAction::Depart { mission: id })
            .unwrap();
        let body = sim
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&leader)
            .unwrap();
        body.vitals.health = 70_000;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
            .unwrap()
            .personality
            .risk_tolerance = 10;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&leader)
            .unwrap()
            .routes
            .clear();
        sim.state
            .combat
            .as_mut()
            .unwrap()
            .profiles
            .get_mut(&leader)
            .unwrap()
            .initiative = 300;
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (leader, 1)])
            .unwrap();
        assert!(
            sim.state.combat.as_ref().unwrap().encounters[&battle].fighters[&leader].surrendered
        );
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter: battle,
                movement: None,
                action: CombatAction::Capture { target: leader },
            },
        })
        .unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        assert!(
            sim.state
                .combat
                .as_ref()
                .unwrap()
                .captivity
                .contains_key(&leader)
        );
        assert_eq!(
            sim.state.people[sim.person_index[&leader]].location,
            Location::AtArea(sim.state.areas[0].id)
        );
        assert_eq!(
            sim.state.missions.as_ref().unwrap().missions[&id].members[&leader].status,
            CompanionStatus::Captured
        );
        assert!(
            !sim.state
                .events
                .iter()
                .any(|event| matches!(event.kind,EventKind::Departed {actor,..} if actor==leader))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
    #[test]
    fn unready_guardian_cannot_accept_service_by_bypassing_its_assessment() {
        let mut sim = Simulation::lineage_fixture(29, Execution::default()).unwrap();
        let prototype = fixture();
        sim.state.missions = prototype.state.missions;
        sim.state.agreements = Some(agreements::World {
            agreements: BTreeMap::new(),
            obligations: BTreeMap::new(),
            amendments: BTreeMap::new(),
            custodians: sim
                .state
                .inventories
                .values()
                .map(|i| {
                    (
                        i.id,
                        match i.location {
                            InventoryLocation::Carried(p) => p,
                            InventoryLocation::Area(_) => sim.state.ruler,
                        },
                    )
                })
                .collect(),
            last_event: sim.state.next_event - 1,
            next_search: BTreeMap::new(),
            pending_work: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            last_observation_event: sim.state.next_event - 1,
        });
        let guardian = sim.state.people[1].id;
        let forest = sim.state.areas[2].id;
        let id = propose(
            &mut sim,
            guardian,
            MissionObjective::Visit {
                destination: forest,
            },
            vec![],
        );
        let recruitment =
            sim.state.missions.as_ref().unwrap().missions[&id].members[&guardian].recruitment;
        let before = sim.state.clone();
        assert!(
            sim.mission_command(guardian, &MissionAction::Accept { recruitment })
                .is_err()
        );
        assert_eq!(sim.state, before);
        assert!(!sim.mission_actor_busy(guardian));
    }
    #[test]
    fn local_views_show_declared_choices_without_private_reasoning() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.mission_explain(id, leader, "My private trust, risk, and route judgment");
        sim.observe_own_mission(leader, id);
        sim.publish_mission(id);
        assert!(
            sim.mission_view(leader)[0].companions[0]
                .explanation
                .contains("private trust")
        );
        assert!(
            !sim.mission_view(sim.state.ruler)[0].companions[0]
                .explanation
                .contains("private trust")
        );
        assert!(
            sim.mission_view(sim.state.ruler)[0].companions[0]
                .explanation
                .contains("Accepted")
        );
    }
    #[test]
    fn invalid_remote_tactical_order_preserves_turn_and_body() {
        let mut sim = fixture();
        let leader = sim.state.people[5].id;
        let enemy = sim.state.people[6].id;
        let ruler = sim.state.ruler;
        let farm = sim.state.areas[1].id;
        let id = propose(
            &mut sim,
            leader,
            MissionObjective::Visit { destination: farm },
            vec![],
        );
        accept(&mut sim, id, leader);
        sim.debug_move_actor(leader, farm).unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        let battle = sim
            .debug_start_encounter(vec![(ruler, 0), (enemy, 1)])
            .unwrap();
        let before = sim.state.clone();
        assert!(
            sim.submit(Submission {
                caller_sequence: sim.next_caller_sequence().unwrap(),
                command: Command::CombatTurn {
                    encounter: battle,
                    movement: Some(Point { x: 5_000, y: 8_000 }),
                    action: CombatAction::MissionObjective {
                        mission: id,
                        objective: MissionObjective::Defeat { person: enemy }
                    }
                }
            })
            .is_err()
        );
        assert_eq!(sim.state, before);
        assert!(
            sim.mission_command(ruler, &MissionAction::Leave { mission: id })
                .is_err()
        );
        assert_eq!(sim.state, before);
    }
}

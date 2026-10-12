use crate::*;
use crate::{mission as stored_mission, recruitment as stored_recruitment};
use game_agreements::{AgreementTerm, PermissionAsset, TermsContext};
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};

/// Only referenced asset identities and kinds, never remote balances/locations.
/// The coordinator obtains custody acknowledgments from the asset's owner.
#[derive(Clone, Debug, Default)]
pub struct MissionAssets {
    pub areas: BTreeSet<AreaId>,
    pub people: BTreeSet<PersonId>,
    pub terms: TermsContext,
    pub carrying_capacity: BTreeMap<PersonId, Quantity>,
}
impl MissionAssets {
    pub fn objective_valid(&self, objective: &MissionObjective, now: SimTime) -> bool {
        match objective {
            MissionObjective::Visit { destination } => self.areas.contains(destination),
            MissionObjective::Deliver {
                source,
                destination,
                quantity,
            } => {
                quantity.0 > 0
                    && source != destination
                    && self
                        .terms
                        .inventories
                        .get(source)
                        .zip(self.terms.inventories.get(destination))
                        .is_some_and(|(a, b)| a.good == b.good && quantity.0 <= b.capacity.0)
            }
            MissionObjective::Rescue { person, return_to } => {
                self.people.contains(person) && self.areas.contains(return_to)
            }
            MissionObjective::Defeat { person } => self.people.contains(person),
            MissionObjective::Confine { person, until } => {
                self.people.contains(person) && *until > now
            }
            MissionObjective::Patrol { areas, until } => {
                !areas.is_empty()
                    && areas.len() <= 16
                    && areas.is_subset(&self.areas)
                    && *until > now
            }
        }
    }
    pub fn compensation_valid(
        &self,
        sponsor: PersonId,
        recruit: PersonId,
        terms: &[AgreementTerm],
        now: SimTime,
    ) -> bool {
        terms.len() <= 8
            && terms.iter().all(|term| {
                let [a, b] = term.parties();
                a != b
                    && [a, b].iter().all(|p| *p == sponsor || *p == recruit)
                    && term.amount() > 0
                    && term.deadline().is_none_or(|at| at > now)
                    && match term {
                        AgreementTerm::Delivery {
                            source,
                            destination,
                            quantity,
                            ..
                        } => {
                            source != destination
                                && self
                                    .terms
                                    .inventories
                                    .get(source)
                                    .zip(self.terms.inventories.get(destination))
                                    .is_some_and(|(a, b)| {
                                        a.good == b.good && quantity.0 <= b.capacity.0
                                    })
                        }
                        AgreementTerm::Access {
                            grantor, inventory, ..
                        } => self
                            .terms
                            .grants
                            .contains(&(PermissionAsset::Inventory(*inventory), *grantor)),
                        _ => false,
                    }
            })
    }
}
/// Current body/contact facts are supplied by their owners. Service authority,
/// previous acceptance and competing known commitments are decided here.
#[derive(Clone, Debug)]
pub struct CommandFacts {
    pub now: SimTime,
    pub area: AreaId,
    /// Independent, capable people actually present and able to converse.
    pub available: BTreeSet<PersonId>,
    /// Actual guardian/urgent-care commitments, not a willingness verdict.
    pub duties: BTreeSet<PersonId>,
    /// Observed local contacts at proposal time, deterministically bounded.
    pub contacts: BTreeMap<PersonId, AreaId>,
    pub assets: MissionAssets,
}
#[derive(Clone, Debug, Default)]
pub struct MissionIds {
    pub mission: Option<MissionId>,
    pub recruitments: BTreeMap<PersonId, RecruitmentId>,
    /// Reserved by the agreement owner; the accompanying consent effect must
    /// be prepared and committed with this mission transition, never invented.
    pub backing: Option<AgreementId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceConsent {
    pub recruitment: RecruitmentId,
    pub sponsor: PersonId,
    pub recruit: PersonId,
    pub terms: Vec<AgreementTerm>,
    pub agreement: AgreementId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MissionEvent {
    Reported {
        mission: MissionId,
        speaker: PersonId,
        recipient: PersonId,
    },
    Proposed {
        mission: MissionId,
        sponsor: PersonId,
    },
    RecruitmentChanged {
        mission: MissionId,
        person: PersonId,
        status: CompanionStatus,
    },
    Departed {
        mission: MissionId,
    },
    ObjectiveChanged {
        mission: MissionId,
        leader: PersonId,
    },
    Finished {
        mission: MissionId,
        status: MissionStatus,
    },
}
#[derive(Clone, Debug)]
struct MissionPatch {
    before: Option<Mission>,
    after: Mission,
}
#[derive(Clone, Debug)]
struct RecruitmentPatch {
    before: Option<Recruitment>,
    after: Recruitment,
}
#[derive(Clone, Debug, Default)]
pub struct PreparedMission {
    missions: Vec<MissionPatch>,
    recruitments: Vec<RecruitmentPatch>,
    memberships: Vec<(PersonId, Option<MissionId>)>,
    pub(crate) receipt_cursor: Option<(u64, u64)>,
    pub consent: Option<ServiceConsent>,
    pub conversation: Option<(PersonId, Conversation)>,
    pub events: Vec<MissionEvent>,
    pub notices: BTreeSet<MissionId>,
    pub trust: Vec<TrustEffect>,
    pub allegiance: Option<AllegianceEffect>,
}
impl PreparedMission {
    pub fn created_missions(&self) -> impl Iterator<Item = MissionId> {
        self.missions
            .iter()
            .filter(|patch| patch.before.is_none())
            .map(|patch| patch.after.id)
    }
    pub fn check(&self, state: &Runtime, store: &Store) -> Result<(), Error> {
        let fresh = self
            .missions
            .iter()
            .filter(|p| p.before.is_none())
            .map(|p| p.after.id.0)
            .chain(
                self.recruitments
                    .iter()
                    .filter(|p| p.before.is_none())
                    .map(|p| p.after.id.0),
            );
        if self
            .receipt_cursor
            .is_some_and(|(before, _)| state.last_event != before)
            || fresh.into_iter().any(|id| mission_id_exists(store, id))
            || self
                .missions
                .iter()
                .any(|p| stored_mission(store, p.after.id) != p.before.as_ref())
            || self
                .recruitments
                .iter()
                .any(|p| stored_recruitment(store, p.after.id) != p.before.as_ref())
            || self
                .memberships
                .iter()
                .any(|(person, old)| state.membership(store, *person) != *old)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// The coordinator preflights and commits the exact compensation effect in
    /// the same transaction. This changes service records only, never stocks.
    pub fn stage_into(
        self,
        state: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<MissionFinalization, Error> {
        self.check(state, store)?;
        if self
            .missions
            .iter()
            .any(|p| batch.touches::<Mission>(StableId(p.after.id.0)))
            || self
                .recruitments
                .iter()
                .any(|p| batch.touches::<Recruitment>(StableId(p.after.id.0)))
        {
            return Err(Error::Unavailable);
        }
        for patch in self.missions {
            patch.after.validate()?;
            batch.insert(StableId(patch.after.id.0), patch.after);
        }
        for patch in self.recruitments {
            patch.after.validate()?;
            batch.insert(StableId(patch.after.id.0), patch.after);
        }
        Ok(MissionFinalization {
            receipt_cursor: self.receipt_cursor.map(|(_, after)| after),
        })
    }
    pub fn commit(self, state: &mut Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(state, store, &mut batch)?;
        store.apply_batch(batch)?;
        finish.finish(state);
        Ok(())
    }
    pub(crate) fn guard_membership(&mut self, state: &Runtime, store: &Store, actor: PersonId) {
        self.memberships
            .push((actor, state.membership(store, actor)));
    }
    pub(crate) fn mission(&mut self, store: &Store, after: Mission) {
        self.notices.insert(after.id);
        self.missions.push(MissionPatch {
            before: stored_mission(store, after.id).cloned(),
            after,
        });
    }
    pub(crate) fn recruitment(&mut self, store: &Store, after: Recruitment) {
        self.recruitments.push(RecruitmentPatch {
            before: stored_recruitment(store, after.id).cloned(),
            after,
        });
    }
}
impl Runtime {
    /// A distant outcome releases a remembered commitment only after an actual
    /// observation/report. Authoritative status is not private knowledge.
    pub fn membership(&self, store: &Store, actor: PersonId) -> Option<MissionId> {
        missions(store)
            .map(|(_, row)| row)
            .find(|m| {
                !knowledge(store, actor)
                    .map(|row| &row.missions)
                    .and_then(|k| k.get(&m.id))
                    .is_some_and(|view| terminal(view.status))
                    && m.members
                        .get(&actor)
                        .is_some_and(|p| p.status == CompanionStatus::Accepted)
            })
            .map(|m| m.id)
    }
    pub fn prepare_command(
        &self,
        store: &Store,
        actor: PersonId,
        action: &MissionAction,
        f: &CommandFacts,
        ids: &MissionIds,
    ) -> Result<PreparedMission, Error> {
        if !f.available.contains(&actor) {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedMission::default();
        match action {
            MissionAction::Propose { proposal } => {
                let people: BTreeSet<_> = proposal
                    .recruits
                    .iter()
                    .copied()
                    .chain([proposal.leader])
                    .collect();
                if !(1..=32).contains(&people.len())
                    || proposal.deadline <= f.now
                    || proposal.home != f.area
                    || !people.is_subset(&f.available)
                    || people.iter().any(|p| self.membership(store, *p).is_some())
                    || !f.assets.objective_valid(&proposal.objective, f.now)
                    || matches!(&proposal.objective,MissionObjective::Defeat {person}|MissionObjective::Confine {person,..} if people.contains(person)||*person==actor)
                    || matches!(proposal.objective,MissionObjective::Confine {until,..} if until>proposal.deadline)
                    || proposal.supplies.len() > 32
                    || proposal.compensation.iter().any(|(p, terms)| {
                        !people.contains(p) || !f.assets.compensation_valid(actor, *p, terms, f.now)
                    })
                    || proposal.supplies.iter().any(|s| {
                        !people.contains(&s.carrier)
                            || s.quantity.0 == 0
                            || f.assets
                                .carrying_capacity
                                .get(&s.carrier)
                                .is_none_or(|cap| s.quantity.0 > cap.0)
                            || !f.assets.terms.inventories.contains_key(&s.source)
                    })
                {
                    return Err(Error::Unavailable);
                }
                let id = ids.mission.ok_or(Error::Unavailable)?;
                let unique: BTreeSet<_> = ids
                    .recruitments
                    .values()
                    .map(|id| id.0)
                    .chain([id.0])
                    .collect();
                if id.0 == 0
                    || mission_id_exists(store, id.0)
                    || ids.recruitments.keys().copied().collect::<BTreeSet<_>>() != people
                    || unique.len() != people.len() + 1
                    || unique.contains(&0)
                    || ids
                        .recruitments
                        .values()
                        .any(|id| mission_id_exists(store, id.0))
                    || ids.backing.is_some()
                {
                    return Err(Error::Unavailable);
                }
                let mut members = BTreeMap::new();
                for person in people {
                    plan.memberships
                        .push((person, self.membership(store, person)));
                    let recruitment = ids.recruitments[&person];
                    plan.recruitment(
                        store,
                        Recruitment {
                            id: recruitment,
                            mission: id,
                            person,
                            compensation: proposal
                                .compensation
                                .get(&person)
                                .cloned()
                                .unwrap_or_default(),
                            agreement: None,
                            countered: false,
                        },
                    );
                    members.insert(
                        person,
                        Companion {
                            confinement: None,
                            person,
                            recruitment,
                            status: if person == actor {
                                CompanionStatus::Accepted
                            } else {
                                CompanionStatus::Offered
                            },
                            directive: proposal.objective.clone(),
                            received_at: f.now,
                            visited: BTreeSet::from([f.area]),
                            delivered: 0,
                            cargo: BTreeMap::new(),
                            supply_cargo: BTreeMap::new(),
                            contacts: f.contacts.iter().take(64).map(|(p, a)| (*p, *a)).collect(),
                            last_leg_from: None,
                            declared_explanation: "Received an expedition proposal".into(),
                            explanation: "Considering the proposed expedition and compensation"
                                .into(),
                        },
                    );
                }
                plan.mission(
                    store,
                    Mission {
                        id,
                        sponsor: actor,
                        proposal: proposal.clone(),
                        status: MissionStatus::Preparing,
                        members,
                        created_at: f.now,
                        departed_at: None,
                        explanation: "Preparing volunteers and real provisions".into(),
                        succeeded: false,
                    },
                );
                plan.events.push(MissionEvent::Proposed {
                    mission: id,
                    sponsor: actor,
                });
            }
            MissionAction::Accept { recruitment }
            | MissionAction::Refuse { recruitment }
            | MissionAction::AcceptCounteroffer { recruitment }
            | MissionAction::Counteroffer { recruitment, .. } => {
                let offer = stored_recruitment(store, *recruitment).ok_or(Error::Unavailable)?;
                let old = stored_mission(store, offer.mission).ok_or(Error::Unavailable)?;
                let mut m = old.clone();
                let mut r = offer.clone();
                let member = m.members.get_mut(&offer.person).ok_or(Error::Unavailable)?;
                match action {
                    MissionAction::Accept { .. } => {
                        if offer.person != actor
                            || f.duties.contains(&actor)
                            || self.membership(store, actor).is_some_and(|id| id != m.id)
                            || offer.countered
                            || m.status != MissionStatus::Preparing
                            || member.status != CompanionStatus::Offered
                            || !f.available.contains(&m.sponsor)
                            || !f.assets.compensation_valid(
                                m.sponsor,
                                actor,
                                &offer.compensation,
                                f.now,
                            )
                        {
                            return Err(Error::Unavailable);
                        }
                        plan.memberships
                            .push((actor, self.membership(store, actor)));
                        if !offer.compensation.is_empty() {
                            let agreement = ids
                                .backing
                                .filter(|id| id.0 != 0)
                                .ok_or(Error::Unavailable)?;
                            r.agreement = Some(agreement);
                            plan.consent = Some(ServiceConsent {
                                recruitment: *recruitment,
                                sponsor: m.sponsor,
                                recruit: actor,
                                terms: offer.compensation.clone(),
                                agreement,
                            });
                        } else if ids.backing.is_some() {
                            return Err(Error::Unavailable);
                        }
                        member.status = CompanionStatus::Accepted;
                        member.declared_explanation =
                            "Accepted the expedition objective and compensation".into();
                        member.explanation = "Accepted the objective and compensation".into();
                        plan.events.push(MissionEvent::RecruitmentChanged {
                            mission: m.id,
                            person: actor,
                            status: CompanionStatus::Accepted,
                        });
                    }
                    MissionAction::Refuse { .. } => {
                        if offer.person != actor
                            || member.status != CompanionStatus::Offered
                            || !f.available.contains(&m.sponsor)
                        {
                            return Err(Error::Unavailable);
                        }
                        member.status = CompanionStatus::Refused;
                        member.declared_explanation = "Declined the proposed service".into();
                        member.explanation = member.declared_explanation.clone();
                        plan.events.push(MissionEvent::RecruitmentChanged {
                            mission: m.id,
                            person: actor,
                            status: CompanionStatus::Refused,
                        });
                    }
                    MissionAction::AcceptCounteroffer { .. } => {
                        if m.sponsor != actor
                            || !offer.countered
                            || !f.available.contains(&offer.person)
                        {
                            return Err(Error::Unavailable);
                        }
                        r.countered = false;
                    }
                    MissionAction::Counteroffer { compensation, .. } => {
                        if offer.person != actor
                            || member.status != CompanionStatus::Offered
                            || !f.available.contains(&m.sponsor)
                            || !f
                                .assets
                                .compensation_valid(m.sponsor, actor, compensation, f.now)
                            || *compensation == offer.compensation
                        {
                            return Err(Error::Unavailable);
                        }
                        r.compensation = compensation.clone();
                        r.countered = true;
                        member.explanation="Asked for different compensation; the sponsor must accept the counteroffer".into();
                    }
                    _ => unreachable!(),
                }
                plan.recruitment(store, r);
                plan.mission(store, m);
            }
            MissionAction::Depart { mission } => {
                let old = stored_mission(store, *mission).ok_or(Error::Unavailable)?;
                if actor != old.sponsor && actor != old.proposal.leader
                    || old.status != MissionStatus::Preparing
                    || old
                        .members
                        .get(&old.proposal.leader)
                        .is_none_or(|p| p.status != CompanionStatus::Accepted)
                    || !f.available.contains(&old.proposal.leader)
                    || old.proposal.home != f.area
                {
                    return Err(Error::Unavailable);
                }
                let mut m = old.clone();
                m.status = MissionStatus::Traveling;
                m.departed_at = Some(f.now);
                m.explanation = "Gathering supplies and setting out".into();
                plan.mission(store, m);
                plan.events
                    .push(MissionEvent::Departed { mission: *mission });
            }
            MissionAction::Leave { mission } => {
                let mut m = stored_mission(store, *mission)
                    .ok_or(Error::Unavailable)?
                    .clone();
                let member = m
                    .members
                    .get_mut(&actor)
                    .filter(|p| p.status == CompanionStatus::Accepted)
                    .ok_or(Error::Unavailable)?;
                member.status = CompanionStatus::Withdrawn;
                member.declared_explanation = "Withdrew service".into();
                member.explanation = "Withdrew service; existing payment obligations remain".into();
                plan.mission(store, m);
                plan.events.push(MissionEvent::RecruitmentChanged {
                    mission: *mission,
                    person: actor,
                    status: CompanionStatus::Withdrawn,
                });
            }
            MissionAction::Objective { mission, .. } | MissionAction::Retreat { mission } => {
                let old = stored_mission(store, *mission).ok_or(Error::Unavailable)?;
                let retreat = matches!(action, MissionAction::Retreat { .. });
                if actor != old.sponsor && actor != old.proposal.leader
                    || !f.available.contains(&old.proposal.leader)
                    || !matches!(
                        old.status,
                        MissionStatus::Traveling | MissionStatus::Operating
                    ) && (retreat || old.status != MissionStatus::Preparing)
                {
                    return Err(Error::Unavailable);
                }
                let conversation = if let MissionAction::Objective { objective, .. } = action {
                    if !f.assets.objective_valid(objective, f.now) {
                        return Err(Error::Unavailable);
                    }
                    Conversation::Objective {
                        mission: *mission,
                        leader: old.proposal.leader,
                        objective: objective.clone(),
                    }
                } else {
                    Conversation::Retreat {
                        mission: *mission,
                        leader: old.proposal.leader,
                    }
                };
                plan.missions.push(MissionPatch {
                    before: Some(old.clone()),
                    after: old.clone(),
                });
                plan.conversation = Some((old.proposal.leader, conversation));
            }
            MissionAction::Report { mission, recipient } => {
                if *recipient == actor || !f.available.contains(recipient) {
                    return Err(Error::Unavailable);
                }
                let snapshot = knowledge(store, actor)
                    .map(|row| &row.missions)
                    .and_then(|k| k.get(mission))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                plan.conversation = Some((
                    *recipient,
                    Conversation::Report {
                        recipient: *recipient,
                        snapshot: Box::new(snapshot),
                    },
                ));
            }
        }
        if !matches!(action, MissionAction::Propose { .. })
            && (ids.mission.is_some() || !ids.recruitments.is_empty())
        {
            return Err(Error::Unavailable);
        }
        if !matches!(action, MissionAction::Accept { .. }) && ids.backing.is_some() {
            return Err(Error::Unavailable);
        }
        Ok(plan)
    }
}
fn terminal(status: MissionStatus) -> bool {
    matches!(
        status,
        MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
    )
}

/// A declaration is the player's actual choice; autonomous people provide their
/// own needs/personality/remembered trust. The kernel computes willingness.
#[derive(Clone, Copy, Debug)]
pub enum ServiceChoice {
    DeclaredAcceptance,
    Personal(RecruitmentInput),
}
#[derive(Clone, Debug)]
pub struct OrderFacts {
    pub now: SimTime,
    /// Only accepted people who actually heard this local objective.
    pub heard: BTreeMap<PersonId, ServiceChoice>,
    pub assets: MissionAssets,
}
impl Runtime {
    pub fn prepare_order(
        &self,
        store: &Store,
        id: MissionId,
        leader: PersonId,
        objective: &MissionObjective,
        f: &OrderFacts,
        retreat: bool,
    ) -> Result<PreparedMission, Error> {
        let old = stored_mission(store, id).ok_or(Error::Unavailable)?;
        if old.proposal.leader != leader
            || terminal(old.status)
            || old
                .members
                .get(&leader)
                .is_none_or(|m| m.status != CompanionStatus::Accepted)
            || !f.heard.contains_key(&leader)
            || !f.assets.objective_valid(objective, f.now)
            || retreat
                && !matches!(
                    old.status,
                    MissionStatus::Traveling | MissionStatus::Operating
                )
        {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedMission::default();
        let mut m = old.clone();
        m.proposal.objective = objective.clone();
        for (actor, choice) in &f.heard {
            let member = m
                .members
                .get_mut(actor)
                .filter(|p| p.status == CompanionStatus::Accepted)
                .ok_or(Error::Unavailable)?;
            let offer = stored_recruitment(store, member.recruitment).ok_or(Error::Unavailable)?;
            if offer.person != *actor || offer.mission != id {
                return Err(Error::Unavailable);
            }
            // Guard the original compensation used for the actual judgment.
            plan.recruitment(store, offer.clone());
            let willing = match choice {
                ServiceChoice::DeclaredAcceptance => true,
                ServiceChoice::Personal(input) => {
                    if input.actor != *actor {
                        return Err(Error::Unavailable);
                    }
                    recruitment_score(
                        RecruitmentInput {
                            violent: objective.requires_force(),
                            ..*input
                        },
                        &offer.compensation,
                    ) >= 0
                }
            };
            member.received_at = f.now;
            if willing {
                member.directive = objective.clone();
                member.confinement = None;
                member.declared_explanation = "Accepted a locally communicated objective".into();
                member.explanation =
                    "Received and accepted the locally communicated objective".into();
                member.delivered = 0;
                member.cargo.clear();
            } else {
                member.status = CompanionStatus::Withdrawn;
                member.declared_explanation = "Declined the changed objective".into();
                member.explanation =
                    "Declined a changed objective that conflicts with personal needs or values"
                        .into();
                plan.events.push(MissionEvent::RecruitmentChanged {
                    mission: id,
                    person: *actor,
                    status: CompanionStatus::Withdrawn,
                });
            }
        }
        if retreat {
            m.status = MissionStatus::Returning;
            m.succeeded = false;
            m.explanation =
                "Leader communicated a retreat; participants decide whether to return".into();
        }
        plan.mission(store, m);
        plan.events.push(MissionEvent::ObjectiveChanged {
            mission: id,
            leader,
        });
        Ok(plan)
    }
    pub fn recruitment_response(
        &self,
        store: &Store,
        id: RecruitmentId,
        input: RecruitmentInput,
        area: AreaId,
        self_inventory: InventoryId,
        known_supplies: &[StockKnowledge],
    ) -> Option<MissionAction> {
        let offer = stored_recruitment(store, id)?;
        let mission = stored_mission(store, offer.mission)?;
        if offer.person != input.actor
            || mission.status != MissionStatus::Preparing
            || offer.countered
            || mission.members.get(&input.actor)?.status != CompanionStatus::Offered
        {
            return None;
        }
        let score = recruitment_score(
            RecruitmentInput {
                violent: mission.proposal.objective.requires_force(),
                ..input
            },
            &offer.compensation,
        );
        if score >= 0 {
            return Some(MissionAction::Accept { recruitment: id });
        }
        if (-500..0).contains(&score)
            && offer.compensation.is_empty()
            && let Some(stock) = known_supplies.iter().find(|stock| {
                stock.good == Good::Food
                    && stock.accessible
                    && stock.quantity.0 >= 1000
                    && stock.area == area
                    && stock.inventory != self_inventory
            })
        {
            return Some(MissionAction::Counteroffer {
                recruitment: id,
                compensation: vec![AgreementTerm::Delivery {
                    debtor: mission.sponsor,
                    creditor: input.actor,
                    source: stock.inventory,
                    destination: self_inventory,
                    quantity: Quantity(1000),
                    deadline: mission.proposal.deadline,
                }],
            });
        }
        Some(MissionAction::Refuse { recruitment: id })
    }
}

fn mission_id_exists(store: &Store, id: u64) -> bool {
    store.contains(StableId(id))
}
impl Runtime {
    /// A tactical or completed conversation authorizes the named leader's
    /// service objective, never other bodies' force actions or private stocks.
    pub fn validate_order(
        &self,
        store: &Store,
        actor: PersonId,
        id: MissionId,
        objective: &MissionObjective,
        f: &CommandFacts,
    ) -> Result<(), Error> {
        let m = stored_mission(store, id).ok_or(Error::Unavailable)?;
        if !f.available.contains(&actor)
            || actor != m.sponsor && actor != m.proposal.leader
            || !f.available.contains(&m.proposal.leader)
            || terminal(m.status)
            || m.members
                .get(&m.proposal.leader)
                .is_none_or(|p| p.status != CompanionStatus::Accepted)
            || !f.assets.objective_valid(objective, f.now)
            || matches!(objective,MissionObjective::Defeat {person}|MissionObjective::Confine {person,..} if m.members.contains_key(person)||*person==m.sponsor)
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

pub struct MissionFinalization {
    receipt_cursor: Option<u64>,
}
impl MissionFinalization {
    pub fn finish(self, state: &mut Runtime) {
        if let Some(after) = self.receipt_cursor {
            state.last_event = after;
        }
    }
}

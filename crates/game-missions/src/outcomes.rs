use crate::*;
use game_agreements::AgreementView;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Receipts come only after the owning physical operation committed. They
/// describe conserved movements and observed custody, never promised supplies.
#[derive(Clone, Debug)]
pub enum MissionReceipt {
    Captured {
        actor: PersonId,
        target: PersonId,
        at: SimTime,
    },
    Released {
        actor: PersonId,
        target: PersonId,
        at: SimTime,
    },
    Departed {
        actor: PersonId,
        from: AreaId,
    },
    Arrived {
        actor: PersonId,
        area: AreaId,
        observers: BTreeSet<PersonId>,
        contacts: BTreeSet<PersonId>,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        destination_carrier: Option<PersonId>,
    },
    Lost {
        source: InventoryId,
        quantity: Quantity,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberBody {
    pub alive: bool,
    pub captive: bool,
    /// An actually capable body currently at an area; transit/incapacity has no
    /// such area. This does not contain any other person's private knowledge.
    pub area: Option<AreaId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetObservation {
    pub person: PersonId,
    pub area: AreaId,
    pub alive: bool,
    pub health: u64,
    pub incapacitation: u64,
    pub captor: Option<PersonId>,
    pub restricted: bool,
    pub observed_at: SimTime,
}
/// Own remembered stock locations can inform intentions, but do not prove that
/// stock or target still exists there. Actual target observations are separate.
#[derive(Clone, Debug, Default)]
pub struct ObjectiveKnowledge {
    pub stocks: BTreeMap<InventoryId, StockKnowledge>,
    pub target: Option<TargetObservation>,
}
#[derive(Clone, Debug)]
pub struct OutcomeFacts {
    pub now: SimTime,
    pub bodies: BTreeMap<PersonId, MemberBody>,
    pub leader_knowledge: ObjectiveKnowledge,
    /// Actual local recipients of a completed outcome. No remote trust effect.
    pub receivers: BTreeSet<PersonId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustEffect {
    pub observer: PersonId,
    pub leader: PersonId,
    pub delta: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllegianceEffect {
    pub actor: PersonId,
    pub opponent: PersonId,
    pub team: u32,
}
fn terminal(s: MissionStatus) -> bool {
    matches!(
        s,
        MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
    )
}

impl World {
    pub fn prepare_receipt(
        &self,
        sequence: EventId,
        receipt: &MissionReceipt,
    ) -> Result<PreparedMission, Error> {
        if sequence.0 == 0 {
            return Err(Error::Unavailable);
        }
        if sequence.0 <= self.last_event {
            return Ok(PreparedMission::default());
        }
        let mut plan = PreparedMission::default();
        plan.receipt_cursor = Some((self.last_event, sequence.0));
        let mut changed: BTreeMap<MissionId, Mission> = BTreeMap::new();
        match receipt {
            MissionReceipt::Captured { actor, target, at } => {
                plan.guard_membership(self, *actor);
                if let Some(id) = self.membership(*actor) {
                    let old = self.missions.get(&id).ok_or(Error::Unavailable)?;
                    if let Some(member) = old.members.get(actor)
                        && matches!(member.directive,MissionObjective::Confine {person,..} if person==*target)
                    {
                        let mut m = old.clone();
                        m.members.get_mut(actor).unwrap().confinement =
                            Some(ConfinementObservation {
                                person: *target,
                                captured_at: *at,
                                released_at: None,
                            });
                        changed.insert(id, m);
                    }
                }
            }
            MissionReceipt::Released { actor, target, at } => {
                for old in self.missions.values().filter(|m| !terminal(m.status)) {
                    let relevant=old.members.values().any(|p|p.confinement.is_some() && matches!(p.directive,MissionObjective::Confine {person,..}if person==*target));
                    if !relevant {
                        continue;
                    }
                    let mut m = old.clone();
                    let party_released = m.members.contains_key(actor);
                    for member in m.members.values_mut() {
                        if let MissionObjective::Confine { person, until } = member.directive
                            && person == *target
                            && let Some(record) = &mut member.confinement
                        {
                            if party_released && *at >= until {
                                record.released_at = Some(*at);
                            } else {
                                member.confinement = None;
                            }
                        }
                    }
                    changed.insert(m.id, m);
                }
            }
            MissionReceipt::Departed { actor, from } => {
                plan.guard_membership(self, *actor);
                if let Some(id) = self.membership(*actor) {
                    let mut m = self.missions[&id].clone();
                    m.members
                        .get_mut(actor)
                        .ok_or(Error::Unavailable)?
                        .last_leg_from = Some(*from);
                    changed.insert(id, m);
                }
            }
            MissionReceipt::Arrived {
                actor,
                area,
                observers,
                contacts,
            } => {
                for observer in observers {
                    plan.guard_membership(self, *observer);
                    if let Some(id) = self.membership(*observer) {
                        let m = changed
                            .entry(id)
                            .or_insert_with(|| self.missions[&id].clone());
                        m.members
                            .get_mut(observer)
                            .ok_or(Error::Unavailable)?
                            .contacts
                            .insert(*actor, *area);
                    }
                }
                plan.guard_membership(self, *actor);
                if let Some(id) = self.membership(*actor) {
                    let m = changed
                        .entry(id)
                        .or_insert_with(|| self.missions[&id].clone());
                    let p = m.members.get_mut(actor).ok_or(Error::Unavailable)?;
                    p.visited.insert(*area);
                    for contact in contacts.iter().take(64) {
                        p.contacts.insert(*contact, *area);
                    }
                }
            }
            MissionReceipt::Transferred {
                actor,
                source,
                destination,
                quantity,
                destination_carrier,
            } => {
                if source == destination && quantity.0 > 0 {
                    return Err(Error::Unavailable);
                }
                for old in self.missions.values().filter(|m| !terminal(m.status)) {
                    let relevant = old.members.values().any(|p| {
                        p.supply_cargo
                            .values()
                            .any(|tags| tags.contains_key(source))
                            || old
                                .proposal
                                .supplies
                                .iter()
                                .any(|s| s.carrier == p.person && s.source == *source)
                            || p.cargo.contains_key(source)
                            || p.person == *actor
                                && p.status == CompanionStatus::Accepted
                                && matches!(p.directive, MissionObjective::Deliver { .. })
                    });
                    if !relevant {
                        continue;
                    }
                    let mut m = old.clone();
                    for p in m.members.values_mut() {
                        let mut unallocated = quantity.0;
                        let mut moved_tags = BTreeMap::new();
                        // Debit existing provenance first. A carried compartment may
                        // itself be another named source; its identity cannot leave
                        // older tags behind after those physical goods moved away.
                        for (origin, tags) in &mut p.supply_cargo {
                            let tagged = tags.remove(source).unwrap_or(0);
                            let moved = tagged.min(unallocated);
                            if tagged > moved {
                                tags.insert(*source, tagged - moved);
                            }
                            unallocated -= moved;
                            moved_tags.insert(*origin, moved);
                        }
                        let mut supplies = BTreeMap::<InventoryId, u64>::new();
                        for supply in m.proposal.supplies.iter().filter(|s| s.carrier == p.person) {
                            supplies
                                .entry(supply.source)
                                .and_modify(|q| *q = (*q).max(supply.quantity.0))
                                .or_insert(supply.quantity.0);
                        }
                        for (origin, quantity_owed) in supplies {
                            let tags = p.supply_cargo.entry(origin).or_default();
                            let moved = moved_tags.get(&origin).copied().unwrap_or(0);
                            if p.status == CompanionStatus::Accepted
                                && *destination_carrier == Some(p.person)
                            {
                                let picked = if *source == origin {
                                    unallocated
                                } else {
                                    moved
                                };
                                let held: u128 = tags.values().map(|q| u128::from(*q)).sum();
                                let room = u128::from(quantity_owed).saturating_sub(held) as u64;
                                let credited = picked.min(room);
                                if credited > 0 {
                                    *tags.entry(*destination).or_default() += credited;
                                    if *source == origin {
                                        unallocated -= credited;
                                    }
                                }
                            }
                        }
                        p.supply_cargo.retain(|_, tags| !tags.is_empty());
                        let tagged = p.cargo.get(source).copied().unwrap_or(0).min(quantity.0);
                        if tagged > 0 {
                            let left = p.cargo[source] - tagged;
                            if left == 0 {
                                p.cargo.remove(source);
                            } else {
                                p.cargo.insert(*source, left);
                            }
                        }
                        if p.person != *actor || p.status != CompanionStatus::Accepted {
                            continue;
                        }
                        let MissionObjective::Deliver {
                            source: origin,
                            destination: goal,
                            quantity: owed,
                        } = p.directive
                        else {
                            continue;
                        };
                        let picked = if *source == origin {
                            quantity.0.min(owed.0.saturating_sub(p.delivered))
                        } else {
                            tagged
                        };
                        if *destination == goal && *source != goal && picked > 0 {
                            p.delivered = p.delivered.saturating_add(picked).min(owed.0);
                            p.explanation = format!("Physically delivered {} units", p.delivered);
                        } else if picked > 0 && *destination_carrier == Some(*actor) {
                            let held = p.cargo.entry(*destination).or_default();
                            *held = held
                                .saturating_add(picked)
                                .min(owed.0.saturating_sub(p.delivered));
                        }
                    }
                    changed.insert(m.id, m);
                }
            }
            MissionReceipt::Lost { source, quantity } => {
                for old in self.missions.values().filter(|m| {
                    m.members.values().any(|p| {
                        p.cargo.contains_key(source)
                            || p.supply_cargo
                                .values()
                                .any(|tags| tags.contains_key(source))
                    })
                }) {
                    let mut m = old.clone();
                    for p in m.members.values_mut() {
                        let mut lost = quantity.0;
                        for tags in p.supply_cargo.values_mut() {
                            if let Some(held) = tags.remove(source) {
                                let removed = held.min(lost);
                                lost -= removed;
                                if held > removed {
                                    tags.insert(*source, held - removed);
                                }
                            }
                        }
                        p.supply_cargo.retain(|_, tags| !tags.is_empty());
                        if let Some(held) = p.cargo.get_mut(source) {
                            *held = held.saturating_sub(quantity.0);
                            if *held == 0 {
                                p.cargo.remove(source);
                            }
                        }
                    }
                    changed.insert(m.id, m);
                }
            }
        }
        for m in changed.into_values() {
            plan.mission(self, m);
        }
        // A receipt updates facts; receiving observers are projected separately.
        plan.notices.clear();
        Ok(plan)
    }
    pub fn prepare_outcome(
        &self,
        id: MissionId,
        f: &OutcomeFacts,
    ) -> Result<PreparedMission, Error> {
        let old = self.missions.get(&id).ok_or(Error::Unavailable)?;
        let mut plan = PreparedMission::default();
        if terminal(old.status) {
            return Ok(plan);
        }
        let mut m = old.clone();
        for p in m.members.values_mut().filter(|p| {
            matches!(
                p.status,
                CompanionStatus::Accepted | CompanionStatus::Captured
            )
        }) {
            let body = f.bodies.get(&p.person).ok_or(Error::Unavailable)?;
            let status = if !body.alive {
                CompanionStatus::Dead
            } else if body.captive {
                CompanionStatus::Captured
            } else if p.status == CompanionStatus::Captured {
                CompanionStatus::Accepted
            } else {
                continue;
            };
            if status == p.status {
                continue;
            }
            p.status = status;
            p.declared_explanation = match status {
                CompanionStatus::Dead => "Died",
                CompanionStatus::Captured => "Captured",
                _ => "Released",
            }
            .into();
            p.explanation = match status {
                CompanionStatus::Dead => "Died during the expedition",
                CompanionStatus::Captured => "Captured; unable to travel with the expedition",
                _ => "Released; may choose to resume the expedition",
            }
            .into();
            plan.events.push(MissionEvent::RecruitmentChanged {
                mission: id,
                person: p.person,
                status,
            });
        }
        let leader = m
            .members
            .get(&m.proposal.leader)
            .ok_or(Error::Unavailable)?;
        if !matches!(
            leader.status,
            CompanionStatus::Accepted | CompanionStatus::Offered
        ) {
            finish(
                &mut m,
                false,
                "The leader can no longer lead this expedition",
                &mut plan,
                f,
            );
        } else if m.status == MissionStatus::Preparing {
            if f.now >= m.proposal.deadline {
                finish(
                    &mut m,
                    false,
                    "Recruitment or preparation missed the agreed deadline",
                    &mut plan,
                    f,
                );
            }
        } else {
            let leader_area = f
                .bodies
                .get(&m.proposal.leader)
                .ok_or(Error::Unavailable)?
                .area;
            let fresh_target = f.leader_knowledge.target.filter(|t| {
                t.observed_at == f.now
                    && Some(t.area) == leader_area
                    && Some(t.person) == m.proposal.objective.target_person()
            });
            let achieved = match &m.proposal.objective {
                MissionObjective::Visit { destination } => leader_area == Some(*destination),
                MissionObjective::Deliver { quantity, .. } => {
                    m.members
                        .values()
                        .filter(|p| {
                            leader_area.is_some()
                                && f.bodies
                                    .get(&p.person)
                                    .is_some_and(|b| b.area == leader_area)
                        })
                        .map(|p| u128::from(p.delivered))
                        .sum::<u128>()
                        >= u128::from(quantity.0)
                }
                MissionObjective::Rescue { return_to, .. } => {
                    leader_area == Some(*return_to) && fresh_target.is_some_and(|t| !t.restricted)
                }
                MissionObjective::Defeat { .. } => fresh_target.is_some_and(|t| {
                    !t.alive
                        || t.health <= t.incapacitation
                        || t.captor.is_some_and(|p| m.members.contains_key(&p))
                }),
                MissionObjective::Confine { person, until } => {
                    fresh_target.is_some_and(|t| t.alive)
                        && f.now >= *until
                        && m.members.values().any(|p| {
                            p.confinement
                                .as_ref()
                                .is_some_and(|r| r.finished(*person, *until, f.now))
                        })
                }
                MissionObjective::Patrol { areas, until } => {
                    f.now >= *until && areas.is_subset(&leader.visited)
                }
            };
            if m.status == MissionStatus::Traveling
                && !achieved
                && leader_area.is_some()
                && matches!(
                    m.proposal.objective,
                    MissionObjective::Defeat { .. }
                        | MissionObjective::Confine { .. }
                        | MissionObjective::Rescue { .. }
                        | MissionObjective::Patrol { .. }
                )
                && objective_destination(
                    leader,
                    &m.proposal.objective,
                    &f.leader_knowledge,
                    leader_area,
                ) == leader_area
            {
                m.status = MissionStatus::Operating;
                m.explanation =
                    "Leader is present at the objective and undertaking the local task".into();
            }
            if m.status != MissionStatus::Returning && (achieved || f.now >= m.proposal.deadline) {
                m.status = MissionStatus::Returning;
                m.succeeded = achieved;
                m.explanation = if achieved {
                    "Leader observed the objective achieved and is returning"
                } else {
                    "The objective deadline passed; leader is returning without success"
                }
                .into();
            }
            if m.status == MissionStatus::Returning && leader_area == Some(m.proposal.home) {
                let success = m.succeeded;
                finish(
                    &mut m,
                    success,
                    if success {
                        "Leader returned after observing the objective achieved"
                    } else {
                        "Leader returned after retreat or failure"
                    },
                    &mut plan,
                    f,
                );
            }
        }
        if &m != old {
            plan.mission(self, m);
        }
        plan.notices.clear(); // Knowing an outcome requires actual observation.
        Ok(plan)
    }
}
fn finish(
    m: &mut Mission,
    success: bool,
    message: &str,
    plan: &mut PreparedMission,
    f: &OutcomeFacts,
) {
    m.status = if success {
        MissionStatus::Completed
    } else {
        MissionStatus::Failed
    };
    m.explanation = message.into();
    for member in m.members.values_mut() {
        member.cargo.clear();
        member.supply_cargo.clear();
    }
    plan.events.push(MissionEvent::Finished {
        mission: m.id,
        status: m.status,
    });
    for person in f
        .receivers
        .intersection(&m.members.keys().copied().chain([m.sponsor]).collect())
    {
        if *person != m.proposal.leader {
            plan.trust.push(TrustEffect {
                observer: *person,
                leader: m.proposal.leader,
                delta: if success { 100 } else { -50 },
            });
        }
    }
}
/// Chooses a destination from this person's memories and observed target, never
/// reading a global location. Route feasibility and actual travel remain owned
/// by navigation/transport; a destination belief can be stale or unreachable.
pub fn objective_destination(
    member: &Companion,
    objective: &MissionObjective,
    k: &ObjectiveKnowledge,
    own_area: Option<AreaId>,
) -> Option<AreaId> {
    match objective {
        MissionObjective::Visit { destination } => Some(*destination),
        MissionObjective::Deliver {
            source,
            destination,
            ..
        } => k
            .stocks
            .get(if member.cargo.values().any(|q| *q > 0) {
                destination
            } else {
                source
            })
            .map(|k| k.area),
        MissionObjective::Rescue { person, return_to } => {
            if k.target
                .is_some_and(|t| t.person == *person && Some(t.area) == own_area && !t.restricted)
            {
                Some(*return_to)
            } else {
                member.contacts.get(person).copied()
            }
        }
        MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
            member.contacts.get(person).copied()
        }
        MissionObjective::Patrol { areas, .. } => areas
            .iter()
            .find(|area| !member.visited.contains(area))
            .copied(),
    }
}

#[derive(Clone, Debug)]
pub struct ServiceMotives {
    pub honesty: u16,
    pub leader_trust: i32,
    pub guardian_duty: bool,
    /// Only the actor's own dated view, acquired through observation/report.
    pub payment: Option<AgreementView>,
}
#[derive(Clone, Copy, Debug)]
pub struct ObservedOpponent {
    pub trust: i32,
    pub team: u32,
}
#[derive(Clone, Debug)]
pub struct DefectionFacts {
    pub motives: ServiceMotives,
    pub own_team: u32,
    pub opponents: BTreeMap<PersonId, ObservedOpponent>,
}
impl World {
    fn payment_failed(&self, actor: PersonId, id: MissionId, motives: &ServiceMotives) -> bool {
        let Some(member) = self.missions.get(&id).and_then(|m| m.members.get(&actor)) else {
            return false;
        };
        let Some(agreement) = self
            .recruitments
            .get(&member.recruitment)
            .and_then(|r| r.agreement)
        else {
            return false;
        };
        motives.payment.as_ref().is_some_and(|view| {
            view.id == agreement
                && view
                    .obligations
                    .iter()
                    .any(|o| o.defaulted && o.term.parties()[1] == actor)
        })
    }
    pub fn service_withdrawal(
        &self,
        actor: PersonId,
        id: MissionId,
        motives: &ServiceMotives,
    ) -> Option<MissionAction> {
        if self.missions.get(&id)?.members.get(&actor)?.status != CompanionStatus::Accepted {
            return None;
        }
        (motives.guardian_duty
            || self.payment_failed(actor, id, motives) && motives.honesty < 800
            || motives.leader_trust < -500)
            .then_some(MissionAction::Leave { mission: id })
    }
    pub fn prepare_defection(
        &self,
        actor: PersonId,
        id: MissionId,
        f: &DefectionFacts,
    ) -> Result<Option<PreparedMission>, Error> {
        let old = self.missions.get(&id).ok_or(Error::Unavailable)?;
        let member = old.members.get(&actor).ok_or(Error::Unavailable)?;
        if member.status != CompanionStatus::Accepted || f.motives.honesty >= 500 {
            return Ok(None);
        }
        let Some((opponent, other)) = f
            .opponents
            .iter()
            .filter(|(_, p)| p.team != f.own_team)
            .max_by_key(|(id, p)| (p.trust, **id))
        else {
            return Ok(None);
        };
        if i64::from(other.trust) < i64::from(f.motives.leader_trust) + 500
            || !(self.payment_failed(actor, id, &f.motives) || f.motives.leader_trust < -500)
        {
            return Ok(None);
        }
        let mut m = old.clone();
        let p = m.members.get_mut(&actor).unwrap();
        p.status = CompanionStatus::Defected;
        p.declared_explanation = "Changed allegiance during combat".into();
        p.explanation = "Chose a trusted opponent over a distrusted or defaulting leader".into();
        let mut plan = PreparedMission::default();
        let offer = self
            .recruitments
            .get(&member.recruitment)
            .ok_or(Error::Unavailable)?;
        plan.recruitment(self, offer.clone());
        plan.mission(self, m);
        plan.notices.clear();
        plan.allegiance = Some(AllegianceEffect {
            actor,
            opponent: *opponent,
            team: other.team,
        });
        plan.events.push(MissionEvent::RecruitmentChanged {
            mission: id,
            person: actor,
            status: CompanionStatus::Defected,
        });
        Ok(Some(plan))
    }
}
pub fn combat_retreat_wanted(
    health: u64,
    max_health: u64,
    risk_tolerance: u16,
    known_status: Option<MissionStatus>,
) -> bool {
    health < max_health / 2 && risk_tolerance < 600
        || known_status == Some(MissionStatus::Returning)
}

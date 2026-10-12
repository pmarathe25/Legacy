//! Available intentions and autonomous review selection use personal knowledge.
use crate::*;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;
pub trait CommandAvailabilityFacts {
    fn area(&self, person: PersonId) -> Option<AreaId>;
    fn in_combat(&self, person: PersonId) -> bool;
    fn urgent_care(&self, person: PersonId) -> bool;
    fn guardian_duty(&self, person: PersonId, mission: MissionId) -> bool;
    fn local_people(&self, area: AreaId) -> impl Iterator<Item = PersonId>;
}
pub fn urgent_service_care(urgencies: impl IntoIterator<Item = i64>) -> bool {
    urgencies.into_iter().any(|urgency| urgency >= 20_000)
}
pub fn recruitment_disruption(
    actor_area: Option<AreaId>,
    sponsor_area: Option<AreaId>,
    penalty: i64,
) -> i64 {
    if actor_area.is_some() && actor_area == sponsor_area {
        penalty / 3
    } else {
        0
    }
}
impl Runtime {
    pub fn available_commands(
        &self,
        store: &Store,
        actor: PersonId,
        facts: &impl CommandAvailabilityFacts,
    ) -> Vec<MissionAction> {
        let Some(area) = facts.area(actor) else {
            return Vec::new();
        };
        if facts.in_combat(actor) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        for view in self.views(store, actor) {
            let Some(m) = mission(store, view.id) else {
                continue;
            };
            for offer in &view.recruitments {
                if offer.status != CompanionStatus::Offered {
                    continue;
                }
                if offer.person == actor && facts.area(m.sponsor) == Some(area) {
                    if !offer.countered
                        && view.status == MissionStatus::Preparing
                        && !facts.guardian_duty(actor, m.id)
                        && !facts.urgent_care(actor)
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
                    && facts.area(offer.person) == Some(area)
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
            if facts.area(m.proposal.leader) == Some(area)
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
            for recipient in facts
                .local_people(area)
                .filter(|p| *p != actor)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
            {
                if facts.area(recipient).is_some() {
                    actions.push(MissionAction::Report {
                        mission: m.id,
                        recipient,
                    });
                }
            }
        }
        actions
    }
    pub fn review_due(&self, now: SimTime) -> bool {
        now != self.last_review && now.0.is_multiple_of(60_000)
    }
    pub fn reviewed(&mut self, now: SimTime) {
        self.last_review = now;
    }
    pub fn recruitment_reviews(
        &self,
        store: &Store,
        player: PersonId,
        facts: &impl CommandAvailabilityFacts,
    ) -> Vec<RecruitmentId> {
        recruitments(store)
            .filter_map(|(id, offer)| {
                let m = mission(store, offer.mission)?;
                (offer.person != player
                    && m.status == MissionStatus::Preparing
                    && m.members[&offer.person].status == CompanionStatus::Offered
                    && !offer.countered
                    && facts
                        .area(offer.person)
                        .is_some_and(|area| facts.area(m.sponsor) == Some(area)))
                .then_some(id)
            })
            .collect()
    }
    pub fn autonomous_departure(
        &self,
        store: &Store,
        id: MissionId,
        player: PersonId,
    ) -> Option<PersonId> {
        let m = mission(store, id)?;
        (m.status == MissionStatus::Preparing
            && m.proposal.leader != player
            && m.members[&m.proposal.leader].status == CompanionStatus::Accepted
            && m.members
                .values()
                .all(|person| person.status != CompanionStatus::Offered))
        .then_some(m.proposal.leader)
    }
    pub fn control_members(
        &self,
        store: &Store,
        index: &MembershipIndex,
        id: MissionId,
    ) -> Vec<PersonId> {
        mission(store, id)
            .into_iter()
            .flat_map(|m| m.members.keys().copied())
            .filter(|person| index.membership(store, *person) == Some(id))
            .collect()
    }
}
pub trait CommandFactSource: CommandAvailabilityFacts {
    fn assets(
        &self,
        objective: Option<&MissionObjective>,
        terms: &[game_agreements::AgreementTerm],
        supplies: &[MissionSupply],
    ) -> MissionAssets;
}
impl Runtime {
    pub fn command_facts(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        action: &MissionAction,
        now: SimTime,
        source: &impl CommandFactSource,
    ) -> Result<CommandFacts, Error> {
        let mut relevant = BTreeSet::from([actor]);
        let mut terms = Vec::new();
        let mut objective = None;
        let mut supplies = Vec::new();
        let mut duties = BTreeSet::new();
        match action {
            MissionAction::Propose { proposal } => {
                relevant.extend(proposal.recruits.iter().copied());
                relevant.insert(proposal.leader);
                terms.extend(proposal.compensation.values().flatten().cloned());
                objective = Some(&proposal.objective);
                supplies = proposal.supplies.clone();
            }
            MissionAction::Accept { recruitment: id }
            | MissionAction::Refuse { recruitment: id }
            | MissionAction::AcceptCounteroffer { recruitment: id }
            | MissionAction::Counteroffer {
                recruitment: id, ..
            } => {
                let offer = recruitment(store, *id).ok_or(Error::Unavailable)?;
                let m = mission(store, offer.mission).ok_or(Error::Unavailable)?;
                relevant.extend([offer.person, m.sponsor]);
                terms = if let MissionAction::Counteroffer { compensation, .. } = action {
                    compensation.clone()
                } else {
                    offer.compensation.clone()
                };
                if source.guardian_duty(actor, m.id) || source.urgent_care(actor) {
                    duties.insert(actor);
                }
            }
            MissionAction::Depart { mission: id }
            | MissionAction::Retreat { mission: id }
            | MissionAction::Leave { mission: id }
            | MissionAction::Objective { mission: id, .. } => {
                let m = mission(store, *id).ok_or(Error::Unavailable)?;
                relevant.insert(m.proposal.leader);
                if let MissionAction::Objective {
                    objective: asked, ..
                } = action
                {
                    objective = Some(asked);
                }
            }
            MissionAction::Report { recipient, .. } => {
                relevant.insert(*recipient);
            }
        }
        let proposing = matches!(action, MissionAction::Propose { .. });
        let contacts = if proposing {
            source
                .local_people(area)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(64)
                .map(|p| (p, area))
                .collect()
        } else {
            Default::default()
        };
        Ok(CommandFacts {
            now,
            area,
            available: relevant
                .into_iter()
                .filter(|id| {
                    source.area(*id) == Some(area) && (!proposing || !source.in_combat(*id))
                })
                .collect(),
            duties,
            contacts,
            assets: source.assets(objective, &terms, &supplies),
        })
    }
    pub fn command_ids(
        &self,
        store: &Store,
        action: &MissionAction,
        next: u64,
    ) -> Result<(MissionIds, u64), Error> {
        let mut ids = MissionIds::default();
        let mut after = next;
        if let MissionAction::Propose { proposal } = action {
            let people = proposal
                .recruits
                .iter()
                .copied()
                .chain([proposal.leader])
                .collect::<BTreeSet<_>>();
            next.checked_add(1 + people.len() as u64)
                .ok_or(Error::Overflow)?;
            ids.mission = Some(MissionId(after));
            after += 1;
            for person in people {
                ids.recruitments.insert(person, RecruitmentId(after));
                after += 1;
            }
        } else if let MissionAction::Accept { recruitment: id } = action
            && !recruitment(store, *id)
                .ok_or(Error::Unavailable)?
                .compensation
                .is_empty()
        {
            ids.backing = Some(AgreementId(after));
        }
        Ok((ids, after))
    }
}

pub fn service_hungry(hunger: u64, meal_relief: u64) -> bool {
    hunger >= meal_relief
}

impl Runtime {
    pub fn validate_action_presence(
        &self,
        actor: PersonId,
        source: &impl CommandAvailabilityFacts,
    ) -> Result<(), Error> {
        if source.in_combat(actor) {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}

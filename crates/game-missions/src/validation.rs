//! Intrinsic record shape; actual people, stocks and clock are composed separately.
use crate::*;
use game_core::*;
use std::collections::BTreeSet;
fn fail() -> Error {
    Error::InvalidSave("invalid mission component".into())
}
fn objective(o: &MissionObjective) -> bool {
    match o {
        MissionObjective::Visit { destination } => destination.0 > 0,
        MissionObjective::Deliver {
            source,
            destination,
            quantity,
        } => source.0 > 0 && destination.0 > 0 && source != destination && quantity.0 > 0,
        MissionObjective::Defeat { person } | MissionObjective::Confine { person, .. } => {
            person.0 > 0
        }
        MissionObjective::Rescue { person, return_to } => person.0 > 0 && return_to.0 > 0,
        MissionObjective::Patrol { areas, .. } => {
            !areas.is_empty() && areas.len() <= 16 && areas.iter().all(|id| id.0 > 0)
        }
    }
}
fn terms(terms: &[game_agreements::AgreementTerm]) -> bool {
    terms.len() <= 8
        && terms.iter().all(|t| {
            t.amount() > 0
                && t.parties()[0] != t.parties()[1]
                && t.parties().iter().all(|p| p.0 > 0)
        })
}
impl Mission {
    pub fn validate(&self) -> Result<(), Error> {
        let roster: BTreeSet<_> = self
            .proposal
            .recruits
            .iter()
            .copied()
            .chain([self.proposal.leader])
            .collect();
        if self.id.0 == 0
            || self.sponsor.0 == 0
            || self.proposal.leader.0 == 0
            || self.proposal.home.0 == 0
            || !objective(&self.proposal.objective)
            || !(1..=32).contains(&self.members.len())
            || roster != self.members.keys().copied().collect()
            || self.proposal.deadline <= self.created_at
            || self.departed_at.is_some_and(|at| at < self.created_at)
            || self.proposal.supplies.len() > 32
            || self.explanation.len() > 4096
            || self
                .proposal
                .compensation
                .iter()
                .any(|(actor, t)| !self.members.contains_key(actor) || !terms(t))
            || self.proposal.supplies.iter().any(|s| {
                !self.members.contains_key(&s.carrier) || s.source.0 == 0 || s.quantity.0 == 0
            })
        {
            return Err(fail());
        }
        for (id, m) in &self.members {
            if id.0 == 0
                || *id != m.person
                || m.recruitment.0 == 0
                || !objective(&m.directive)
                || m.visited.iter().any(|a| a.0 == 0)
                || m.contacts
                    .iter()
                    .any(|(person, area)| person.0 == 0 || area.0 == 0)
                || m.last_leg_from.is_some_and(|a| a.0 == 0)
                || m.explanation.len() > 4096
                || m.declared_explanation.len() > 4096
                || m.cargo.iter().any(|(id, q)| id.0 == 0 || *q == 0)
                || m.supply_cargo.iter().any(|(source, tags)| {
                    source.0 == 0
                        || tags.is_empty()
                        || tags.iter().any(|(id, q)| id.0 == 0 || *q == 0)
                })
            {
                return Err(fail());
            }
            if let Some(c) = &m.confinement
                && (!matches!(m.directive,MissionObjective::Confine{person,..}if person==c.person)
                    || c.released_at.is_some_and(|at| at < c.captured_at))
            {
                return Err(fail());
            }
            if let MissionObjective::Deliver { quantity, .. } = m.directive
                && m.delivered > quantity.0
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
impl Recruitment {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.mission.0 == 0
            || self.person.0 == 0
            || !terms(&self.compensation)
            || self.agreement.is_some_and(|id| id.0 == 0)
        {
            return Err(fail());
        }
        Ok(())
    }
}
fn view(view: &MissionView) -> bool {
    view.id.0 > 0
        && view.sponsor.0 > 0
        && view.leader.0 > 0
        && view.home.0 > 0
        && objective(&view.objective)
        && view.companions.len() <= 32
        && view.recruitments.len() <= 32
        && view.supplies.len() <= 32
        && view.explanation.len() <= 4096
        && view.reported_by.is_none_or(|id| id.0 > 0)
        && view.companions.iter().all(|c| {
            c.person.0 > 0
                && c.explanation.len() <= 4096
                && c.location
                    .as_ref()
                    .is_none_or(|location| game_spatial::validate(location).is_ok())
        })
        && view.recruitments.iter().all(|r| {
            r.id.0 > 0
                && r.person.0 > 0
                && terms(&r.compensation)
                && r.payment_agreement.is_none_or(|id| id.0 > 0)
        })
        && view
            .supplies
            .iter()
            .all(|s| s.carrier.0 > 0 && s.source.0 > 0 && s.quantity.0 > 0)
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.missions.iter().any(|(id, m)| *id != m.id || !view(m)) {
            Err(fail())
        } else {
            Ok(())
        }
    }
}
impl PendingTalk {
    pub fn validate(&self) -> Result<(), Error> {
        if self.speaker.0 == 0 || self.listener.0 == 0 || self.area.0 == 0 || self.completes.0 == 0
        {
            return Err(fail());
        }
        let valid = match &self.conversation {
            Conversation::Objective {
                mission,
                leader,
                objective: o,
            } => mission.0 > 0 && leader.0 > 0 && objective(o),
            Conversation::Report {
                recipient,
                snapshot,
            } => *recipient == self.listener && view(snapshot),
            Conversation::Retreat { mission, leader } => mission.0 > 0 && leader.0 > 0,
        };
        if valid { Ok(()) } else { Err(fail()) }
    }
}

impl Runtime {
    pub fn validate_storage(&self, store: &game_ecs::Store, now: SimTime) -> Result<(), Error> {
        if self.last_review > now {
            return Err(fail());
        }
        for (id, m) in missions(store) {
            m.validate()?;
            if id != m.id || m.created_at > now || m.departed_at.is_some_and(|at| at > now) {
                return Err(fail());
            }
            for member in m.members.values() {
                let offer = recruitment(store, member.recruitment).ok_or_else(fail)?;
                if offer.mission != id || offer.person != member.person || member.received_at > now
                {
                    return Err(fail());
                }
            }
        }
        for (id, row) in recruitments(store) {
            row.validate()?;
            if id != row.id
                || !mission(store, row.mission).is_some_and(|m| {
                    m.members
                        .get(&row.person)
                        .is_some_and(|member| member.recruitment == id)
                })
            {
                return Err(fail());
            }
        }
        for (_, row) in knowledges(store) {
            row.validate()?;
            if row.missions.values().any(|v| {
                mission(store, v.id).is_none()
                    || v.observed_at > now
                    || v.companions.iter().any(|m| m.observed_at > now)
            }) {
                return Err(fail());
            }
        }
        for (id, row) in talks(store) {
            row.validate()?;
            if id != row.speaker {
                return Err(fail());
            }
        }
        Ok(())
    }
}

use crate::*;
use game_core::*;
use game_missions::{MissionObjective, MissionView};
use std::collections::BTreeSet;

pub struct ValidationCatalog<'a> {
    pub people: &'a BTreeSet<PersonId>,
    pub missions: &'a BTreeSet<MissionId>,
    pub now: SimTime,
}
impl Kernel<'_> {
    /// Native mission views remain the mission owner's protocol. Its validator
    /// checks nested objectives, compensation and inventory/area references.
    /// Remembered outcomes need not agree with current distant world facts.
    pub fn validate(
        &self,
        catalog: &ValidationCatalog<'_>,
        mut validate_objective: impl FnMut(&MissionObjective) -> Result<(), Error>,
        mut validate_mission_view: impl FnMut(&MissionView) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let c = catalog;
        let mut commanded = BTreeSet::new();
        let mut orders = BTreeSet::new();
        let mut reports = BTreeSet::new();
        for (id, a) in armies(self.store) {
            require(
                id.0 > 0
                    && a.id == id
                    && c.people.contains(&a.commander)
                    && a.formed_at <= c.now
                    && !a.detachments.is_empty()
                    && a.detachments.len() <= MAX_DETACHMENTS
                    && a.disbanded_at
                        .is_none_or(|at| at >= a.formed_at && at <= c.now),
            )?;
            for (mission, d) in &a.detachments {
                require(
                    d.mission == *mission
                        && c.missions.contains(mission)
                        && c.people.contains(&d.leader)
                        && detachment_valid(d, a.formed_at, c.now),
                )?;
                if a.disbanded_at.is_none() && d.participation.active() {
                    require(commanded.insert(*mission))?;
                }
            }
            for (id, o) in &a.orders {
                require(id.0 > 0 && o.id == *id && orders.insert(*id) && order_valid(o, a, c.now))?;
                validate_objective(&o.objective)?;
            }
            for (id, r) in &a.reports {
                require(
                    id.0 > 0
                        && r.id == *id
                        && reports.insert(*id)
                        && r.army == a.id
                        && r.sent_at >= a.formed_at
                        && r.sent_at <= c.now
                        && a.detachments
                            .get(&r.mission)
                            .is_some_and(|d| d.leader == r.speaker)
                        && r.snapshot.id == r.mission
                        && r.snapshot.leader == r.speaker
                        && r.snapshot.observed_at <= r.sent_at,
                )?;
                validate_mission_view(&r.snapshot)?;
            }
        }
        for (actor, row) in knowledges(self.store) {
            let armies = &row.armies;
            require(c.people.contains(&actor))?;
            for (id, k) in armies {
                let a = army(self.store, *id).ok_or_else(invalid)?;
                if let Some(d) = &k.commander {
                    require(dated_valid(d, actor, c) && d.value == a.commander)?;
                }
                if let Some(d) = &k.disbanded_at {
                    require(dated_valid(d, actor, c) && Some(d.value) == a.disbanded_at)?;
                }
                for (mission, d) in &k.detachments {
                    let actual = a.detachments.get(mission).ok_or_else(invalid)?;
                    require(
                        dated_valid(d, actor, c)
                            && d.value.mission == *mission
                            && d.value.leader == actual.leader
                            && detachment_valid(&d.value, a.formed_at, c.now),
                    )?;
                }
                for (id, d) in &k.orders {
                    let actual = a.orders.get(id).ok_or_else(invalid)?;
                    require(
                        dated_valid(d, actor, c)
                            && same_order(&d.value, actual)
                            && order_valid(&d.value, a, c.now),
                    )?;
                }
                for (id, rs) in &k.responses {
                    let o = a.orders.get(id).ok_or_else(invalid)?;
                    for (mission, d) in rs {
                        require(
                            dated_valid(d, actor, c)
                                && recipient_valid(&d.value, o, c.now)
                                && d.value.responded_at.is_some()
                                && o.recipients
                                    .get(mission)
                                    .is_some_and(|actual| actual == &d.value),
                        )?;
                    }
                }
                for (id, d) in &k.reports {
                    require(dated_valid(d, actor, c) && a.reports.get(id) == Some(&d.value))?;
                }
            }
        }
        Ok(())
    }
}
fn detachment_valid(d: &Detachment, formed: SimTime, now: SimTime) -> bool {
    d.invited_at == formed
        && d.changed_at >= d.invited_at
        && d.changed_at <= now
        && d.heard_at.is_none_or(|at| at >= d.invited_at && at <= now)
        && (d.participation == Participation::Offered
            || d.heard_at.is_some_and(|at| at <= d.changed_at))
        && !d.explanation.is_empty()
        && d.explanation.len() <= 512
}
fn same_order(a: &Order, b: &Order) -> bool {
    a.id == b.id
        && a.commander == b.commander
        && a.objective == b.objective
        && a.issued_at == b.issued_at
        && a.expires == b.expires
        && a.recipients.iter().all(|(m, r)| {
            b.recipients
                .get(m)
                .is_some_and(|actual| r.leader == actual.leader)
        })
}
fn order_valid(o: &Order, a: &Army, now: SimTime) -> bool {
    o.id.0 > 0
        && o.commander == a.commander
        && o.issued_at >= a.formed_at
        && o.issued_at <= now
        && o.expires > o.issued_at
        && !o.recipients.is_empty()
        && o.recipients.len() <= MAX_DETACHMENTS
        && o.recipients.iter().all(|(m, r)| {
            a.detachments.get(m).is_some_and(|d| d.leader == r.leader) && recipient_valid(r, o, now)
        })
}
fn recipient_valid(r: &OrderRecipient, o: &Order, now: SimTime) -> bool {
    r.received_at
        .is_none_or(|at| at >= o.issued_at && at < o.expires && at <= now)
        && r.responded_at.is_none_or(|at| {
            r.received_at
                .is_some_and(|received| at >= received && at < o.expires && at <= now)
        })
        && r.explanation.len() <= 512
        && match r.status {
            OrderStatus::AwaitingDelivery => r.received_at.is_none() && r.responded_at.is_none(),
            OrderStatus::Received | OrderStatus::Superseded => {
                r.received_at.is_some() && r.responded_at.is_none()
            }
            OrderStatus::Accepted
            | OrderStatus::Refused
            | OrderStatus::Retreated
            | OrderStatus::Defected => r.responded_at.is_some() && !r.explanation.is_empty(),
            OrderStatus::Expired => now >= o.expires && r.responded_at.is_none(),
        }
}
fn dated_valid<T>(d: &Dated<T>, actor: PersonId, c: &ValidationCatalog<'_>) -> bool {
    d.observed_at <= d.learned_at
        && d.learned_at <= c.now
        && match d.source {
            KnowledgeSource::Observed(who) => who == actor,
            KnowledgeSource::Reported(who) => c.people.contains(&who),
            KnowledgeSource::Recorded(_) => false,
        }
}
fn invalid() -> Error {
    Error::InvalidSave("Invalid army relationships, dates or personal knowledge".into())
}
fn require(condition: bool) -> Result<(), Error> {
    if condition { Ok(()) } else { Err(invalid()) }
}

/// Local record admission; referential and current-time checks use the catalog.
pub(crate) fn army_shape_valid(a: &Army) -> bool {
    let latest = SimTime(u64::MAX);
    a.id.0 > 0
        && a.commander.0 > 0
        && !a.detachments.is_empty()
        && a.detachments.len() <= MAX_DETACHMENTS
        && a.disbanded_at.is_none_or(|at| at >= a.formed_at)
        && a.detachments.iter().all(|(id, d)| {
            id.0 > 0
                && d.mission == *id
                && d.leader.0 > 0
                && detachment_valid(d, a.formed_at, latest)
        })
        && a.orders
            .iter()
            .all(|(id, o)| id.0 > 0 && o.id == *id && order_valid(o, a, latest))
        && a.reports.iter().all(|(id, r)| {
            id.0 > 0
                && r.id == *id
                && r.army == a.id
                && r.sent_at >= a.formed_at
                && r.snapshot.id == r.mission
                && r.snapshot.leader == r.speaker
                && r.snapshot.observed_at <= r.sent_at
                && a.detachments
                    .get(&r.mission)
                    .is_some_and(|d| d.leader == r.speaker)
        })
}

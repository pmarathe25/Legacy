use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

/// Visibility/hearing belongs to the physical scene owner. Only actual visible
/// named people and completed/local declarations enter a personal snapshot.
#[derive(Clone, Debug)]
pub struct ObservationFacts {
    pub now: SimTime,
    pub visible_locations: BTreeMap<PersonId, Location>,
    pub heard: BTreeSet<PersonId>,
}
#[derive(Clone, Debug)]
pub struct PreparedObservation {
    observer: PersonId,
    mission: MissionId,
    before: Option<MissionView>,
    pub snapshot: MissionView,
}
impl PreparedObservation {
    pub fn commit(self, state: &mut World) -> Result<(), Error> {
        if state
            .known
            .get(&self.observer)
            .and_then(|k| k.get(&self.mission))
            != self.before.as_ref()
        {
            return Err(Error::Unavailable);
        }
        state
            .known
            .entry(self.observer)
            .or_default()
            .insert(self.mission, self.snapshot);
        Ok(())
    }
}
impl World {
    pub fn prepare_observation(
        &self,
        observer: PersonId,
        id: MissionId,
        f: &ObservationFacts,
    ) -> Result<PreparedObservation, Error> {
        let m = self.missions.get(&id).ok_or(Error::Unavailable)?;
        let old = self.known.get(&observer).and_then(|k| k.get(&id));
        let leader_heard = f.heard.contains(&m.proposal.leader) || observer == m.proposal.leader;
        if !leader_heard && old.is_none() {
            return Err(Error::Unavailable);
        }
        let companions = m
            .members
            .values()
            .filter_map(|p| {
                if let Some(location) = f.visible_locations.get(&p.person) {
                    Some(CompanionView {
                        person: p.person,
                        status: p.status,
                        location: Some(location.clone()),
                        explanation: if p.person == observer {
                            p.explanation.clone()
                        } else {
                            p.declared_explanation.clone()
                        },
                        observed_at: f.now,
                    })
                } else {
                    old.and_then(|view| {
                        view.companions
                            .iter()
                            .find(|known| known.person == p.person)
                    })
                    .cloned()
                }
            })
            .collect();
        let recruitments = m
            .members
            .values()
            .filter(|p| {
                p.person == observer || observer == m.sponsor && f.heard.contains(&p.person)
            })
            .map(|p| {
                let offer = self
                    .recruitments
                    .get(&p.recruitment)
                    .ok_or(Error::Unavailable)?;
                Ok(RecruitmentView {
                    id: offer.id,
                    person: offer.person,
                    status: p.status,
                    compensation: offer.compensation.clone(),
                    payment_agreement: offer.agreement,
                    countered: offer.countered,
                })
            })
            .collect::<Result<_, Error>>()?;
        let snapshot = MissionView {
            id: m.id,
            sponsor: m.sponsor,
            leader: m.proposal.leader,
            objective: if leader_heard {
                m.proposal.objective.clone()
            } else {
                old.unwrap().objective.clone()
            },
            status: if leader_heard {
                m.status
            } else {
                old.unwrap().status
            },
            home: m.proposal.home,
            deadline: m.proposal.deadline,
            companions,
            recruitments,
            supplies: m.proposal.supplies.clone(),
            explanation: if leader_heard {
                m.explanation.clone()
            } else {
                old.unwrap().explanation.clone()
            },
            observed_at: if leader_heard {
                f.now
            } else {
                old.unwrap().observed_at
            },
            reported_by: None,
        };
        Ok(PreparedObservation {
            observer,
            mission: id,
            before: old.cloned(),
            snapshot,
        })
    }
    /// A report carries its original dated snapshot. It cannot read or refresh
    /// current remote mission/body truth, even when newer authority exists.
    pub fn prepare_report(
        &self,
        speaker: PersonId,
        recipient: PersonId,
        snapshot: &MissionView,
        now: SimTime,
    ) -> Result<Option<PreparedObservation>, Error> {
        if speaker == recipient
            || snapshot.observed_at > now
            || snapshot.companions.iter().any(|p| p.observed_at > now)
        {
            return Err(Error::Unavailable);
        }
        let before = self.known.get(&recipient).and_then(|k| k.get(&snapshot.id));
        if before.is_some_and(|old| old.observed_at > snapshot.observed_at) {
            return Ok(None);
        }
        let mut after = snapshot.clone();
        after.reported_by = Some(speaker);
        Ok(Some(PreparedObservation {
            observer: recipient,
            mission: snapshot.id,
            before: before.cloned(),
            snapshot: after,
        }))
    }
}

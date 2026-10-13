use crate::mission as stored_mission;
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
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
    source: Option<Mission>,
    offers: Vec<Recruitment>,
    pub snapshot: MissionView,
}
impl PreparedObservation {
    pub(crate) fn observer(&self) -> PersonId {
        self.observer
    }
    pub(crate) fn add_reads(&self, store: &Store, reads: &mut game_ecs::ReadSet) {
        reads.read::<Knowledge>(store, StableId(self.observer.0));
        reads.read::<Mission>(store, StableId(self.mission.0));
        for offer in &self.offers {
            reads.expect_row(StableId(offer.id.0), Some(offer.clone()));
        }
    }
    pub fn check(&self, store: &Store) -> Result<(), Error> {
        let personal = knowledge(store, self.observer);
        if personal.and_then(|k| k.missions.get(&self.mission)) != self.before.as_ref()
            || self
                .source
                .as_ref()
                .is_some_and(|m| mission(store, self.mission) != Some(m))
            || self
                .offers
                .iter()
                .any(|r| recruitment(store, r.id) != Some(r))
            || mission(store, self.mission).is_none()
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.check(store)?;
        if batch.touches::<Knowledge>(StableId(self.observer.0))
            || batch.touches::<Mission>(StableId(self.mission.0))
            || self
                .offers
                .iter()
                .any(|r| batch.touches::<Recruitment>(StableId(r.id.0)))
        {
            return Err(Error::Unavailable);
        }
        let mut personal = knowledge(store, self.observer).cloned().unwrap_or_default();
        personal.missions.insert(self.mission, self.snapshot);
        personal.validate()?;
        batch.insert(StableId(self.observer.0), personal);
        Ok(())
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        self.stage_into(store, &mut batch)?;
        store.apply_batch(batch)
    }
}
impl Runtime {
    pub fn prepare_observation(
        &self,
        store: &Store,
        observer: PersonId,
        id: MissionId,
        f: &ObservationFacts,
    ) -> Result<PreparedObservation, Error> {
        let m = stored_mission(store, id).ok_or(Error::Unavailable)?;
        let old = knowledge(store, observer)
            .map(|row| &row.missions)
            .and_then(|k| k.get(&id));
        let offers = m
            .members
            .values()
            .filter_map(|member| recruitment(store, member.recruitment).map(|row| (row.id, row)))
            .collect();
        let snapshot = observation_snapshot(m, &offers, observer, old, f)?;
        Ok(PreparedObservation {
            observer,
            mission: id,
            before: old.cloned(),
            source: Some(m.clone()),
            offers: m
                .members
                .values()
                .filter_map(|member| recruitment(store, member.recruitment).cloned())
                .collect(),
            snapshot,
        })
    }
    /// A report carries its original dated snapshot. It cannot read or refresh
    /// current remote mission/body truth, even when newer authority exists.
    pub fn prepare_report(
        &self,
        store: &Store,
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
        let before = knowledge(store, recipient)
            .map(|row| &row.missions)
            .and_then(|k| k.get(&snapshot.id));
        if before.is_some_and(|old| old.observed_at > snapshot.observed_at) {
            return Ok(None);
        }
        let mut after = snapshot.clone();
        after.reported_by = Some(speaker);
        Ok(Some(PreparedObservation {
            observer: recipient,
            mission: snapshot.id,
            before: before.cloned(),
            source: None,
            offers: Vec::new(),
            snapshot: after,
        }))
    }
}

pub(crate) fn observation_snapshot(
    m: &Mission,
    offers: &BTreeMap<RecruitmentId, &Recruitment>,
    observer: PersonId,
    old: Option<&MissionView>,
    f: &ObservationFacts,
) -> Result<MissionView, Error> {
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
        .filter(|p| p.person == observer || observer == m.sponsor && f.heard.contains(&p.person))
        .map(|p| {
            let offer = offers
                .get(&p.recruitment)
                .copied()
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
    Ok(MissionView {
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
    })
}

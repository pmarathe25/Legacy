//! Physical facts bound by the host; service owns availability and local reception.
use crate::*;
use game_conflict::{Encounter, Map};
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct ActorAvailability {
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub captive: bool,
    pub incapacitation: Option<u64>,
}
impl ActorAvailability {
    pub fn area(self, location: &Location) -> Option<AreaId> {
        if !self.alive
            || self.health == 0
            || !self.independent
            || self.captive
            || self
                .incapacitation
                .is_some_and(|limit| self.health <= limit)
        {
            return None;
        }
        match location {
            Location::AtArea(area) => Some(*area),
            _ => None,
        }
    }
}
pub trait PresenceFacts {
    fn location(&self, person: PersonId) -> Location;
    fn raw_location(&self, person: PersonId) -> Location;
    fn area(&self, person: PersonId) -> Option<AreaId>;
    fn same_place(&self, a: PersonId, b: PersonId) -> bool;
    fn in_combat(&self, person: PersonId) -> bool;
    fn combat_scene(&self, person: PersonId) -> Option<(&Encounter, &Map)>;
}
impl Runtime {
    pub fn can_observe(
        &self,
        observer: PersonId,
        person: PersonId,
        facts: &impl PresenceFacts,
    ) -> bool {
        if observer == person {
            return true;
        }
        let location = facts.location(person);
        if facts.location(observer) != location || !matches!(location, Location::AtArea(_)) {
            return false;
        }
        if let Some((encounter, map)) = facts.combat_scene(observer) {
            return encounter.fighters.get(&person).is_some_and(|other| {
                map.clear(encounter.fighters[&observer].position, other.position)
            });
        }
        !facts.in_combat(person)
    }
    pub fn receivers(
        &self,
        store: &Store,
        id: MissionId,
        facts: &impl PresenceFacts,
    ) -> Vec<PersonId> {
        let Some(m) = mission(store, id) else {
            return Vec::new();
        };
        self.receivers_for(m, facts)
    }
    pub(crate) fn receivers_for(&self, m: &Mission, facts: &impl PresenceFacts) -> Vec<PersonId> {
        m.members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|p| {
                facts.area(m.proposal.leader).is_some()
                    && facts.area(*p).is_some()
                    && facts.same_place(*p, m.proposal.leader)
            })
            .collect()
    }
    pub fn observation_facts(
        &self,
        store: &Store,
        observer: PersonId,
        id: MissionId,
        now: SimTime,
        facts: &impl PresenceFacts,
    ) -> Result<ObservationFacts, Error> {
        let m = mission(store, id).ok_or(Error::Unavailable)?;
        Ok(self.observation_facts_for(m, observer, now, facts))
    }
    pub(crate) fn observation_facts_for(
        &self,
        m: &Mission,
        observer: PersonId,
        now: SimTime,
        facts: &impl PresenceFacts,
    ) -> ObservationFacts {
        let relevant = m
            .members
            .keys()
            .copied()
            .chain([m.sponsor])
            .collect::<BTreeSet<_>>();
        let visible_locations = relevant
            .iter()
            .filter(|p| self.can_observe(observer, **p, facts))
            .map(|p| (*p, facts.raw_location(*p)))
            .collect::<BTreeMap<_, _>>();
        let heard = relevant
            .into_iter()
            .filter(|p| {
                facts.area(*p).is_some()
                    && facts.area(observer).is_some()
                    && facts.same_place(observer, *p)
            })
            .collect();
        ObservationFacts {
            now,
            visible_locations,
            heard,
        }
    }
    pub fn encounter_end_responses(
        &self,
        store: &Store,
        encounter: &Encounter,
    ) -> Vec<(MissionId, PersonId)> {
        if encounter.active {
            return Vec::new();
        }
        missions(store)
            .map(|(_, m)| m)
            .filter(|m| {
                matches!(
                    m.status,
                    MissionStatus::Traveling | MissionStatus::Operating
                )
            })
            .flat_map(|m| {
                m.members
                    .values()
                    .filter(|member| {
                        member.status == CompanionStatus::Accepted
                            && encounter.fighters.contains_key(&member.person)
                            && matches!(member.directive, MissionObjective::Confine { .. })
                    })
                    .map(|member| (m.id, member.person))
            })
            .collect()
    }
    pub fn views(&self, store: &Store, actor: PersonId) -> Vec<MissionView> {
        knowledge(store, actor).map_or_else(Vec::new, |k| {
            k.missions.values().rev().take(64).cloned().collect()
        })
    }
}
/// A derived ID roster for a receipt pass; it never stores live member state.
pub struct ReceiptObservers {
    interested: BTreeSet<PersonId>,
}
impl Runtime {
    pub fn receipt_observers(&self, store: &Store) -> ReceiptObservers {
        ReceiptObservers {
            interested: missions(store)
                .flat_map(|(_, m)| m.members.keys().copied())
                .collect(),
        }
    }
}
impl ReceiptObservers {
    pub fn arrived(
        &self,
        runtime: &Runtime,
        actor: PersonId,
        area: AreaId,
        contacts: BTreeSet<PersonId>,
        facts: &impl PresenceFacts,
    ) -> MissionReceipt {
        let observers = contacts
            .intersection(&self.interested)
            .copied()
            .filter(|p| runtime.can_observe(*p, actor, facts))
            .collect();
        MissionReceipt::Arrived {
            actor,
            area,
            contacts,
            observers,
        }
    }
}
pub trait OrderHearingFacts: PresenceFacts {
    fn player(&self) -> PersonId;
    fn recruitment(&self, person: PersonId, mission: &Mission) -> RecruitmentInput;
}
impl Runtime {
    pub fn order_heard(
        &self,
        store: &Store,
        id: MissionId,
        leader: PersonId,
        facts: &impl OrderHearingFacts,
    ) -> Result<BTreeMap<PersonId, ServiceChoice>, Error> {
        let area = facts.area(leader).ok_or(Error::Unavailable)?;
        let m = mission(store, id).ok_or(Error::Unavailable)?;
        Ok(m.members
            .values()
            .filter(|p| {
                p.status == CompanionStatus::Accepted
                    && facts.area(p.person) == Some(area)
                    && facts.same_place(p.person, leader)
            })
            .map(|p| {
                (
                    p.person,
                    if p.person == facts.player() {
                        ServiceChoice::DeclaredAcceptance
                    } else {
                        ServiceChoice::Personal(facts.recruitment(p.person, m))
                    },
                )
            })
            .collect())
    }
    pub fn validate_accepted_leader(
        &self,
        store: &Store,
        id: MissionId,
        leader: PersonId,
        in_combat: bool,
    ) -> Result<(), Error> {
        if mission(store, id).is_none_or(|m| m.proposal.leader != leader) || in_combat {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

impl Runtime {
    /// The service observation scene is the first active actual encounter in
    /// stable encounter order that contains this observer.
    pub fn observation_encounter<'a>(
        &self,
        person: PersonId,
        encounters: impl IntoIterator<Item = &'a Encounter>,
    ) -> Option<&'a Encounter> {
        encounters
            .into_iter()
            .find(|encounter| encounter.active && encounter.fighters.contains_key(&person))
    }
}

impl Runtime {
    /// Prepare a tactical declaration against the supplied projected contact
    /// scene. Service owns authority, recipients and each companion's consent.
    pub fn prepare_combat_order(
        &self,
        store: &Store,
        actor: PersonId,
        id: MissionId,
        objective: &MissionObjective,
        command: &CommandFacts,
        facts: &impl OrderHearingFacts,
    ) -> Result<PreparedMission, Error> {
        self.validate_order(store, actor, id, objective, command)?;
        let leader = mission(store, id)
            .ok_or(Error::Unavailable)?
            .proposal
            .leader;
        let heard = self.order_heard(store, id, leader, facts)?;
        self.prepare_order(
            store,
            id,
            leader,
            objective,
            &OrderFacts {
                now: command.now,
                heard,
                assets: command.assets.clone(),
            },
            false,
        )
    }
}

//! Canonical lineage columns and metadata-only runtime.
use crate::*;
use game_core::*;
use game_ecs::{Batch, ReadSet, Schema, StableId, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The campaign founder's entity anchors its ordered incarnation history.
/// Current incarnation and succession waiting are derived from its final record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub incarnations: Vec<Incarnation>,
}
impl History {
    pub fn current(&self) -> Option<PersonId> {
        self.incarnations
            .last()
            .filter(|life| life.died_at.is_none())
            .map(|life| life.person)
    }
    pub fn waiting(&self) -> Option<Waiting> {
        let last = self.incarnations.last()?;
        Some(Waiting {
            predecessor: last.person,
            died_at: last.died_at?,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        let mut people = BTreeSet::new();
        if self.incarnations.is_empty() {
            return Err(configuration("empty lineage history"));
        }
        for (index, life) in self.incarnations.iter().enumerate() {
            if life.person.0 == 0
                || !people.insert(life.person)
                || life.died_at.is_some_and(|died| died < life.bound_at)
                || index + 1 < self.incarnations.len() && life.died_at.is_none()
                || index > 0
                    && self.incarnations[index - 1]
                        .died_at
                        .is_none_or(|died| life.bound_at <= died)
            {
                return Err(configuration("inconsistent lineage history"));
            }
        }
        Ok(())
    }
}

/// Content and the stable reference to History; entity state remains in Store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: MentorshipRules,
    pub history: StableId,
}

pub(crate) fn configuration(message: &str) -> Error {
    Error::InvalidConfiguration(message.into())
}

impl MentorshipRules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.lesson_duration.0 == 0
            || !(1..=INVOCATION_LEARNING_LIMIT).contains(&self.lesson_gain)
            || !(1..=INVOCATION_LEARNING_LIMIT).contains(&self.invocation_access_threshold)
        {
            return Err(configuration("invalid lineage mentorship rules"));
        }
        Ok(())
    }
}

impl Guardian {
    pub fn validate(&self) -> Result<(), Error> {
        if self.target.is_some_and(|person| person.0 == 0)
            || self.target.is_some() != self.sensed_location.is_some()
            || self.assessments.iter().any(|(person, assessment)| {
                person.0 == 0
                    || [
                        assessment.independence,
                        assessment.competence,
                        assessment.judgment,
                        assessment.risks,
                    ]
                    .into_iter()
                    .any(|value| value > 1_000)
                    || assessment.ready
                        && (assessment.independence < 100
                            || assessment.competence < 100
                            || assessment.judgment < 75
                            || assessment.observed_stage == Some(LifeStage::Infant))
            })
            || self
                .provision
                .as_ref()
                .is_some_and(|provision| !provision.delivery.valid())
            || self.requested_delivery.as_ref().is_some_and(|request| {
                !request.delivery.valid()
                    || request
                        .cargo
                        .iter()
                        .any(|(id, quantity)| id.0 == 0 || *quantity == 0)
            })
            || self
                .assignment
                .as_ref()
                .is_some_and(|assignment| match assignment.request {
                    GuardianRequest::Move { destination } => destination.0 == 0,
                    GuardianRequest::Work { site } => site.0 == 0,
                    GuardianRequest::Supply {
                        source, quantity, ..
                    } => source.0 == 0 || quantity.0 == 0,
                    GuardianRequest::Rest => false,
                })
        {
            return Err(configuration("invalid lineage guardian"));
        }
        Ok(())
    }
}

pub(crate) fn register_canonical_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<History>("lineage/history", History::validate)?;
    schema.register::<Guardian>("lineage/guardian", Guardian::validate)
}
pub fn guardian(store: &Store, actor: PersonId) -> Option<&Guardian> {
    store.get(StableId(actor.0))
}
pub fn guardians(store: &Store) -> impl Iterator<Item = (PersonId, &Guardian)> {
    store
        .iter::<Guardian>()
        .map(|(id, row)| (PersonId(id.0), row))
}

impl Runtime {
    /// Startup attaches History to the existing campaign founder entity.
    /// No rows or allocation metadata are changed until every row is valid.
    pub fn initialize(
        store: &mut Store,
        history_id: StableId,
        rules: MentorshipRules,
        history: History,
        founders: impl IntoIterator<Item = (PersonId, Guardian)>,
    ) -> Result<Self, Error> {
        rules.validate()?;
        history.validate()?;
        if history_id.0 == 0
            || !store.contains(history_id)
            || history.incarnations.first().map(|life| life.person.0) != Some(history_id.0)
            || store.iter::<History>().next().is_some()
            || guardians(store).next().is_some()
            || history
                .incarnations
                .iter()
                .any(|life| !store.contains(StableId(life.person.0)))
        {
            return Err(configuration(
                "lineage initialization requires fresh history on the campaign founder",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut batch = Batch::new();
        for (actor, row) in founders {
            row.validate()?;
            if !store.contains(StableId(actor.0))
                || !seen.insert(actor)
                || row.target != history.current()
                || row.assessments.keys().any(|person| {
                    !history
                        .incarnations
                        .iter()
                        .any(|life| life.person == *person)
                })
            {
                return Err(configuration("inconsistent founding guardian"));
            }
            batch.insert(StableId(actor.0), row);
        }
        batch.insert(history_id, history);
        store.apply_batch(batch)?;
        Ok(Self {
            rules,
            history: history_id,
        })
    }
    pub fn history<'a>(&self, store: &'a Store) -> Result<&'a History, Error> {
        let history = store
            .get::<History>(self.history)
            .ok_or(Error::Unavailable)?;
        if history.incarnations.first().map(|life| life.person.0) != Some(self.history.0) {
            return Err(Error::Unavailable);
        }
        Ok(history)
    }
    pub fn current(&self, store: &Store) -> Option<PersonId> {
        self.history(store).ok()?.current()
    }
    pub fn waiting(&self, store: &Store) -> Option<Waiting> {
        self.history(store).ok()?.waiting()
    }
    /// Waiting spectators retain the full guardian view; a living incarnation
    /// sees guardians in the supplied factual local roster.
    pub fn visible_guardians<'a>(
        &self,
        store: &'a Store,
        local: &BTreeSet<PersonId>,
    ) -> Vec<(PersonId, &'a Guardian)> {
        let waiting = self.current(store).is_none();
        guardians(store)
            .filter(|(id, _)| waiting || local.contains(id))
            .collect()
    }
    pub fn invocation_ready(&self, store: &Store, person: PersonId) -> bool {
        self.current(store) == Some(person)
            && invocation_learning(store, person) >= self.rules.invocation_access_threshold
    }
    pub fn requested_delivery_quantity(
        &self,
        store: &Store,
        actor: PersonId,
        good: Good,
        source: Option<InventoryId>,
    ) -> Option<Quantity> {
        guardian(store, actor)?.requested_delivery_quantity(good, source)
    }
    pub fn guardian_service_due(&self, store: &Store, facts: GuardianServiceFacts) -> bool {
        crate::service::guardian_service_due(
            self.current(store),
            guardian(store, facts.actor),
            facts,
        )
    }
    pub fn validate(&self, store: &Store, now: SimTime) -> Result<(), Error> {
        let fail = || Error::InvalidSave("inconsistent canonical lineage".into());
        self.rules.validate().map_err(|_| fail())?;
        let history = self.history(store).map_err(|_| fail())?;
        history.validate().map_err(|_| fail())?;
        if store
            .iter::<History>()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
            != vec![self.history]
            || history
                .incarnations
                .iter()
                .any(|life| life.bound_at > now || life.died_at.is_some_and(|died| died > now))
        {
            return Err(fail());
        }
        let people: BTreeSet<_> = history
            .incarnations
            .iter()
            .map(|life| life.person)
            .collect();
        for (_, row) in guardians(store) {
            row.validate().map_err(|_| fail())?;
            if row.target != history.current()
                || row.sensed_at > now
                || row.assessments.iter().any(|(actor, assessment)| {
                    !people.contains(actor) || assessment.observed_at > now
                })
                || row.assignment.as_ref().is_some_and(|assignment| {
                    assignment.accepted_at > now || history.current().is_none()
                })
                || row.provision.as_ref().is_some_and(|provision| {
                    provision.observed_at > now
                        || history.current().is_none_or(|person| {
                            provision.delivery.destination
                                != game_logistics::Destination::Contact(person)
                        })
                })
                || row.requested_delivery.as_ref().is_some_and(|request| {
                    request.accepted_at > now
                        || history.current().is_none_or(|person| {
                            request.delivery.destination
                                != game_logistics::Destination::Contact(person)
                        })
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }

    // Temporary selected rows reuse the established policies. This is neither
    // persisted nor retained in Runtime; the shared store is the only authority.
    pub(crate) fn project(&self, store: &Store, actors: &[PersonId]) -> Result<Projection, Error> {
        self.rules.validate()?;
        let mut reads = ReadSet::new();
        let history = reads
            .read::<History>(store, self.history)
            .ok_or(Error::Unavailable)?;
        history.validate()?;
        if history.incarnations.first().map(|life| life.person.0) != Some(self.history.0) {
            return Err(Error::Unavailable);
        }
        let mut selected = BTreeMap::new();
        for actor in actors {
            let row = reads
                .read::<Guardian>(store, StableId(actor.0))
                .ok_or(Error::Unavailable)?;
            row.validate()?;
            selected.insert(*actor, row.clone());
        }
        Ok(Projection {
            reads,
            world: World {
                incarnations: history.incarnations.clone(),
                waiting: history.waiting(),
                guardians: selected,
            },
        })
    }
    pub(crate) fn project_all(&self, store: &Store) -> Result<Projection, Error> {
        let mut membership = ReadSet::new();
        let actors: Vec<_> = membership
            .membership::<Guardian>(store, |_, _| true)
            .into_iter()
            .map(|id| PersonId(id.0))
            .collect();
        let mut projection = self.project(store, &actors)?;
        projection.reads.extend(membership);
        Ok(projection)
    }
}
pub(crate) struct Projection {
    pub(crate) reads: ReadSet,
    pub(crate) world: World,
}

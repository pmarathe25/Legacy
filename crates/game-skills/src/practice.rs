//! Sparse learning transactions over shared personal skill components.
use crate::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Change {
    actor: PersonId,
    before: Option<Skills>,
    after: Skills,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ActorLearningWitness {
    actor: PersonId,
    before: Option<Skills>,
    after: Option<Skills>,
}

/// Frozen learning effects; unrelated people's practice remains independent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedPractice {
    rules: Rules,
    changes: Vec<Change>,
    openings: Vec<(PersonId, Option<Skills>)>,
    requested: Vec<PracticeDelta>,
    actor_learning: Option<Vec<ActorLearningWitness>>,
}
/// Retained owner authorization for the final composed component batch.
pub struct StagedPractice {
    rules: Rules,
    reads: ReadGuard,
    requested: Vec<PracticeDelta>,
    actor_learning: Option<Vec<ActorLearningWitness>>,
}
#[derive(Clone, Debug)]
pub struct PreparedNewborn {
    rules: Rules,
    child: PersonId,
    reads: ReadSet,
}
impl Runtime {
    pub fn prepare_newborn(
        &self,
        store: &Store,
        child: PersonId,
    ) -> Result<PreparedNewborn, Error> {
        self.rules.validate()?;
        if child.0 == 0 {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        if reads.entity(store, StableId(child.0))
            || reads.read::<Skills>(store, StableId(child.0)).is_some()
        {
            return Err(Error::Unavailable);
        }
        Ok(PreparedNewborn {
            rules: self.rules.clone(),
            child,
            reads,
        })
    }
}
impl PreparedNewborn {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedPractice, Error> {
        if self.rules != runtime.rules || batch.touches::<Skills>(StableId(self.child.0)) {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        admitted.protect::<Skills>(StableId(self.child.0));
        batch.insert(StableId(self.child.0), Skills::default());
        Ok(StagedPractice {
            rules: self.rules,
            reads: self.reads.bind(store, batch, &admitted)?,
            requested: Vec::new(),
            actor_learning: None,
        })
    }
}
impl Runtime {
    pub fn prepare_actor_learning(
        &self,
        store: &Store,
        evaluation: &EvaluatedLearning,
        actor: PersonId,
    ) -> Result<PreparedPractice, Error> {
        self.prepare_actors_learning(store, evaluation, [actor])
    }
    pub fn prepare_actors_learning(
        &self,
        store: &Store,
        evaluation: &EvaluatedLearning,
        actors: impl IntoIterator<Item = PersonId>,
    ) -> Result<PreparedPractice, Error> {
        evaluation.validate(self, store)?;
        let actors: BTreeSet<_> = actors.into_iter().collect();
        if actors
            .iter()
            .any(|actor| !store.contains(StableId(actor.0)))
        {
            return Err(Error::Unavailable);
        }
        let mut prepared = self.prepare_practice(
            store,
            evaluation
                .deltas
                .iter()
                .filter(|delta| actors.contains(&delta.actor))
                .cloned(),
        )?;
        let mut witnesses = Vec::new();
        for actor in actors {
            let before = skills(store, actor).cloned();
            let after = prepared
                .changes
                .iter()
                .find(|change| change.actor == actor)
                .map_or_else(|| before.clone(), |change| Some(change.after.clone()));
            if !prepared
                .openings
                .iter()
                .any(|(selected, _)| *selected == actor)
            {
                prepared.openings.push((actor, before.clone()));
            }
            witnesses.push(ActorLearningWitness {
                actor,
                before,
                after,
            });
        }
        for teacher in evaluation.teacher_caps.keys() {
            if !prepared.openings.iter().any(|(actor, _)| actor == teacher) {
                prepared
                    .openings
                    .push((*teacher, skills(store, *teacher).cloned()));
            }
        }
        prepared.openings.sort_by_key(|(actor, _)| *actor);
        prepared.actor_learning = Some(witnesses);
        Ok(prepared)
    }
    pub fn prepare_learning(
        &self,
        store: &Store,
        evaluation: EvaluatedLearning,
    ) -> Result<PreparedPractice, Error> {
        evaluation.validate(self, store)?;
        let mut prepared = self.prepare_practice(store, evaluation.deltas)?;
        for teacher in evaluation.teacher_caps.into_keys() {
            if !prepared.openings.iter().any(|(actor, _)| *actor == teacher) {
                prepared
                    .openings
                    .push((teacher, skills(store, teacher).cloned()));
            }
        }
        prepared.openings.sort_by_key(|(actor, _)| *actor);
        Ok(prepared)
    }

    pub fn prepare_event(
        &self,
        store: &Store,
        event: LearningEvent,
        now: SimTime,
    ) -> Result<PreparedPractice, Error> {
        self.prepare_events(store, [event], now)
    }
    pub fn prepare_events(
        &self,
        store: &Store,
        events: impl IntoIterator<Item = LearningEvent>,
        now: SimTime,
    ) -> Result<PreparedPractice, Error> {
        self.rules.validate()?;
        let mut changes = BTreeMap::<PersonId, Change>::new();
        for event in events {
            let (actors, newborn) = match event {
                LearningEvent::Born { child } => (vec![child], true),
                LearningEvent::AgreementActivated { accepted } => (accepted, false),
                LearningEvent::AgreementAmended { proposer } => (vec![proposer], false),
            };
            for actor in actors {
                if !store.contains(StableId(actor.0)) {
                    return Err(Error::Unavailable);
                }
                let change = changes.entry(actor).or_insert_with(|| {
                    let before = skills(store, actor).cloned();
                    Change {
                        actor,
                        after: before.clone().unwrap_or_default(),
                        before,
                    }
                });
                change.after.validate_progress(&self.rules)?;
                if newborn {
                    change.after = Skills::default();
                    continue;
                }
                let tick = now.0 / MINUTE.0;
                if change
                    .after
                    .semantic_ticks
                    .get(&Skill::Negotiation)
                    .is_some_and(|previous| *previous >= tick)
                {
                    continue;
                }
                change.after.semantic_ticks.insert(Skill::Negotiation, tick);
                change.after.credit(
                    Skill::Negotiation,
                    self.rules.action_practice_ms,
                    None,
                    &self.rules,
                );
            }
        }
        let openings = changes
            .values()
            .map(|change| (change.actor, change.before.clone()))
            .collect();
        Ok(PreparedPractice {
            rules: self.rules.clone(),
            openings,
            changes: changes
                .into_values()
                .filter(|change| change.before.as_ref() != Some(&change.after))
                .collect(),
            requested: Vec::new(),
            actor_learning: None,
        })
    }

    pub fn prepare_practice(
        &self,
        store: &Store,
        deltas: impl IntoIterator<Item = PracticeDelta>,
    ) -> Result<PreparedPractice, Error> {
        self.rules.validate()?;
        let mut changes = BTreeMap::<PersonId, Change>::new();
        let deltas: Vec<_> = deltas.into_iter().collect();
        let requested = deltas.clone();
        for delta in deltas {
            if delta.milliseconds == 0 {
                continue;
            }
            if !store.contains(StableId(delta.actor.0)) {
                return Err(Error::Unavailable);
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = changes.entry(delta.actor) {
                let before = skills(store, delta.actor).cloned();
                let after = before.clone().unwrap_or_default();
                after.validate_progress(&self.rules)?;
                entry.insert(Change {
                    actor: delta.actor,
                    before,
                    after,
                });
            }
            changes
                .get_mut(&delta.actor)
                .expect("admitted practice actor")
                .after
                .credit(
                    delta.skill,
                    delta.milliseconds,
                    delta.mastery_cap,
                    &self.rules,
                );
        }
        Ok(PreparedPractice {
            rules: self.rules.clone(),
            requested,
            actor_learning: None,
            openings: changes
                .values()
                .map(|change| (change.actor, change.before.clone()))
                .collect(),
            changes: changes
                .into_values()
                .filter(|change| match &change.before {
                    Some(before) => before != &change.after,
                    None => change.after != Skills::default(),
                })
                .collect(),
        })
    }

    /// Earliest level transition among actual activity-owner practice effects.
    pub fn practice_boundary(
        &self,
        store: &Store,
        from: SimTime,
        limit: SimTime,
        deltas: &[PracticeDelta],
    ) -> Result<SimTime, Error> {
        self.practice_boundary_with(from, limit, deltas, |actor, skill| {
            self.practice(store, actor, skill)
        })
    }

    /// Journal-native variant using an owner's borrowed canonical-row lookup.
    pub fn practice_boundary_with(
        &self,
        from: SimTime,
        limit: SimTime,
        deltas: &[PracticeDelta],
        mut practice: impl FnMut(PersonId, Skill) -> u64,
    ) -> Result<SimTime, Error> {
        self.rules.validate()?;
        if limit < from {
            return Err(Error::Unavailable);
        }
        let mut next = limit;
        for delta in deltas {
            let current = practice(delta.actor, delta.skill);
            let level = current / self.rules.per_level.0;
            if level >= u64::from(self.rules.max_level) {
                continue;
            }
            let threshold = (level + 1)
                .checked_mul(self.rules.per_level.0)
                .ok_or(Error::Overflow)?;
            if delta.mastery_cap.is_some_and(|cap| cap < threshold) {
                continue;
            }
            let remaining = threshold.saturating_sub(current);
            if remaining <= delta.milliseconds {
                next = next.min(SimTime(
                    from.0.checked_add(remaining).ok_or(Error::Overflow)?,
                ));
            }
        }
        Ok(next)
    }
}
impl PreparedPractice {
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if self.rules != runtime.rules {
            return Err(Error::Unavailable);
        }
        for (actor, opening) in &self.openings {
            if !store.contains(StableId(actor.0)) || skills(store, *actor) != opening.as_ref() {
                return Err(Error::Unavailable);
            }
        }
        for change in &self.changes {
            change.after.validate_progress(&self.rules)?;
        }
        Ok(())
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let staged = self.stage_into(runtime, store, &mut batch)?;
        staged.validate(runtime, store, &batch)?;
        store.apply_batch(batch)
    }

    /// Bind immediately after these writes. Later owners may append unrelated
    /// rows; the final guard rejects any alteration of this owner's projection.
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StagedPractice, Error> {
        self.validate(runtime, store)?;
        let mut reads = ReadSet::new();
        for (actor, opening) in self.openings {
            let id = StableId(actor.0);
            reads.entity(store, id);
            reads.expect_row(id, opening);
            if batch.touches::<Skills>(id) {
                return Err(Error::Unavailable);
            }
        }
        let mut admitted = ReadFootprint::new();
        for change in self.changes {
            admitted.protect::<Skills>(StableId(change.actor.0));
            batch.insert(StableId(change.actor.0), change.after);
        }
        Ok(StagedPractice {
            rules: self.rules,
            reads: reads.bind(store, batch, &admitted)?,
            requested: self.requested,
            actor_learning: self.actor_learning,
        })
    }
}

impl StagedPractice {
    pub fn row_guard(&self) -> &ReadGuard {
        &self.reads
    }
    pub(crate) fn applied_learning_actors(&self) -> Result<Vec<PersonId>, Error> {
        self.actor_learning
            .as_ref()
            .map(|rows| rows.iter().map(|row| row.actor).collect())
            .ok_or(Error::Unavailable)
    }
    pub(crate) fn projected_actor_learning(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
        actor: PersonId,
        expected: &[PracticeDelta],
        teacher_cap: Option<u64>,
    ) -> Result<u64, Error> {
        self.validate(runtime, store, batch)?;
        let witness = self
            .actor_learning
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.actor == actor))
            .ok_or(Error::Unavailable)?;
        let requested = self
            .requested
            .iter()
            .filter(|delta| delta.actor == actor)
            .cloned()
            .collect::<Vec<_>>();
        if requested != expected
            || batch.projected::<Skills>(store, StableId(actor.0))? != witness.after.as_ref()
            || teacher_cap.is_some_and(|cap| {
                witness
                    .before
                    .as_ref()
                    .map_or(0, |row| row.practice(Skill::Craft))
                    != cap
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(witness
            .after
            .as_ref()
            .map_or(0, |row| row.practice(Skill::Craft)))
    }
    pub fn into_guard(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<ReadGuard, Error> {
        self.validate(runtime, store, batch)?;
        Ok(self.reads)
    }
    pub fn matches(&self, delta: &PracticeDelta) -> bool {
        self.requested.as_slice() == std::slice::from_ref(delta)
    }
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.reads.validate(store, batch)
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store, batch: &Batch) -> Result<(), Error> {
        runtime.rules.validate()?;
        if self.rules != runtime.rules {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store, batch)
    }
}

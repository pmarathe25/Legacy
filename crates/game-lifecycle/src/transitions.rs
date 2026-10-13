//! Prepared lifecycle changes own their guards and effects. Callers provide
//! current physical/choice facts and IDs, never a world reference or a verdict.
use super::*;
use game_core::Location;
use game_ecs::{Batch, StableId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub pregnancy: PregnancyId,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyFacts {
    pub actor: PersonId,
    pub alive: bool,
    pub health: u64,
    pub position: Location,
    pub action_restricted: bool,
    pub attempt: Option<Attempt>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Consent {
    pub initiator: bool,
    pub partner: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Refused {
        initiator: PersonId,
        partner: PersonId,
    },
    AttemptStarted {
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
        completes: SimTime,
    },
    Conceived {
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
        due: SimTime,
    },
    PregnancyEnded(PregnancyEndedEffect),
    Developed(AgeEffect),
    Death {
        actor: PersonId,
    },
    Born {
        pregnancy: PregnancyId,
        child: PersonId,
        parents: [PersonId; 2],
        position: Location,
        next_age: SimTime,
    },
}
#[derive(Clone, Debug)]
enum Edit {
    Pregnancy {
        before: Option<Pregnancy>,
        after: Pregnancy,
    },
    Life {
        before: Option<Life>,
        after: Life,
    },
}
#[derive(Clone, Debug)]
enum ReadGuard {
    Life {
        actor: PersonId,
        before: Option<Life>,
    },
    Pregnancy {
        id: PregnancyId,
        before: Option<Pregnancy>,
    },
    ActivePregnancies {
        gestator: PersonId,
        before: Vec<Pregnancy>,
    },
}
/// Current chosen conception or gestation rows for this gestator, in stable ID
/// order. Completed historical pregnancies do not expand a factual scene.
pub fn active_pregnancies(store: &Store, gestator: PersonId) -> Vec<Pregnancy> {
    let mut rows: Vec<_> = all_pregnancies(store)
        .filter(|(_, pregnancy)| {
            pregnancy.gestator == gestator
                && matches!(
                    pregnancy.state,
                    PregnancyState::Conceiving { .. } | PregnancyState::Gestating { .. }
                )
        })
        .map(|(_, row)| row.clone())
        .collect();
    rows.sort_by_key(|row| row.id);
    rows
}
#[derive(Clone, Debug)]
pub struct PreparedLifecycle {
    factual_reads: game_ecs::ReadSet,
    rules: Rules,
    guards: Vec<ReadGuard>,
    edits: Vec<Edit>,
    effects: Vec<Effect>,
}
impl PreparedLifecycle {
    /// A later interruption observes this owner-prepared final pregnancy row.
    /// Only a previously conceiving row changed to gestation or an ending is
    /// protected from a second conception cancellation in the same batch.
    pub fn is_completed_pregnancy(&self, id: PregnancyId) -> bool {
        self.edits.iter().any(|edit| matches!(edit,
            Edit::Pregnancy { before: Some(before), after }
            if before.id == id
                && matches!(before.state, PregnancyState::Conceiving { .. })
                && matches!(after.state, PregnancyState::Gestating { .. } | PregnancyState::Ended)
        ))
    }
    pub fn with_factual_reads(mut self, reads: game_ecs::ReadSet) -> Self {
        self.factual_reads.extend(reads);
        self
    }
    /// Receive an owner's interpreted reproductive role into the unpublished Life.
    pub fn receive_reproduction(mut self, role: Reproduction) -> Result<Self, Error> {
        let [Effect::Born { child, .. }] = self.effects.as_slice() else {
            return Err(Error::Unavailable);
        };
        let child = *child;
        let Some(Edit::Life {
            before: None,
            after,
        }) = self
            .edits
            .iter_mut()
            .find(|edit| matches!(edit, Edit::Life { after, .. } if after.id == child))
        else {
            return Err(Error::Unavailable);
        };
        after.reproduction = role;
        after.validate()?;
        Ok(self)
    }
    pub fn newborn_life(&self) -> Option<&Life> {
        let [Effect::Born { child, .. }] = self.effects.as_slice() else {
            return None;
        };
        self.edits.iter().find_map(|edit| match edit {
            Edit::Life {
                before: None,
                after,
            } if after.id == *child => Some(after),
            _ => None,
        })
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        self.factual_reads.validate(store)?;
        if runtime.rules != self.rules {
            return Err(Error::Unavailable);
        }
        for guard in &self.guards {
            let valid = match guard {
                ReadGuard::Life { actor, before } => life(store, *actor) == before.as_ref(),
                ReadGuard::Pregnancy { id, before } => pregnancy(store, *id) == before.as_ref(),
                ReadGuard::ActivePregnancies { gestator, before } => {
                    active_pregnancies(store, *gestator) == *before
                }
            };
            if !valid {
                return Err(Error::Unavailable);
            }
        }
        for edit in &self.edits {
            let valid = match edit {
                Edit::Pregnancy { before, after } => pregnancy(store, after.id) == before.as_ref(),
                Edit::Life { before, after } => life(store, after.id) == before.as_ref(),
            };
            if !valid {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<LifecycleFinalization, Error> {
        self.stage_into_with_factual_writes(runtime, store, batch, game_ecs::ReadFootprint::new())
    }
    /// Freeze deliberately changed foreign facts after their owning receivers
    /// have staged. Their independent guards remain with the publication host.
    pub fn stage_into_with_factual_writes(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
        factual_writes: game_ecs::ReadFootprint,
    ) -> Result<LifecycleFinalization, Error> {
        self.validate(runtime, store)?;
        let mut reads = self.factual_reads;
        for guard in &self.guards {
            match guard {
                ReadGuard::Life { actor, before } => {
                    reads.expect_row(StableId(actor.0), before.clone())
                }
                ReadGuard::Pregnancy { id, before } => {
                    reads.expect_row(StableId(id.0), before.clone())
                }
                ReadGuard::ActivePregnancies { gestator, .. } => {
                    let gestator = *gestator;
                    reads.membership::<Pregnancy>(store, move |_, row| {
                        row.gestator == gestator
                            && matches!(
                                row.state,
                                PregnancyState::Conceiving { .. }
                                    | PregnancyState::Gestating { .. }
                            )
                    });
                }
            }
        }
        let mut admitted = factual_writes;
        for edit in self.edits {
            match edit {
                Edit::Pregnancy { before, after } => {
                    reads.expect_row(StableId(after.id.0), before);
                    admitted.protect::<Pregnancy>(StableId(after.id.0));
                    batch.insert(StableId(after.id.0), after);
                }
                Edit::Life { before, after } => {
                    reads.expect_row(StableId(after.id.0), before);
                    admitted.protect::<Life>(StableId(after.id.0));
                    batch.insert(StableId(after.id.0), after);
                }
            }
        }
        let guard = reads.bind(store, batch, &admitted)?;
        Ok(LifecycleFinalization {
            effects: self.effects,
            guard,
        })
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, &mut batch)?;
        finish.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
    fn noop(rules: &Rules) -> Self {
        Self {
            factual_reads: game_ecs::ReadSet::new(),
            rules: rules.clone(),
            guards: Vec::new(),
            edits: Vec::new(),
            effects: Vec::new(),
        }
    }
}
pub struct LifecycleFinalization {
    guard: game_ecs::ReadGuard,
    effects: Vec<Effect>,
}
impl LifecycleFinalization {
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.guard.validate(store, batch)
    }
    pub fn finish(self) -> Vec<Effect> {
        self.effects
    }
}
#[derive(Clone, Debug)]
pub struct InterruptionRequest {
    pub actor: PersonId,
    pub died: bool,
    pub affected: Vec<PregnancyId>,
}
impl InterruptionRequest {
    /// A successfully completed attempt is not an interrupted pregnancy.
    pub fn for_attempt(
        actor: PersonId,
        pregnancy: Option<PregnancyId>,
        completed: Option<&PreparedLifecycle>,
    ) -> Self {
        let affected = pregnancy
            .filter(|pregnancy| {
                !completed.is_some_and(|plan| plan.is_completed_pregnancy(*pregnancy))
            })
            .into_iter()
            .collect();
        Self {
            actor,
            died: false,
            affected,
        }
    }
}

pub struct PreparedLifecycleInterruptions {
    change: PreparedLifecycle,
    groups: Vec<(PersonId, Vec<Effect>)>,
}
impl PreparedLifecycleInterruptions {
    pub fn effects(&self) -> impl Iterator<Item = (PersonId, &[Effect])> {
        self.groups
            .iter()
            .map(|(actor, effects)| (*actor, effects.as_slice()))
    }
    pub fn validate(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        self.change.validate(runtime, store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<LifecycleInterruptionFinalization, Error> {
        let change = self.change.stage_into(runtime, store, batch)?;
        Ok(LifecycleInterruptionFinalization {
            change,
            groups: self.groups,
        })
    }
}
pub struct LifecycleInterruptionFinalization {
    change: LifecycleFinalization,
    groups: Vec<(PersonId, Vec<Effect>)>,
}
impl LifecycleInterruptionFinalization {
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.change.validate(store, batch)
    }
    pub fn finish(self) -> Vec<(PersonId, Vec<Effect>)> {
        self.groups
    }
}
impl Runtime {
    /// Mortality ends chosen gestation. A current conception is separately
    /// cancelled by the ordinary activity interruption receiver.
    pub fn prepare_gestation_death(&self, store: &Store, actor: PersonId) -> PreparedLifecycle {
        let selected = active_pregnancies(store, actor)
            .into_iter()
            .filter(|pregnancy| matches!(pregnancy.state, PregnancyState::Gestating { .. }))
            .map(|pregnancy| pregnancy.id);
        let mut reads = game_ecs::ReadSet::new();
        reads.membership::<Pregnancy>(store, move |_, pregnancy| {
            pregnancy.gestator == actor
                && matches!(pregnancy.state, PregnancyState::Gestating { .. })
        });
        self.prepare_interruption(store, actor, true, selected)
            .with_factual_reads(reads)
    }
    pub fn prepare_interruptions(
        &self,
        store: &Store,
        requests: &[InterruptionRequest],
    ) -> PreparedLifecycleInterruptions {
        let mut change = PreparedLifecycle::noop(&self.rules);
        let mut groups = Vec::new();
        let mut ended = BTreeSet::new();
        for request in requests {
            let one = self.prepare_interruption(
                store,
                request.actor,
                request.died,
                request.affected.iter().copied(),
            );
            change.guards.extend(one.guards);
            let mut effects = Vec::new();
            for edit in one.edits {
                if let Edit::Pregnancy { after, .. } = &edit
                    && ended.insert(after.id)
                {
                    change.edits.push(edit);
                }
            }
            for effect in one.effects {
                if let Effect::PregnancyEnded(ending) = &effect {
                    if change.effects.iter().any(|effect|matches!(effect,Effect::PregnancyEnded(old) if old.pregnancy==ending.pregnancy)){continue;}
                    change.effects.push(effect.clone());
                    effects.push(effect);
                }
            }
            groups.push((request.actor, effects));
        }
        change.effects.clear();
        PreparedLifecycleInterruptions { change, groups }
    }
}
/// Typed lifecycle deadlines; adapters translate their scheduling storage into
/// these requests without implementing biological time rules themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deadline {
    Conception(PregnancyId),
    Age(PersonId),
    Birth(PregnancyId),
}
impl Runtime {
    pub fn preflight_deadlines(
        &self,
        store: &Store,
        at: SimTime,
        deadlines: impl IntoIterator<Item = Deadline>,
    ) -> Result<(), Error> {
        if !self.rules.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid lifecycle rules".into(),
            ));
        }
        // Reserve future life timestamps before any current boundary is published.
        at.0.checked_add(self.rules.lifespan.0)
            .ok_or(Error::Overflow)?;
        for deadline in deadlines {
            match deadline {
                Deadline::Conception(id) => {
                    if pregnancy(store, id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Conceiving { completes: at })
                    }) {
                        return Err(Error::Unavailable);
                    }
                    at.0.checked_add(self.rules.gestation.0)
                        .ok_or(Error::Overflow)?;
                }
                Deadline::Birth(id) => {
                    if pregnancy(store, id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Gestating { due: at })
                    }) {
                        return Err(Error::Unavailable);
                    }
                    at.0.checked_add(self.rules.infancy.0)
                        .ok_or(Error::Overflow)?;
                }
                Deadline::Age(id) => {
                    life(store, id).ok_or(Error::Unavailable)?.age_at(at)?;
                }
            }
        }
        Ok(())
    }
    pub fn prepare_start_conception(
        &self,
        store: &Store,
        now: SimTime,
        id: PregnancyId,
        bodies: [&BodyFacts; 2],
        consent: Consent,
    ) -> Result<PreparedLifecycle, Error> {
        let [first, second] = bodies;
        let a = life(store, first.actor).ok_or(Error::Unavailable)?;
        let b = life(store, second.actor).ok_or(Error::Unavailable)?;
        if first.actor == second.actor
            || !self.rules.valid()
            || !matches!(
                self.rules.stage_at(a.age_at(now)?),
                LifeStage::Adult | LifeStage::Elder
            )
            || !matches!(
                self.rules.stage_at(b.age_at(now)?),
                LifeStage::Adult | LifeStage::Elder
            )
            || !first.alive
            || first.health == 0
            || !second.alive
            || second.health == 0
            || first.action_restricted
            || second.action_restricted
            || !matches!(first.position, Location::AtArea(_))
            || first.position != second.position
        {
            return Err(Error::Unavailable);
        }
        let (gestator, partner) = reproductive_pair(
            (first.actor, a.reproduction),
            (second.actor, b.reproduction),
        )
        .ok_or(Error::Unavailable)?;
        let active = active_pregnancies(store, gestator);
        if !active.is_empty() {
            return Err(Error::Unavailable);
        }
        let guards = vec![
            ReadGuard::Life {
                actor: first.actor,
                before: Some(a.clone()),
            },
            ReadGuard::Life {
                actor: second.actor,
                before: Some(b.clone()),
            },
            ReadGuard::ActivePregnancies {
                gestator,
                before: active,
            },
        ];
        if !consent.initiator || !consent.partner {
            return Ok(PreparedLifecycle {
                factual_reads: game_ecs::ReadSet::new(),
                rules: self.rules.clone(),
                guards,
                edits: Vec::new(),
                effects: vec![Effect::Refused {
                    initiator: first.actor,
                    partner: second.actor,
                }],
            });
        }
        if id.0 == 0 || id.0 == gestator.0 || id.0 == partner.0 || pregnancy(store, id).is_some() {
            return Err(Error::Unavailable);
        }
        let completes = SimTime(
            now.0
                .checked_add(self.rules.conception_time.0)
                .ok_or(Error::Overflow)?,
        );
        completes
            .0
            .checked_add(self.rules.gestation.0)
            .ok_or(Error::Overflow)?;
        let after = Pregnancy {
            id,
            gestator,
            partner,
            state: PregnancyState::Conceiving { completes },
        };
        Ok(PreparedLifecycle {
            factual_reads: game_ecs::ReadSet::new(),
            rules: self.rules.clone(),
            guards,
            edits: vec![Edit::Pregnancy {
                before: None,
                after,
            }],
            effects: vec![Effect::AttemptStarted {
                pregnancy: id,
                parents: [gestator, partner],
                completes,
            }],
        })
    }
    pub fn prepare_complete_conception(
        &self,
        store: &Store,
        id: PregnancyId,
        now: SimTime,
        bodies: [&BodyFacts; 2],
    ) -> Result<PreparedLifecycle, Error> {
        let before = pregnancy(store, id).ok_or(Error::Unavailable)?;
        if before.state != (PregnancyState::Conceiving { completes: now }) {
            return Err(Error::Unavailable);
        }
        let pair = [before.gestator, before.partner];
        if bodies[0].actor != pair[0] || bodies[1].actor != pair[1] {
            return Err(Error::Unavailable);
        }
        let valid = bodies.iter().all(|body| {
            body.alive
                && body.health > 0
                && body.attempt
                    == Some(Attempt {
                        pregnancy: id,
                        completes: now,
                    })
        }) && bodies[0].position == bodies[1].position;
        let mut after = before.clone();
        let effect = if valid {
            let due = SimTime(
                now.0
                    .checked_add(self.rules.gestation.0)
                    .ok_or(Error::Overflow)?,
            );
            after.state = PregnancyState::Gestating { due };
            Effect::Conceived {
                pregnancy: id,
                parents: pair,
                due,
            }
        } else {
            after.state = PregnancyState::Ended;
            Effect::PregnancyEnded(PregnancyEndedEffect {
                pregnancy: id,
                parents: pair,
                was_conceiving: true,
            })
        };
        Ok(PreparedLifecycle {
            factual_reads: game_ecs::ReadSet::new(),
            rules: self.rules.clone(),
            guards: pair
                .into_iter()
                .map(|actor| ReadGuard::Life {
                    actor,
                    before: life(store, actor).cloned(),
                })
                .collect(),
            edits: vec![Edit::Pregnancy {
                before: Some(before.clone()),
                after,
            }],
            effects: vec![effect],
        })
    }
    pub fn prepare_interruption(
        &self,
        store: &Store,
        actor: PersonId,
        died: bool,
        affected: impl IntoIterator<Item = PregnancyId>,
    ) -> PreparedLifecycle {
        let mut prepared = PreparedLifecycle::noop(&self.rules);
        for id in affected.into_iter().collect::<BTreeSet<_>>() {
            prepared.guards.push(ReadGuard::Pregnancy {
                id,
                before: pregnancy(store, id).cloned(),
            });
            let Some(before) = pregnancy(store, id) else {
                continue;
            };
            let was_conceiving = matches!(before.state, PregnancyState::Conceiving { .. });
            if was_conceiving && (before.gestator == actor || before.partner == actor)
                || died
                    && before.gestator == actor
                    && matches!(before.state, PregnancyState::Gestating { .. })
            {
                let mut after = before.clone();
                after.state = PregnancyState::Ended;
                prepared.edits.push(Edit::Pregnancy {
                    before: Some(before.clone()),
                    after,
                });
                prepared
                    .effects
                    .push(Effect::PregnancyEnded(PregnancyEndedEffect {
                        pregnancy: id,
                        parents: [before.gestator, before.partner],
                        was_conceiving,
                    }));
            }
        }
        prepared
    }
    pub fn prepare_age(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        body_alive: bool,
        lifespan_loss: u64,
        aging_enabled: bool,
    ) -> Result<PreparedLifecycle, Error> {
        let life = life(store, actor).ok_or(Error::Unavailable)?;
        let guards = vec![ReadGuard::Life {
            actor,
            before: Some(life.clone()),
        }];
        if !body_alive || !aging_enabled {
            let mut prepared = PreparedLifecycle::noop(&self.rules);
            prepared.guards = guards;
            return Ok(prepared);
        }
        let effect = self.evaluate_age(store, actor, now, lifespan_loss, true)?;
        if effect.died {
            return Ok(PreparedLifecycle {
                factual_reads: game_ecs::ReadSet::new(),
                rules: self.rules.clone(),
                guards,
                edits: Vec::new(),
                effects: vec![Effect::Death { actor }],
            });
        }
        let mut after = life.clone();
        after.stage = effect.stage;
        Ok(PreparedLifecycle {
            factual_reads: game_ecs::ReadSet::new(),
            rules: self.rules.clone(),
            guards,
            edits: vec![Edit::Life {
                before: Some(life.clone()),
                after,
            }],
            effects: vec![Effect::Developed(effect)],
        })
    }
    pub fn prepare_birth(
        &self,
        store: &Store,
        id: PregnancyId,
        now: SimTime,
        child: PersonId,
        gestator: &BodyFacts,
        seed: u64,
    ) -> Result<PreparedLifecycle, Error> {
        let before = pregnancy(store, id).ok_or(Error::Unavailable)?;
        let mut guards = vec![ReadGuard::Pregnancy {
            id,
            before: Some(before.clone()),
        }];
        if matches!(
            before.state,
            PregnancyState::Ended | PregnancyState::Born(_)
        ) {
            let mut prepared = PreparedLifecycle::noop(&self.rules);
            prepared.guards = guards;
            return Ok(prepared);
        }
        if before.state != (PregnancyState::Gestating { due: now })
            || gestator.actor != before.gestator
        {
            return Err(Error::Unavailable);
        }
        let parents = [before.gestator, before.partner];
        guards.extend(parents.into_iter().map(|actor| ReadGuard::Life {
            actor,
            before: life(store, actor).cloned(),
        }));
        let mut after = before.clone();
        if !gestator.alive {
            after.state = PregnancyState::Ended;
            return Ok(PreparedLifecycle {
                factual_reads: game_ecs::ReadSet::new(),
                rules: self.rules.clone(),
                guards,
                edits: vec![Edit::Pregnancy {
                    before: Some(before.clone()),
                    after,
                }],
                effects: vec![Effect::PregnancyEnded(PregnancyEndedEffect {
                    pregnancy: id,
                    parents,
                    was_conceiving: false,
                })],
            });
        }
        if child.0 == 0
            || child.0 == id.0
            || parents.contains(&child)
            || life(store, child).is_some()
        {
            return Err(Error::Unavailable);
        }
        let next_age = SimTime(
            now.0
                .checked_add(self.rules.infancy.0)
                .ok_or(Error::Overflow)?,
        );
        let life = Life {
            id: child,
            born: now,
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction: newborn_reproduction(seed, id),
            parents: parents.to_vec(),
            caregivers: BTreeSet::new(),
            preference: None,
        };
        after.state = PregnancyState::Born(child);
        Ok(PreparedLifecycle {
            factual_reads: game_ecs::ReadSet::new(),
            rules: self.rules.clone(),
            guards,
            edits: vec![
                Edit::Pregnancy {
                    before: Some(before.clone()),
                    after,
                },
                Edit::Life {
                    before: None,
                    after: life,
                },
            ],
            effects: vec![Effect::Born {
                pregnancy: id,
                child,
                parents,
                position: gestator.position.clone(),
                next_age,
            }],
        })
    }
}
pub fn newborn_reproduction(seed: u64, pregnancy: PregnancyId) -> Reproduction {
    let mut value = seed ^ pregnancy.0.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    if (value ^ (value >> 31)).is_multiple_of(2) {
        Reproduction::Gestates
    } else {
        Reproduction::SuppliesSeed
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationBody {
    pub body: BodyFacts,
    pub aging_enabled: bool,
    pub lifespan_loss: u64,
}
impl Runtime {
    /// Validate this FSM against explicit external body and scheduler facts.
    /// Physical activity storage and political identities remain outside it.
    pub fn validate_with_facts(
        &self,
        store: &Store,
        now: SimTime,
        bodies: &BTreeMap<PersonId, ValidationBody>,
        deadlines: &[(SimTime, Deadline)],
    ) -> Result<(), Error> {
        validate_runtime(self, store, now)?;
        let fail = || Error::InvalidSave("inconsistent lifecycle body or deadline facts".into());
        if all_lives(store)
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            .iter()
            .ne(bodies.keys())
        {
            return Err(fail());
        }
        let mut ages = BTreeSet::new();
        let mut births = BTreeSet::new();
        for (at, deadline) in deadlines {
            if *at <= now {
                return Err(fail());
            }
            match *deadline {
                Deadline::Age(actor) => {
                    let life = life(store, actor).ok_or_else(fail)?;
                    let facts = bodies.get(&actor).ok_or_else(fail)?;
                    let age = life.age_at(*at).map_err(|_| fail())?;
                    if ![
                        self.rules.infancy.0,
                        self.rules.adulthood.0,
                        self.rules.elderhood.0,
                        self.rules.effective_lifespan(facts.lifespan_loss),
                    ]
                    .contains(&age)
                        || !ages.insert(actor)
                    {
                        return Err(fail());
                    }
                }
                Deadline::Birth(id) => {
                    if pregnancy(store, id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Gestating { due: *at })
                    }) || !births.insert(id)
                    {
                        return Err(fail());
                    }
                }
                Deadline::Conception(id) => {
                    if pregnancy(store, id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Conceiving { completes: *at })
                    }) {
                        return Err(fail());
                    }
                }
            }
        }
        for (actor, life) in all_lives(store) {
            let facts = &bodies[&actor];
            if facts.body.actor != actor
                || (facts.body.alive && facts.aging_enabled) != ages.contains(&actor)
            {
                return Err(fail());
            }
            if facts.body.alive && facts.aging_enabled {
                let age = life.age_at(now).map_err(|_| fail())?;
                if age >= self.rules.effective_lifespan(facts.lifespan_loss)
                    || life.stage != self.rules.stage_at(age)
                {
                    return Err(fail());
                }
            }
        }
        for (id, pregnancy) in all_pregnancies(store) {
            match pregnancy.state {
                PregnancyState::Conceiving { completes } => {
                    for actor in [pregnancy.gestator, pregnancy.partner] {
                        if bodies[&actor].body.attempt
                            != Some(Attempt {
                                pregnancy: id,
                                completes,
                            })
                        {
                            return Err(fail());
                        }
                    }
                }
                PregnancyState::Gestating { .. }
                    if !births.contains(&id) || !bodies[&pregnancy.gestator].body.alive =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod staging_tests;

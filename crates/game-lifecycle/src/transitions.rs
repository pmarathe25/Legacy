//! Prepared lifecycle changes own their guards and effects. Callers provide
//! current physical/choice facts and IDs, never a world reference or a verdict.
use super::*;
use game_core::Location;

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
    Stage {
        actor: PersonId,
        before: LifeStage,
        after: LifeStage,
    },
    NewLife(Life),
}
#[derive(Clone, Debug)]
pub struct Prepared {
    edits: Vec<Edit>,
    effects: Vec<Effect>,
}
impl Prepared {
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub fn validate(&self, world: &World) -> Result<(), Error> {
        for edit in &self.edits {
            let valid = match edit {
                Edit::Pregnancy { before, after } => {
                    world.pregnancies.get(&after.id) == before.as_ref()
                }
                Edit::Stage { actor, before, .. } => {
                    world.lives.get(actor).map(|life| life.stage) == Some(*before)
                }
                Edit::NewLife(life) => !world.lives.contains_key(&life.id),
            };
            if !valid {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn commit(self, world: &mut World) -> Result<Vec<Effect>, Error> {
        self.validate(world)?;
        for edit in self.edits {
            match edit {
                Edit::Pregnancy { after, .. } => {
                    world.pregnancies.insert(after.id, after);
                }
                Edit::Stage { actor, after, .. } => {
                    world.lives.get_mut(&actor).unwrap().stage = after
                }
                Edit::NewLife(life) => {
                    world.lives.insert(life.id, life);
                }
            }
        }
        Ok(self.effects)
    }
    fn noop() -> Self {
        Self {
            edits: Vec::new(),
            effects: Vec::new(),
        }
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
impl World {
    pub fn preflight_deadlines(
        &self,
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
                    if self.pregnancies.get(&id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Conceiving { completes: at })
                    }) {
                        return Err(Error::Unavailable);
                    }
                    at.0.checked_add(self.rules.gestation.0)
                        .ok_or(Error::Overflow)?;
                }
                Deadline::Birth(id) => {
                    if self.pregnancies.get(&id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Gestating { due: at })
                    }) {
                        return Err(Error::Unavailable);
                    }
                    at.0.checked_add(self.rules.infancy.0)
                        .ok_or(Error::Overflow)?;
                }
                Deadline::Age(id) => {
                    self.lives.get(&id).ok_or(Error::Unavailable)?.age_at(at)?;
                }
            }
        }
        Ok(())
    }
    pub fn prepare_start_conception(
        &self,
        now: SimTime,
        id: PregnancyId,
        bodies: [&BodyFacts; 2],
        consent: Consent,
    ) -> Result<Prepared, Error> {
        let [first, second] = bodies;
        let a = self.lives.get(&first.actor).ok_or(Error::Unavailable)?;
        let b = self.lives.get(&second.actor).ok_or(Error::Unavailable)?;
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
        if self.pregnancies.values().any(|pregnancy| {
            pregnancy.gestator == gestator
                && matches!(
                    pregnancy.state,
                    PregnancyState::Conceiving { .. } | PregnancyState::Gestating { .. }
                )
        }) {
            return Err(Error::Unavailable);
        }
        if !consent.initiator || !consent.partner {
            return Ok(Prepared {
                edits: Vec::new(),
                effects: vec![Effect::Refused {
                    initiator: first.actor,
                    partner: second.actor,
                }],
            });
        }
        if id.0 == 0 || self.pregnancies.contains_key(&id) {
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
        Ok(Prepared {
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
        id: PregnancyId,
        now: SimTime,
        bodies: [&BodyFacts; 2],
    ) -> Result<Prepared, Error> {
        let before = self.pregnancies.get(&id).ok_or(Error::Unavailable)?;
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
        Ok(Prepared {
            edits: vec![Edit::Pregnancy {
                before: Some(before.clone()),
                after,
            }],
            effects: vec![effect],
        })
    }
    pub fn prepare_interruption(
        &self,
        actor: PersonId,
        died: bool,
        affected: impl IntoIterator<Item = PregnancyId>,
    ) -> Prepared {
        let mut prepared = Prepared::noop();
        for id in affected.into_iter().collect::<BTreeSet<_>>() {
            let Some(before) = self.pregnancies.get(&id) else {
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
        actor: PersonId,
        now: SimTime,
        body_alive: bool,
        lifespan_loss: u64,
        aging_enabled: bool,
    ) -> Result<Prepared, Error> {
        let life = self.lives.get(&actor).ok_or(Error::Unavailable)?;
        if !body_alive || !aging_enabled {
            return Ok(Prepared::noop());
        }
        let effect = self.evaluate_age(actor, now, lifespan_loss, true)?;
        if effect.died {
            return Ok(Prepared {
                edits: Vec::new(),
                effects: vec![Effect::Death { actor }],
            });
        }
        Ok(Prepared {
            edits: vec![Edit::Stage {
                actor,
                before: life.stage,
                after: effect.stage,
            }],
            effects: vec![Effect::Developed(effect)],
        })
    }
    pub fn prepare_birth(
        &self,
        id: PregnancyId,
        now: SimTime,
        child: PersonId,
        gestator: &BodyFacts,
        seed: u64,
    ) -> Result<Prepared, Error> {
        let before = self.pregnancies.get(&id).ok_or(Error::Unavailable)?;
        if matches!(
            before.state,
            PregnancyState::Ended | PregnancyState::Born(_)
        ) {
            return Ok(Prepared::noop());
        }
        if before.state != (PregnancyState::Gestating { due: now })
            || gestator.actor != before.gestator
        {
            return Err(Error::Unavailable);
        }
        let parents = [before.gestator, before.partner];
        let mut after = before.clone();
        if !gestator.alive {
            after.state = PregnancyState::Ended;
            return Ok(Prepared {
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
        if child.0 == 0 || parents.contains(&child) || self.lives.contains_key(&child) {
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
        Ok(Prepared {
            edits: vec![
                Edit::Pregnancy {
                    before: Some(before.clone()),
                    after,
                },
                Edit::NewLife(life),
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
impl World {
    /// Validate this FSM against explicit external body and scheduler facts.
    /// Physical activity storage and political identities remain outside it.
    pub fn validate_with_facts(
        &self,
        now: SimTime,
        bodies: &BTreeMap<PersonId, ValidationBody>,
        deadlines: &[(SimTime, Deadline)],
    ) -> Result<(), Error> {
        validate_local(self, now)?;
        let fail = || Error::InvalidSave("inconsistent lifecycle body or deadline facts".into());
        if self.lives.keys().ne(bodies.keys()) {
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
                    let life = self.lives.get(&actor).ok_or_else(fail)?;
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
                    if self.pregnancies.get(&id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Gestating { due: *at })
                    }) || !births.insert(id)
                    {
                        return Err(fail());
                    }
                }
                Deadline::Conception(id) => {
                    if self.pregnancies.get(&id).is_none_or(|pregnancy| {
                        pregnancy.state != (PregnancyState::Conceiving { completes: *at })
                    }) {
                        return Err(fail());
                    }
                }
            }
        }
        for (actor, life) in &self.lives {
            let facts = &bodies[actor];
            if facts.body.actor != *actor
                || (facts.body.alive && facts.aging_enabled) != ages.contains(actor)
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
        for (id, pregnancy) in &self.pregnancies {
            match pregnancy.state {
                PregnancyState::Conceiving { completes } => {
                    for actor in [pregnancy.gestator, pregnancy.partner] {
                        if bodies[&actor].body.attempt
                            != Some(Attempt {
                                pregnancy: *id,
                                completes,
                            })
                        {
                            return Err(fail());
                        }
                    }
                }
                PregnancyState::Gestating { .. }
                    if !births.contains(id) || !bodies[&pregnancy.gestator].body.alive =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

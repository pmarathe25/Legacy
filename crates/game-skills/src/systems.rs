//! Skills' native owner workflow. Activity domains provide factual inputs via
//! readonly requests; this owner selects credit and writes only Skills rows.
use crate::*;
use game_ecs::StableId;
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    LeveledUp {
        actor: PersonId,
        skill: Skill,
        level: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub actor: PersonId,
    pub skills: Vec<SkillView>,
}

/// Source owners report admitted work evidence from their own native payloads.
/// Skills supplies the elapsed interval and applies mastery policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkEvidence {
    pub actor: PersonId,
    pub skill: Skill,
    pub mastery_cap: Option<u64>,
}

macro_rules! evidence_request {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            pub from: SimTime,
            pub next: SimTime,
        }
        impl Request for $name {
            type Response = Vec<WorkEvidence>;
        }
    };
}

evidence_request!(ReadAgricultureEvidence);
evidence_request!(ReadWorkshopEvidence);
evidence_request!(ReadHealthEvidence);
evidence_request!(ReadPhysicalEvidence);

/// Social owns interaction readiness and reports only the facts that affect
/// Skills policy; Skills decides which completed portion earns credit.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadSocialLearningFacts {
    pub from: SimTime,
    pub next: SimTime,
}
impl Request for ReadSocialLearningFacts {
    type Response = Vec<SocialFacts>;
}

#[derive(Clone, Copy, Debug)]
pub struct ReadIdentity(pub PersonId);
impl Request for ReadIdentity {
    type Response = bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyLearningFacts {
    pub alive: bool,
    pub health: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct ReadBodyLearningFacts(pub PersonId);
impl Request for ReadBodyLearningFacts {
    type Response = Option<BodyLearningFacts>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReadPhysicalSkillsEnabled;
impl Request for ReadPhysicalSkillsEnabled {
    type Response = bool;
}

#[derive(Clone, Copy, Debug)]
pub struct ReadSkillLevel {
    pub actor: PersonId,
    pub skill: Skill,
}
impl Request for ReadSkillLevel {
    type Response = u16;
}

#[derive(Clone, Copy, Debug)]
pub struct ReadWorkRate {
    pub actor: PersonId,
    pub skill: Skill,
}
impl Request for ReadWorkRate {
    type Response = SkillRate;
}

#[derive(Clone, Copy, Debug)]
pub struct ReadSkillView(pub PersonId);
impl Request for ReadSkillView {
    type Response = Vec<SkillView>;
}

#[derive(Clone, Debug)]
pub enum Input {
    Initialize {
        runtime: Runtime,
        people: Vec<PersonId>,
    },
    Advance {
        from: SimTime,
        next: SimTime,
    },
    Boundary {
        from: SimTime,
        limit: SimTime,
    },
    Receive(LearningEvent),
    CreditAction {
        actor: PersonId,
        skill: Skill,
        milliseconds: u64,
    },
    Observe(PersonId),
    ValidateSave {
        now: SimTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Done,
    Boundary(SimTime),
    View(Option<View>),
}

#[derive(Default)]
pub struct Systems;
type Tx<'a, 's, E> = OwnerTxn<'a, 's, Systems, E>;

/// Install Skills against the existing canonical runtime field. Skills has no
/// persistent due work: its level boundaries are derived from active practice.
pub fn register_systems<S: 'static, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    runtime: RuntimeSlot<S, Option<Runtime>>,
) -> Result<(), Error> {
    registry.register(Systems, runtime)?;
    registry.owns::<Systems, Skills>()?;
    registry.register_request::<Systems, RecordLearningEvent>(receive_event::<E>)?;
    registry.register_read_request::<Systems, ReadSkillLevel>(read_level::<E>)?;
    registry.register_read_request::<Systems, ReadWorkRate>(read_work_rate::<E>)?;
    registry.register_read_request::<Systems, ReadSkillView>(read_view::<E>)
}

#[derive(Clone, Debug)]
pub struct RecordLearningEvent(pub LearningEvent);
impl Request for RecordLearningEvent {
    type Response = ();
}

pub fn receive_event<E: From<Event> + 'static>(
    _: &Systems,
    tx: &mut Tx<'_, '_, E>,
    request: RecordLearningEvent,
) -> Result<(), Error> {
    receive(tx, request.0)
}

impl<E: From<Event> + 'static> OwnerSystem<E> for Systems {
    type Runtime = Option<Runtime>;
    type Input = Input;
    type Output = Output;

    fn execute(&self, tx: &mut Tx<'_, '_, E>, input: Input) -> Result<Output, Error> {
        match input {
            Input::Initialize { runtime, people } => initialize(tx, runtime, people)?,
            Input::Advance { from, next } => advance(tx, from, next)?,
            Input::Boundary { from, limit } => {
                return Ok(Output::Boundary(boundary(tx, from, limit)?));
            }
            Input::Receive(event) => receive(tx, event)?,
            Input::CreditAction {
                actor,
                skill,
                milliseconds,
            } => credit_action(tx, actor, skill, milliseconds)?,
            Input::Observe(actor) => return Ok(Output::View(view(tx, actor)?)),
            Input::ValidateSave { now } => validate_save(tx, now)?,
        }
        Ok(Output::Done)
    }
}

fn initialize<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    runtime: Runtime,
    mut people: Vec<PersonId>,
) -> Result<(), Error> {
    if tx.runtime().is_some() {
        return Err(Error::Unavailable);
    }
    runtime.rules.validate()?;
    people.sort_unstable();
    if people.iter().any(|person| person.0 == 0)
        || people.windows(2).any(|pair| pair[0] == pair[1])
        || people.is_empty()
    {
        return Err(Error::InvalidConfiguration(
            "invalid initial skill people".into(),
        ));
    }
    for actor in &people {
        let id = StableId(actor.0);
        if !tx.request(ReadIdentity(*actor))? || tx.read::<Skills>(id).is_some() {
            return Err(Error::InvalidConfiguration(
                "invalid initial skill rows".into(),
            ));
        }
    }
    *tx.runtime_mut() = Some(runtime);
    for actor in people {
        tx.write(StableId(actor.0), Skills::default())?;
    }
    Ok(())
}

fn evidence_for<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    from: SimTime,
    next: SimTime,
) -> Result<(Vec<PracticeDelta>, Vec<SocialFacts>), Error> {
    let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
    let mut work = Vec::new();
    for rows in [
        tx.request(ReadAgricultureEvidence { from, next })?,
        tx.request(ReadWorkshopEvidence { from, next })?,
        tx.request(ReadHealthEvidence { from, next })?,
        tx.request(ReadPhysicalEvidence { from, next })?,
    ] {
        for row in rows {
            if row.actor.0 == 0 || !tx.request(ReadIdentity(row.actor))? {
                return Err(Error::Unavailable);
            }
            if !tx
                .request(ReadBodyLearningFacts(row.actor))?
                .is_some_and(|body| body.alive && body.health > 0)
            {
                continue;
            }
            work.push(PracticeDelta {
                actor: row.actor,
                skill: row.skill,
                milliseconds: elapsed,
                mastery_cap: row.mastery_cap,
            });
        }
    }
    let social = tx.request(ReadSocialLearningFacts { from, next })?;
    for facts in &social {
        let actors = match facts.interaction {
            InteractionFacts::Talk { first, second } => vec![first, second],
            InteractionFacts::Instruction {
                teacher, student, ..
            } => vec![teacher, student],
            InteractionFacts::Practice { actor, .. } => vec![actor],
        };
        for actor in actors {
            if !tx.request(ReadIdentity(actor))? {
                return Err(Error::Unavailable);
            }
        }
    }
    Ok((work, social))
}

fn evaluate<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    from: SimTime,
    next: SimTime,
) -> Result<Option<EvaluatedLearning>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(None);
    };
    let (work, social) = evidence_for(tx, from, next)?;
    let evaluation =
        runtime.evaluate_learning_with_practice(from, next, [], social, |actor, skill| {
            tx.read::<Skills>(StableId(actor.0))
                .map_or(0, |row| row.practice(skill))
        })?;
    let mut evaluation = evaluation;
    evaluation.deltas.extend(work);
    Ok(Some(evaluation))
}

fn advance<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    from: SimTime,
    next: SimTime,
) -> Result<(), Error> {
    if next != tx.now() || next < from {
        return Err(Error::SequenceConflict);
    }
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(());
    };
    let Some(evaluation) = evaluate(tx, from, next)? else {
        return Ok(());
    };
    let mut changed = BTreeMap::<PersonId, (Skills, BTreeMap<Skill, u16>)>::new();
    for delta in evaluation.deltas() {
        if delta.milliseconds == 0 {
            continue;
        }
        let id = StableId(delta.actor.0);
        if !tx.request(ReadIdentity(delta.actor))? {
            return Err(Error::Unavailable);
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = changed.entry(delta.actor) {
            let row = tx.read::<Skills>(id).cloned().unwrap_or_default();
            row.validate_progress(&runtime.rules)?;
            entry.insert((row, BTreeMap::new()));
        }
        let (row, before) = changed.get_mut(&delta.actor).expect("inserted skill row");
        before
            .entry(delta.skill)
            .or_insert_with(|| row.level(delta.skill));
        runtime.apply_practice_row(row, delta)?;
    }
    for (actor, (row, before)) in changed {
        row.validate_progress(&runtime.rules)?;
        tx.write(StableId(actor.0), row.clone())?;
        for (skill, level) in before {
            let after = row.level(skill);
            if after > level {
                tx.emit(E::from(Event::LeveledUp {
                    actor,
                    skill,
                    level: after,
                }))?;
            }
        }
    }
    Ok(())
}

fn boundary<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    from: SimTime,
    limit: SimTime,
) -> Result<SimTime, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(limit);
    };
    let Some(evaluation) = evaluate(tx, from, limit)? else {
        return Ok(limit);
    };
    runtime.practice_boundary_with(from, limit, evaluation.deltas(), |actor, skill| {
        tx.read::<Skills>(StableId(actor.0))
            .map_or(0, |row| row.practice(skill))
    })
}

fn receive<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    event: LearningEvent,
) -> Result<(), Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(());
    };
    let actors: BTreeSet<_> = match &event {
        LearningEvent::Born { child } => BTreeSet::from([*child]),
        LearningEvent::AgreementActivated { accepted } => accepted.iter().copied().collect(),
        LearningEvent::AgreementAmended { proposer } => BTreeSet::from([*proposer]),
    };
    for actor in actors {
        if !tx.request(ReadIdentity(actor))? {
            return Err(Error::Unavailable);
        }
        let id = StableId(actor.0);
        let mut row = tx.read::<Skills>(id).cloned().unwrap_or_default();
        runtime.apply_learning_event_row(&mut row, actor, &event, tx.now())?;
        row.validate_progress(&runtime.rules)?;
        tx.write(id, row)?;
    }
    Ok(())
}

fn credit_action<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
    skill: Skill,
    milliseconds: u64,
) -> Result<(), Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(());
    };
    if milliseconds == 0 {
        return Ok(());
    }
    if !tx.request(ReadIdentity(actor))? {
        return Err(Error::Unavailable);
    }
    let id = StableId(actor.0);
    let opening = tx.read::<Skills>(id).cloned().unwrap_or_default();
    let mut row = tx.read::<Skills>(id).cloned().unwrap_or_default();
    let before = row.level(skill);
    runtime.credit_action_row(&mut row, skill, milliseconds, tx.now())?;
    let after = row.level(skill);
    if row != opening {
        tx.write(id, row)?;
    }
    if after > before {
        tx.emit(E::from(Event::LeveledUp {
            actor,
            skill,
            level: after,
        }))?;
    }
    Ok(())
}

fn view<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    actor: PersonId,
) -> Result<Option<View>, Error> {
    if !tx.request(ReadIdentity(actor))? {
        return Ok(None);
    }
    let skills = tx.runtime().clone().map_or_else(Vec::new, |runtime| {
        runtime.view_row(tx.read::<Skills>(StableId(actor.0)))
    });
    Ok(Some(View { actor, skills }))
}

fn validate_save<E: From<Event> + 'static>(
    tx: &mut Tx<'_, '_, E>,
    now: SimTime,
) -> Result<(), Error> {
    if now != tx.now() {
        return Err(Error::SequenceConflict);
    }
    let rows = tx.iter::<Skills>();
    let Some(runtime) = tx.runtime().clone() else {
        return if rows.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidSave("inconsistent personal skills".into()))
        };
    };
    if !tx.request(ReadPhysicalSkillsEnabled)? {
        return Err(Error::InvalidSave("inconsistent personal skills".into()));
    }
    runtime
        .validate_rows(now, rows.iter().map(|(id, row)| (PersonId(id.0), row)))
        .map_err(|_| Error::InvalidSave("inconsistent personal skills".into()))?;
    for (id, _) in &rows {
        if !tx.request(ReadIdentity(PersonId(id.0)))? {
            return Err(Error::InvalidSave("inconsistent personal skills".into()));
        }
    }
    Ok(())
}

pub fn read_level<E: From<Event> + 'static>(
    _: &Systems,
    tx: &mut OwnerReadTxn<'_, '_, Systems, E>,
    request: ReadSkillLevel,
) -> Result<u16, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(0);
    };
    Ok(tx
        .read::<Skills>(StableId(request.actor.0))
        .map_or(0, |row| runtime.level_row(row, request.skill)))
}

pub fn read_work_rate<E: From<Event> + 'static>(
    _: &Systems,
    tx: &mut OwnerReadTxn<'_, '_, Systems, E>,
    request: ReadWorkRate,
) -> Result<SkillRate, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(SkillRate::default());
    };
    let level = tx
        .read::<Skills>(StableId(request.actor.0))
        .map_or(0, |row| runtime.level_row(row, request.skill));
    Ok(rate_for_level(level, &runtime.rules))
}

pub fn read_view<E: From<Event> + 'static>(
    _: &Systems,
    tx: &mut OwnerReadTxn<'_, '_, Systems, E>,
    request: ReadSkillView,
) -> Result<Vec<SkillView>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(Vec::new());
    };
    Ok(runtime.view_row(tx.read::<Skills>(StableId(request.0.0))))
}

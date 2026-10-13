//! Temperament owns expression and acquired offsets in the native journal.
use super::{PersonTemperament, Runtime, Temperament};
use crate::decisions::Personality;
use game_core::{Error, PersonId};
use game_ecs::StableId;
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};

pub struct NewbornDisposition(pub PersonId);
impl Request for NewbornDisposition {
    type Response = Option<Personality>;
}
pub struct LearnExperience {
    pub actor: PersonId,
    pub experience: String,
}
impl Request for LearnExperience {
    type Response = Option<Temperament>;
}
pub struct ReadExpression(pub PersonId);
impl Request for ReadExpression {
    type Response = Option<Temperament>;
}

pub struct Systems;
impl<E: 'static> OwnerSystem<E> for Systems {
    type Runtime = Option<Runtime>;
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
pub fn register_systems<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
) -> Result<(), Error> {
    registry.register(Systems, slot)?;
    registry.owns::<Systems, PersonTemperament>()?;
    registry.register_request::<Systems, NewbornDisposition>(newborn::<E>)?;
    registry.register_request::<Systems, LearnExperience>(learn::<E>)?;
    registry.register_read_request::<Systems, ReadExpression>(expression::<E>)
}
fn expressed<E: 'static>(
    tx: &mut OwnerTxn<'_, '_, Systems, E>,
    runtime: &Runtime,
    actor: PersonId,
    row: &PersonTemperament,
) -> Result<Temperament, Error> {
    if runtime.definitions.traits.is_empty() {
        return acquired(runtime, row);
    }
    #[cfg(feature = "genetics")]
    {
        let pairs = tx.request(game_genetics::services::ReadAlleleScores {
            actor,
            loci: runtime
                .definitions
                .traits
                .iter()
                .map(|definition| definition.inheritance.locus.clone())
                .collect(),
        })?;
        let Some(pairs) = pairs else {
            return acquired(runtime, row);
        };
        runtime.express_record(row, |locus| {
            let [first, second] = *pairs.get(locus).ok_or(Error::Unavailable)?;
            Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
        })
    }
    #[cfg(not(feature = "genetics"))]
    {
        let _ = (tx, actor);
        acquired(runtime, row)
    }
}
fn acquired(runtime: &Runtime, row: &PersonTemperament) -> Result<Temperament, Error> {
    let mut baseline = runtime.clone();
    baseline.definitions.traits.clear();
    baseline.express_record(row, |_| Err(Error::Unavailable))
}
pub fn newborn<E: 'static>(
    _: &Systems,
    tx: &mut OwnerTxn<'_, '_, Systems, E>,
    request: NewbornDisposition,
) -> Result<Option<Personality>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(None);
    };
    runtime.definitions.validate()?;
    let id = StableId(request.0.0);
    if tx.read::<PersonTemperament>(id).is_some() {
        return Err(Error::Unavailable);
    }
    let mut personality = runtime.definitions.newborn_personality.clone();
    let row = PersonTemperament {
        baseline: Temperament::from(&personality),
        learned_offsets: [0; 4],
    };
    expressed(tx, &runtime, request.0, &row)?.apply_to(&mut personality);
    tx.write(id, row)?;
    Ok(Some(personality))
}
pub fn expression<E: 'static>(
    _: &Systems,
    tx: &mut OwnerReadTxn<'_, '_, Systems, E>,
    request: ReadExpression,
) -> Result<Option<Temperament>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(None);
    };
    let Some(row) = tx.read::<PersonTemperament>(StableId(request.0.0)).cloned() else {
        return Ok(None);
    };
    if runtime.definitions.traits.is_empty() {
        return acquired(&runtime, &row).map(Some);
    }
    #[cfg(feature = "genetics")]
    {
        let pairs = tx.request(game_genetics::services::ReadAlleleScores {
            actor: request.0,
            loci: runtime
                .definitions
                .traits
                .iter()
                .map(|definition| definition.inheritance.locus.clone())
                .collect(),
        })?;
        let Some(pairs) = pairs else {
            return acquired(&runtime, &row).map(Some);
        };
        runtime
            .express_record(&row, |locus| {
                let [first, second] = *pairs.get(locus).ok_or(Error::Unavailable)?;
                Ok(((i64::from(first) + i64::from(second)) / 2) as i32)
            })
            .map(Some)
    }
    #[cfg(not(feature = "genetics"))]
    acquired(&runtime, &row).map(Some)
}
pub fn learn<E: 'static>(
    _: &Systems,
    tx: &mut OwnerTxn<'_, '_, Systems, E>,
    request: LearnExperience,
) -> Result<Option<Temperament>, Error> {
    let Some(runtime) = tx.runtime().clone() else {
        return Ok(None);
    };
    let Some(definition) = runtime
        .definitions
        .learning
        .iter()
        .find(|definition| definition.id == request.experience)
    else {
        return Ok(None);
    };
    let id = StableId(request.actor.0);
    let Some(mut row) = tx.read::<PersonTemperament>(id).cloned() else {
        return Ok(None);
    };
    runtime.apply_learning(&mut row, definition.deltas);
    let expression = expressed(tx, &runtime, request.actor, &row)?;
    tx.write(id, row)?;
    Ok(Some(expression))
}

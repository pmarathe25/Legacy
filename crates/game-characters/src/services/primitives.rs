//! Small source-owned receiving contracts for personal facts and learning.
use super::*;

pub struct ReadPeople;
impl Request for ReadPeople {
    type Response = Vec<PersonId>;
}
pub struct ReadKnownStocks(pub PersonId);
impl Request for ReadKnownStocks {
    type Response = Vec<StockKnowledge>;
}
pub struct ReadStocks(pub PersonId);
impl Request for ReadStocks {
    type Response = Vec<StockKnowledge>;
}
pub struct ReadPersonality(pub PersonId);
impl Request for ReadPersonality {
    type Response = Option<Personality>;
}
pub struct ReadTrust {
    pub actor: PersonId,
    pub target: PersonId,
}
impl Request for ReadTrust {
    type Response = i32;
}
pub struct ReceiveJudgment {
    pub observer: PersonId,
    pub target: PersonId,
    pub trust: i32,
}
impl Request for ReceiveJudgment {
    type Response = ();
}
pub struct ReadNeedyChild {
    pub actor: PersonId,
    pub area: AreaId,
}
impl Request for ReadNeedyChild {
    type Response = bool;
}
pub struct ReceivePermission {
    pub inventory: InventoryId,
    pub recipient: PersonId,
    pub allowed: bool,
    pub sender: PersonId,
}
impl Request for ReceivePermission {
    type Response = ();
}
pub enum MemoryLearning {
    Stock {
        fact: StockKnowledge,
        source: KnowledgeSource,
    },
    Route {
        origin: AreaId,
        destination: AreaId,
        duration: SimDuration,
    },
}
pub struct ReceiveMemoryLearning {
    pub actor: PersonId,
    pub now: SimTime,
    pub memory: MemoryLearning,
}
impl Request for ReceiveMemoryLearning {
    type Response = ();
}

pub fn register_primitives<S: 'static, C: Component, E: 'static>(
    registry: &mut Registry<S, E>,
) -> Result<(), Error> {
    registry.register_read_request::<Systems<C>, ReadPeople>(read_people::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadKnownStocks>(read_stocks::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadStocks>(read_stocks_alias::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadPersonality>(read_personality::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadTrust>(read_trust::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadNeedyChild>(read_needy::<C, E>)?;
    registry.register_request::<Systems<C>, ReceiveJudgment>(receive_judgment::<C, E>)?;
    registry.register_request::<Systems<C>, ReceivePermission>(receive_permission::<C, E>)?;
    registry.register_request::<Systems<C>, ReceiveMemoryLearning>(receive_memory::<C, E>)
}
fn read_people<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    _: ReadPeople,
) -> Result<Vec<PersonId>, Error> {
    Ok(tx
        .iter::<Person>()
        .into_iter()
        .map(|(id, _)| PersonId(id.0))
        .collect())
}
fn read_stocks<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    request: ReadKnownStocks,
) -> Result<Vec<StockKnowledge>, Error> {
    if tx.runtime().is_none() {
        return Ok(Vec::new());
    }
    Ok(tx
        .read::<Stocks>(StableId(request.0.0))
        .map(|row| row.known.values().cloned().collect())
        .unwrap_or_default())
}
fn read_stocks_alias<C: Component, E: 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    request: ReadStocks,
) -> Result<Vec<StockKnowledge>, Error> {
    read_stocks(owner, tx, ReadKnownStocks(request.0))
}
fn read_trust<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    request: ReadTrust,
) -> Result<i32, Error> {
    if tx.runtime().is_none() {
        return Ok(0);
    }
    Ok(tx
        .read::<Mind<C>>(StableId(request.actor.0))
        .and_then(|mind| mind.relationships.get(&request.target))
        .map_or(0, |relationship| relationship.trust))
}
fn read_needy<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    request: ReadNeedyChild,
) -> Result<bool, Error> {
    Ok(tx.runtime().is_some()
        && tx
            .read::<Mind<C>>(StableId(request.actor.0))
            .is_some_and(|mind| {
                mind.child_signals.values().any(|signal| {
                    signal.location == Location::AtArea(request.area) && signal.urgency >= 1_000
                })
            }))
}
fn receive_judgment<C: Component, E: 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    request: ReceiveJudgment,
) -> Result<(), Error> {
    owner.relationship(tx, request.observer, request.target, request.trust, 0)
}
fn receive_permission<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    request: ReceivePermission,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    if request.sender.0 == 0 {
        return Err(Error::Unavailable);
    }
    let Some(mut stocks) = tx.read::<Stocks>(StableId(request.recipient.0)).cloned() else {
        return Ok(());
    };
    if let Some(known) = stocks.known.get_mut(&request.inventory) {
        known.accessible = request.allowed;
        known.learned_at = tx.now();
        known.source = KnowledgeSource::Reported(request.sender);
        tx.write(StableId(request.recipient.0), stocks)?;
    }
    Ok(())
}
fn receive_memory<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    request: ReceiveMemoryLearning,
) -> Result<(), Error> {
    if request.now != tx.now() {
        return Err(Error::Unavailable);
    }
    let id = StableId(request.actor.0);
    let mut mind = tx.read::<Mind<C>>(id).cloned().ok_or(Error::Unavailable)?;
    match request.memory {
        MemoryLearning::Stock { fact, source } => {
            let source_id = match source {
                KnowledgeSource::Observed(id) | KnowledgeSource::Reported(id) => id.0,
                KnowledgeSource::Recorded(id) => id.0,
            };
            if fact.inventory.0 == 0
                || fact.area.0 == 0
                || fact.observed_at > request.now
                || source_id == 0
            {
                return Err(Error::Unavailable);
            }
            let mut stocks = tx.read::<Stocks>(id).cloned().ok_or(Error::Unavailable)?;
            stocks.learn_memory_stock(request.now, fact, source);
            tx.write(id, stocks)
        }
        MemoryLearning::Route {
            origin,
            destination,
            duration,
        } => {
            if origin.0 == 0 || destination.0 == 0 || duration.0 == 0 || origin == destination {
                return Err(Error::Unavailable);
            }
            mind.learn_memory_route(origin, destination, duration);
            tx.write(id, mind)
        }
    }
}

fn read_personality<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    request: ReadPersonality,
) -> Result<Option<Personality>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    Ok(tx
        .read::<Mind<C>>(StableId(request.0.0))
        .map(|mind| mind.personality.clone()))
}

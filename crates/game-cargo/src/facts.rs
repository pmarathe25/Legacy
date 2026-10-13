//! Native resource claims and allocation reads over the transaction's physical rows.
use crate::{
    Inventory, InventoryLocation, Reservation, ReservationTotals, ToolAllocations, ToolBodyFacts,
    ToolSlots, ToolUse,
};
use game_core::{AreaId, Error, Good, InventoryId, Location, PersonId, Quantity};
use game_ecs::{Schema, StableId};
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
use serde::{Deserialize, Serialize};

/// A current equipment demand, keyed by the actor whose owner acquired it.
/// Toolkit admission remains the stable-id ranked allocator, never a boolean lock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolClaim {
    pub tool: InventoryId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedStock {
    pub source: InventoryId,
    pub quantity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationClaim {
    pub stock: Vec<ReservedStock>,
}
pub struct SetToolClaim {
    pub actor: PersonId,
    pub claim: Option<ToolClaim>,
}
pub struct ReadToolClaim(pub PersonId);
impl Request for ReadToolClaim {
    type Response = Option<InventoryId>;
}
impl Request for SetToolClaim {
    type Response = ();
}
pub struct SetReservationClaim {
    pub actor: PersonId,
    pub claim: Option<ReservationClaim>,
}
impl Request for SetReservationClaim {
    type Response = ();
}
pub struct ReadInventory(pub InventoryId);
impl Request for ReadInventory {
    type Response = Option<Inventory>;
}
pub struct ReadInventories;
impl Request for ReadInventories {
    type Response = Vec<Inventory>;
}
pub struct ReadInventoryArea {
    pub inventory: InventoryId,
}
impl Request for ReadInventoryArea {
    type Response = Option<AreaId>;
}
pub struct ReadCarryingRoom(pub PersonId);
impl Request for ReadCarryingRoom {
    type Response = Quantity;
}
pub struct ReadStockBudget {
    pub actor: PersonId,
    pub inventory: InventoryId,
    /// Raw reservation facts supplied by another receiving owner with a Cargo dependency.
    pub additional_reservations: Vec<Reservation>,
}
#[derive(Clone, Copy, Debug)]
pub struct StockBudget {
    pub unreserved: Quantity,
    pub tool_allocated: bool,
}
impl Request for ReadStockBudget {
    type Response = StockBudget;
}
/// Institutional access is a raw grant; Cargo combines it with personal access.
pub struct ReadStockPermission {
    pub actor: PersonId,
    pub inventory: InventoryId,
}
impl Request for ReadStockPermission {
    type Response = bool;
}
/// An accepting obligation owner authorizes only this particular receipt.
pub struct ReadReceiptAcceptance {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
}
impl Request for ReadReceiptAcceptance {
    type Response = bool;
}
/// Agreement owners report an accepted receipt without granting general access.
pub struct ReadAgreementAcceptance {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
}
impl Request for ReadAgreementAcceptance {
    type Response = bool;
}
/// Guardian owners report current accepted supply for this exact compartment.
pub struct ReadGuardianAcceptance {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
}
impl Request for ReadGuardianAcceptance {
    type Response = bool;
}
/// Record owners interpret their units and grants; Cargo admits the conjunction.
pub struct ReadRecordPermission {
    pub actor: PersonId,
    pub source: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
}
impl Request for ReadRecordPermission {
    type Response = bool;
}
#[derive(Default)]
pub struct Services;
impl<E: 'static> OwnerSystem<E> for Services {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<ToolClaim>("cargo/tool_claim", |claim| {
        if claim.tool.0 == 0 {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    })?;
    schema.register::<ReservationClaim>("cargo/reservation_claim", |claim| {
        if claim
            .stock
            .iter()
            .any(|row| row.source.0 == 0 || row.quantity.0 == 0)
        {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    })
}
pub fn register_services<S: 'static, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, ()>,
) -> Result<(), Error> {
    registry.register(Services, slot)?;
    registry.owns::<Services, ToolClaim>()?;
    registry.owns::<Services, ReservationClaim>()?;
    registry.register_request::<Services, SetToolClaim>(set_tool::<E>)?;
    registry.register_read_request::<Services, ReadToolClaim>(read_tool::<E>)?;
    registry.register_request::<Services, SetReservationClaim>(set_reservation::<E>)?;
    registry.register_read_request::<Services, ReadStockBudget>(read_budget::<E>)?;
    registry
        .register_read_request::<Services, game_physiology::systems::ReadStock>(read_stock::<E>)?;
    registry.register_read_request::<Services, game_physiology::systems::ReadConsumableSources>(
        |_, tx, request| {
            let area = tx
                .request(game_transport::services::ReadPresence(request.0))?
                .and_then(|presence| match presence.resolved {
                    Location::AtArea(area) => Some(area),
                    _ => None,
                });
            Ok(tx
                .iter::<Inventory>()
                .into_iter()
                .filter(|(_, stock)| {
                    matches!(stock.good, Good::Food | Good::Water)
                        && match stock.location {
                            InventoryLocation::Carried(holder) => holder == request.0,
                            InventoryLocation::Area(stock_area) => area == Some(stock_area),
                        }
                })
                .map(|(_, stock)| stock.id)
                .collect())
        },
    )?;
    registry.register_read_request::<Services, game_transport::systems::ReadCargoMass>(
        read_mass::<E>,
    )?;
    registry.register_read_request::<Services, game_characters::services::ReadLocalStock>(
        read_local_stock::<E>,
    )?;
    registry.register_read_request::<Services, game_characters::services::ReadWorkEquipment>(
        read_work_equipment::<E>,
    )?;
    registry.register_read_request::<Services, game_characters::services::ReadInventoryGrant>(
        |_, tx, request| {
            Ok(tx
                .read::<Inventory>(StableId(request.0.0))
                .and_then(|row| row.last_granted))
        },
    )?;
    registry.register_read_request::<Services, ReadReceiptAcceptance>(|_, tx, request| {
        let agreement = tx.request(ReadAgreementAcceptance {
            actor: request.actor,
            source: request.source,
            destination: request.destination,
            quantity: request.quantity,
        })?;
        let guardian = tx.request(ReadGuardianAcceptance {
            actor: request.actor,
            source: request.source,
            destination: request.destination,
            quantity: request.quantity,
        })?;
        Ok(crate::receiver_authorization(agreement, guardian))
    })?;
    registry.register_read_request::<Services, ReadCarryingRoom>(|owner, tx, request| {
        let capacity = tx
            .request(game_physiology::services::ReadBody(request.0))?
            .map_or(Quantity(u64::MAX), |body| body.rules.carrying_capacity);
        let mass = read_mass(owner, tx, game_transport::systems::ReadCargoMass(request.0))?;
        Ok(Quantity(capacity.0.saturating_sub(mass.0)))
    })?;
    registry.register_read_request::<Services, ReadInventory>(|_, tx, request| {
        Ok(tx.read::<Inventory>(StableId(request.0.0)).cloned())
    })?;
    registry.register_read_request::<Services, ReadInventories>(|_, tx, _| {
        Ok(tx
            .iter::<Inventory>()
            .into_iter()
            .map(|(_, row)| row)
            .collect())
    })?;
    registry.register_read_request::<Services, ReadInventoryArea>(read_inventory_area::<E>)
}
pub fn read_tool<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: ReadToolClaim,
) -> Result<Option<InventoryId>, Error> {
    Ok(tx
        .read::<ToolClaim>(StableId(request.0.0))
        .map(|claim| claim.tool))
}
pub fn set_tool<E: 'static>(
    _: &Services,
    tx: &mut OwnerTxn<'_, '_, Services, E>,
    request: SetToolClaim,
) -> Result<(), Error> {
    let actor = StableId(request.actor.0);
    if let Some(claim) = request.claim {
        if request.actor.0 == 0
            || !tx
                .read::<Inventory>(StableId(claim.tool.0))
                .is_some_and(|stock| stock.good == Good::Tool)
        {
            return Err(Error::Unavailable);
        }
        tx.write(actor, claim)
    } else {
        tx.remove::<ToolClaim>(actor)
    }
}
pub fn set_reservation<E: 'static>(
    _: &Services,
    tx: &mut OwnerTxn<'_, '_, Services, E>,
    request: SetReservationClaim,
) -> Result<(), Error> {
    let actor = StableId(request.actor.0);
    if let Some(claim) = request.claim {
        if request.actor.0 == 0
            || claim.stock.iter().any(|row| {
                row.quantity.0 == 0 || tx.read::<Inventory>(StableId(row.source.0)).is_none()
            })
        {
            return Err(Error::Unavailable);
        }
        tx.write(actor, claim)
    } else {
        tx.remove::<ReservationClaim>(actor)
    }
}
fn inventory_area<E: 'static>(
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    stock: &Inventory,
) -> Result<Option<AreaId>, Error> {
    Ok(match stock.location {
        InventoryLocation::Area(area) => Some(area),
        InventoryLocation::Carried(actor) => tx
            .request(game_transport::services::ReadPresence(actor))?
            .and_then(|presence| match presence.resolved {
                Location::AtArea(area) => Some(area),
                _ => None,
            }),
    })
}
pub fn read_inventory_area<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: ReadInventoryArea,
) -> Result<Option<AreaId>, Error> {
    let Some(stock) = tx.read::<Inventory>(StableId(request.inventory.0)).cloned() else {
        return Ok(None);
    };
    inventory_area(tx, &stock)
}
pub fn read_budget<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: ReadStockBudget,
) -> Result<StockBudget, Error> {
    let Some(stock) = tx.read::<Inventory>(StableId(request.inventory.0)).cloned() else {
        return Ok(StockBudget {
            unreserved: Quantity(0),
            tool_allocated: false,
        });
    };
    let mut claims = request.additional_reservations;
    claims.extend(
        tx.request(game_physiology::services::ReadIntakeReservations {
            source: Some(stock.id),
        })?
        .into_iter()
        .map(|claim| Reservation {
            actor: claim.actor,
            source: claim.source,
            quantity: claim.quantity,
        }),
    );
    claims.extend(
        tx.request(game_transport::services::ReadLoadingReservations {
            source: Some(stock.id),
        })?
        .into_iter()
        .map(|claim| Reservation {
            actor: claim.actor,
            source: claim.source,
            quantity: claim.quantity,
        }),
    );
    for (actor, claim) in tx.iter::<ReservationClaim>() {
        claims.extend(
            claim
                .stock
                .into_iter()
                .filter(|claim| claim.source == stock.id)
                .map(|claim| Reservation {
                    actor: PersonId(actor.0),
                    source: claim.source,
                    quantity: claim.quantity,
                }),
        );
    }
    let unreserved =
        ReservationTotals::prepare(claims)?.available(stock.id, stock.quantity, request.actor);
    let tool_allocated = if stock.good == Good::Tool {
        let area = inventory_area(tx, &stock)?;
        let mut uses = Vec::new();
        for (actor, claim) in tx.iter::<ToolClaim>() {
            if claim.tool != stock.id {
                continue;
            }
            let actor = PersonId(actor.0);
            let Some(body) = tx.request(game_physiology::services::ReadBody(actor))? else {
                continue;
            };
            let stationary = tx
                .request(game_transport::services::ReadPresence(actor))?
                .and_then(|presence| match presence.location {
                    Location::AtArea(area) => Some(area),
                    _ => None,
                });
            if body.vitals.alive && body.vitals.health > 0 && area.is_some() && stationary == area {
                uses.push(ToolUse {
                    actor,
                    tool: stock.id,
                });
            }
        }
        let allocations = ToolAllocations::prepare(
            [ToolSlots {
                tool: stock.id,
                quantity: stock.quantity,
                last_granted: stock.last_granted,
            }],
            uses,
        )?;
        if let Some(body) = tx.request(game_physiology::services::ReadBody(request.actor))? {
            let stationary = tx
                .request(game_transport::services::ReadPresence(request.actor))?
                .and_then(|presence| match presence.location {
                    Location::AtArea(area) => Some(area),
                    _ => None,
                });
            crate::tool_available(
                request.actor,
                &stock,
                ToolBodyFacts {
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                },
                stationary,
                area,
                &allocations,
                false,
            )
        } else {
            false
        }
    } else {
        false
    };
    Ok(StockBudget {
        unreserved,
        tool_allocated,
    })
}

pub fn contact<E: 'static>(
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    actor: PersonId,
    stock: &Inventory,
) -> Result<bool, Error> {
    let Some(presence) = tx.request(game_transport::services::ReadPresence(actor))? else {
        return Ok(false);
    };
    match stock.location {
        InventoryLocation::Area(area) => Ok(presence.resolved == Location::AtArea(area)),
        InventoryLocation::Carried(holder) => tx.request(game_transport::systems::ReadContact {
            first: actor,
            second: holder,
        }),
    }
}
pub fn accessible<E: 'static>(
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    actor: PersonId,
    stock: &Inventory,
) -> Result<bool, Error> {
    Ok(crate::effective_access(
        stock.access.contains(&actor),
        tx.request(ReadStockPermission {
            actor,
            inventory: stock.id,
        })?,
    ))
}
pub fn read_stock<E: 'static>(
    owner: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: game_physiology::systems::ReadStock,
) -> Result<Option<game_physiology::systems::StockFacts>, Error> {
    let Some(stock) = tx.read::<Inventory>(StableId(request.source.0)).cloned() else {
        return Ok(None);
    };
    if stock.id != request.source {
        return Err(Error::Unavailable);
    }
    stock.validate()?;
    let budget = read_budget(
        owner,
        tx,
        ReadStockBudget {
            actor: request.actor,
            inventory: request.source,
            additional_reservations: vec![],
        },
    )?;
    Ok(Some(game_physiology::systems::StockFacts {
        good: stock.good,
        quantity: stock.quantity,
        unreserved: budget.unreserved,
        accessible: accessible(tx, request.actor, &stock)?,
        contact: contact(tx, request.actor, &stock)?,
    }))
}
pub fn read_mass<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: game_transport::systems::ReadCargoMass,
) -> Result<Quantity, Error> {
    let stocks = tx.iter::<Inventory>();
    crate::carried_quantity_rows(stocks.iter().filter_map(|(_, stock)| {
        (stock.location == InventoryLocation::Carried(request.0)).then_some(stock)
    }))
}
pub fn read_local_stock<E: 'static>(
    _: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: game_characters::services::ReadLocalStock,
) -> Result<game_characters::services::LocalStock, Error> {
    let mut result = game_characters::services::LocalStock::default();
    for (_, stock) in tx.iter::<Inventory>() {
        let ground = request.at_area && stock.location == InventoryLocation::Area(request.area);
        let carried = stock.location == InventoryLocation::Carried(request.actor);
        if !ground && !carried {
            continue;
        }
        let observation = game_characters::perception::StockObservationFacts {
            inventory: stock.id,
            good: stock.good,
            quantity: stock.quantity,
            accessible: accessible(tx, request.actor, &stock)?,
        };
        if ground {
            result.ground.push(observation);
        } else {
            result.carried.push(observation);
        }
    }
    Ok(result)
}
pub fn read_work_equipment<E: 'static>(
    owner: &Services,
    tx: &mut OwnerReadTxn<'_, '_, Services, E>,
    request: game_characters::services::ReadWorkEquipment,
) -> Result<bool, Error> {
    let Some(tool) = tx
        .read::<ToolClaim>(StableId(request.0.0))
        .map(|claim| claim.tool)
    else {
        return Ok(true);
    };
    let Some(stock) = tx.read::<Inventory>(StableId(tool.0)).cloned() else {
        return Ok(false);
    };
    Ok(accessible(tx, request.0, &stock)?
        && read_budget(
            owner,
            tx,
            ReadStockBudget {
                actor: request.0,
                inventory: tool,
                additional_reservations: vec![],
            },
        )?
        .tool_allocated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::{SimDuration, SimTime, Vitals};
    use game_physiology::{
        Body, Rules, World,
        services::{IntakeTask, SetIntakeTask},
    };
    use game_transport::services::{Loading, SetLoading};
    use game_world::GenericHost;
    use std::collections::BTreeSet;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct State {
        driver: (),
        physical: Option<World>,
        traits: Option<game_physiology::traits::Runtime>,
        transport: (),
        cargo: (),
    }
    struct Driver;
    impl OwnerSystem<()> for Driver {
        type Runtime = ();
        type Input = ();
        type Output = (Quantity, Quantity, bool, bool);
        fn execute(
            &self,
            tx: &mut OwnerTxn<'_, '_, Self, ()>,
            _: (),
        ) -> Result<Self::Output, Error> {
            tx.request(SetIntakeTask {
                actor: PersonId(2),
                task: Some(IntakeTask {
                    source: InventoryId(3),
                    completes: SimTime(10),
                }),
            })?;
            tx.request(SetLoading {
                actor: PersonId(1),
                loading: Some(Loading {
                    source: InventoryId(3),
                    destination: InventoryId(4),
                    quantity: Quantity(400),
                    completes: SimTime(10),
                }),
            })?;
            tx.request(SetToolClaim {
                actor: PersonId(1),
                claim: Some(ToolClaim {
                    tool: InventoryId(5),
                }),
            })?;
            tx.request(SetToolClaim {
                actor: PersonId(2),
                claim: Some(ToolClaim {
                    tool: InventoryId(5),
                }),
            })?;
            let food_first = tx.request(ReadStockBudget {
                actor: PersonId(1),
                inventory: InventoryId(3),
                additional_reservations: vec![Reservation {
                    actor: PersonId(2),
                    source: InventoryId(3),
                    quantity: Quantity(100),
                }],
            })?;
            let food_second = tx.request(ReadStockBudget {
                actor: PersonId(2),
                inventory: InventoryId(3),
                additional_reservations: vec![Reservation {
                    actor: PersonId(2),
                    source: InventoryId(3),
                    quantity: Quantity(100),
                }],
            })?;
            let tool_first = tx.request(ReadStockBudget {
                actor: PersonId(1),
                inventory: InventoryId(5),
                additional_reservations: vec![],
            })?;
            let tool_second = tx.request(ReadStockBudget {
                actor: PersonId(2),
                inventory: InventoryId(5),
                additional_reservations: vec![],
            })?;
            Ok((
                food_first.unreserved,
                food_second.unreserved,
                tool_first.tool_allocated,
                tool_second.tool_allocated,
            ))
        }
    }
    #[test]
    fn provisional_native_claims_preserve_actor_exclusion_and_tool_cursor_order() {
        let mut schema = Schema::default();
        crate::register_components(&mut schema).unwrap();
        register_components(&mut schema).unwrap();
        game_physiology::register_components(&mut schema).unwrap();
        game_physiology::services::register_task_components(&mut schema).unwrap();
        game_transport::services::register_task_components(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        let rules = Rules {
            hunger_harm: 1_000,
            fatigue_limit: 1_000,
            max_health: 100,
            meal_relief: 200,
            meal_size: Quantity(200),
            eating_time: SimDuration(10),
            loading_time: SimDuration(10),
            carrying_capacity: Quantity(2_000),
        };
        let mut host = GenericHost::new(
            schema,
            1,
            State {
                driver: (),
                physical: Some(World { rules }),
                traits: None,
                transport: (),
                cargo: (),
            },
        );
        for _ in 0..5 {
            host.allocate_id().unwrap();
        }
        for actor in [1, 2] {
            host.storage
                .insert_or_spawn(
                    StableId(actor),
                    Body {
                        vitals: Vitals {
                            alive: true,
                            health: 100,
                            hunger: 0,
                            fatigue: 0,
                        },
                        fatigue_remainder: 0,
                        health_remainder: 0,
                    },
                )
                .unwrap();
            host.storage
                .insert(StableId(actor), Location::AtArea(AreaId(9)))
                .unwrap();
        }
        for (id, good, quantity, last_granted) in [
            (3, Good::Food, 1_000, None),
            (4, Good::Food, 0, None),
            (5, Good::Tool, 1_000, Some(PersonId(1))),
        ] {
            host.storage
                .insert_or_spawn(
                    StableId(id),
                    Inventory {
                        id: InventoryId(id),
                        location: InventoryLocation::Area(AreaId(9)),
                        quantity: Quantity(quantity),
                        capacity: Quantity(2_000),
                        access: BTreeSet::from([PersonId(1), PersonId(2)]),
                        last_granted,
                        good,
                        spoils_at: None,
                    },
                )
                .unwrap();
        }
        let mut registry = Registry::new();
        registry
            .register(
                Driver,
                RuntimeSlot::new(|state: &State| &state.driver, |state| &mut state.driver),
            )
            .unwrap();
        game_physiology::services::register_services(
            &mut registry,
            RuntimeSlot::new(|state: &State| &state.physical, |state| &mut state.physical),
        )
        .unwrap();
        game_physiology::services::register_trait_services(
            &mut registry,
            RuntimeSlot::new(|state: &State| &state.traits, |state| &mut state.traits),
        )
        .unwrap();
        game_transport::services::register_services(
            &mut registry,
            RuntimeSlot::new(
                |state: &State| &state.transport,
                |state| &mut state.transport,
            ),
        )
        .unwrap();
        register_services(
            &mut registry,
            RuntimeSlot::new(|state: &State| &state.cargo, |state| &mut state.cargo),
        )
        .unwrap();
        let prepared = registry
            .prepare(&host, SimTime(0), None, |tx| tx.call::<Driver>(()))
            .unwrap();
        assert_eq!(
            registry.commit(&mut host, prepared).unwrap(),
            (Quantity(700), Quantity(600), false, true)
        );
        assert_eq!(
            crate::inventory(&host.storage, InventoryId(3))
                .unwrap()
                .quantity,
            Quantity(1_000)
        );
    }
}

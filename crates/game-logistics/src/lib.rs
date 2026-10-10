//! Goods acquisition and delivery planning from personal knowledge and actual
//! carrying facts. Execution and consumption remain with their rule owners.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type KnownRoutes = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    pub good: Good,
    pub quantity: Quantity,
    pub source: Option<InventoryId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    Inventory(InventoryId),
    Contact(PersonId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockReason {
    UnknownDestination,
    MissingContact,
    DeniedDestination,
    DestinationFull,
    NoSupply,
    Capacity,
    CannotTravel,
    UnknownRoute,
    PrerequisiteCycle,
    PrerequisiteLimit,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step<A> {
    Acquire(A),
    Carry {
        source: InventoryId,
        quantity: Quantity,
    },
    Travel(Location),
    Transfer {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Arrived {
        source: InventoryId,
        quantity: Quantity,
    },
    Continue,
    Complete,
    Blocked(BlockReason),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub requirement: Requirement,
    pub destination: Destination,
    pub remaining: Quantity,
    pub last_receipt: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cargo {
    pub inventory: InventoryId,
    pub good: Good,
    /// Actual usable quantity, not a remembered balance or a second stock ledger.
    pub quantity: Quantity,
    /// Supplied from actual acquisition/provenance facts for source-constrained work.
    pub origin: Option<InventoryId>,
    pub origin_quantity: Quantity,
    pub room: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownStock {
    pub inventory: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
    pub location: Location,
    pub observed_at: SimTime,
    pub room: Option<Quantity>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownDestination {
    pub destination: Destination,
    /// Frozen remembered position; a moving observation is frozen by transport.
    pub location: Location,
    pub observed_at: SimTime,
    pub accessible: bool,
    /// Dated receiving capacity. None means capacity has not been learned.
    pub room: Option<Quantity>,
    /// The consumer can act on an accessible local source without first
    /// moving it into the actor's cargo. This is an execution capability fact.
    pub direct_transfer: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionOffer<A> {
    /// Stable provider key, independent of input order or worker completion.
    pub key: u64,
    pub good: Good,
    pub source: Option<InventoryId>,
    pub location: Location,
    pub observed_at: SimTime,
    /// Eligibility to attempt the opaque action, never a promise of future yield.
    pub accessible: bool,
    /// Prospective bounded yield. Zero permits a preparation such as asking access.
    pub quantity: OfferYield,
    /// Actual effect placement: an operator storing its yield in own cargo
    /// requires room; local-store production and preparation do not.
    pub requires_carrying_room: bool,
    pub requirements: Vec<Prerequisite>,
    pub action: A,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prerequisite {
    pub requirement: Requirement,
    pub placement: PrerequisitePlacement,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrerequisitePlacement {
    Carried,
    AccessibleAtProvider { inventory: Option<InventoryId> },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferYield {
    Known(Quantity),
    Unknown,
}
pub struct Input<'a, A> {
    pub now: SimTime,
    pub place: &'a Location,
    pub free: bool,
    pub can_travel: bool,
    /// Remaining room for this requirement's Good, computed by physical owners.
    pub room: Quantity,
    /// Physical-owner facts for other Goods in the declared prerequisite closure.
    pub prerequisite_room: &'a BTreeMap<Good, Quantity>,
    pub minimum_handoff: Quantity,
    pub cargo: &'a [Cargo],
    pub stocks: &'a [KnownStock],
    pub destination: Option<&'a KnownDestination>,
    pub contacts: &'a BTreeSet<PersonId>,
    pub routes: &'a KnownRoutes,
    pub offers: &'a [AcquisitionOffer<A>],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub sequence: u64,
    pub destination: Destination,
    pub good: Good,
    pub quantity: Quantity,
}
impl Delivery {
    pub fn valid(&self) -> bool {
        self.requirement.quantity.0 > 0
            && self.remaining <= self.requirement.quantity
            && self.requirement.source.is_none_or(|id| id.0 > 0)
            && !matches!(
                self.destination,
                Destination::Inventory(InventoryId(0)) | Destination::Contact(PersonId(0))
            )
    }
    pub fn new(requirement: Requirement, destination: Destination) -> Result<Self, Error> {
        if requirement.quantity.0 == 0
            || requirement.source.is_some_and(|id| id.0 == 0)
            || matches!(
                destination,
                Destination::Inventory(InventoryId(0)) | Destination::Contact(PersonId(0))
            )
        {
            return Err(Error::Unavailable);
        }
        Ok(Self {
            remaining: requirement.quantity,
            requirement,
            destination,
            last_receipt: 0,
        })
    }
    /// Only an actual handoff/consumption receipt fulfills a delivery. Arrival,
    /// promises, carrying and acquisition forecasts cannot do so.
    pub fn delivered(&mut self, receipt: &DeliveryReceipt) -> Result<bool, Error> {
        if receipt.sequence == 0
            || receipt.quantity.0 == 0
            || receipt.destination != self.destination
            || receipt.good != self.requirement.good
        {
            return Err(Error::Unavailable);
        }
        if receipt.sequence <= self.last_receipt {
            return Ok(false);
        }
        self.last_receipt = receipt.sequence;
        self.remaining = Quantity(self.remaining.0.saturating_sub(receipt.quantity.0));
        Ok(true)
    }
}
impl Delivery {
    pub fn plan<A: Clone>(&self, input: &Input<'_, A>) -> Step<A> {
        if self.remaining.0 == 0 {
            return Step::Complete;
        }
        if input.minimum_handoff.0 == 0 {
            return Step::Blocked(BlockReason::Capacity);
        }
        if !input.free
            || matches!(
                input.place,
                Location::InTransit { .. } | Location::Carried { .. }
            )
        {
            return Step::Continue;
        }
        let Some(destination) = input.destination.filter(|known| {
            known.destination == self.destination
                && known.observed_at <= input.now
                && stationary(&known.location)
        }) else {
            return Step::Blocked(BlockReason::UnknownDestination);
        };
        if !destination.accessible {
            return Step::Blocked(BlockReason::DeniedDestination);
        }
        if let Some(cargo) = input
            .cargo
            .iter()
            .filter(|cargo| cargo_quantity(cargo, &self.requirement) >= input.minimum_handoff)
            .min_by_key(|cargo| cargo.inventory)
        {
            let quantity = Quantity(
                cargo_quantity(cargo, &self.requirement)
                    .0
                    .min(self.remaining.0.max(input.minimum_handoff.0)),
            );
            if same_place(input.place, &destination.location) {
                return handoff(
                    self.destination,
                    destination,
                    input.contacts,
                    cargo.inventory,
                    quantity,
                    input.minimum_handoff,
                );
            }
            return travel(input, &destination.location);
        }
        if same_place(input.place, &destination.location) {
            if matches!(self.destination,Destination::Contact(person) if !input.contacts.contains(&person))
            {
                return Step::Blocked(BlockReason::MissingContact);
            }
            // A dated stock is a lead for a direct attempt, never credited cargo.
            // The executor must revalidate its current balance and permissions.
            if let Some(stock)=input.stocks.iter().filter(|stock| {
                stock.good == self.requirement.good && stock.accessible
                    && stock.quantity >= input.minimum_handoff && stock.observed_at <= input.now
                    && same_place(&stock.location,&destination.location)
                    && self.requirement.source.is_none_or(|source|source==stock.inventory)
                    && match self.destination {
                        Destination::Contact(_) => destination.direct_transfer,
                        Destination::Inventory(target) => target==stock.inventory || destination.direct_transfer,
                    }
                    && !input.cargo.iter().any(|cargo|cargo.inventory==stock.inventory)
                    && (!matches!(self.destination,Destination::Inventory(target) if target==stock.inventory)
                        || stock.observed_at==input.now)
            }).min_by_key(|stock|stock.inventory) {
                let quantity=Quantity(stock.quantity.0.min(self.remaining.0.max(input.minimum_handoff.0)));
                return handoff(self.destination,destination,input.contacts,stock.inventory,quantity,input.minimum_handoff);
            }
        }
        if matches!(self.destination, Destination::Inventory(_))
            && destination.room == Some(Quantity(0))
        {
            return Step::Blocked(BlockReason::DestinationFull);
        }
        let matching: Vec<_> = input
            .cargo
            .iter()
            .filter(|cargo| cargo_quantity(cargo, &self.requirement).0 > 0)
            .collect();
        let held = matching
            .iter()
            .map(|cargo| u128::from(cargo_quantity(cargo, &self.requirement).0))
            .sum::<u128>();
        if held >= u128::from(input.minimum_handoff.0) {
            if let Some(target) =
                matching
                    .iter()
                    .filter(|cargo| cargo.room.0 > 0)
                    .max_by_key(|cargo| {
                        (
                            cargo_quantity(cargo, &self.requirement),
                            std::cmp::Reverse(cargo.inventory),
                        )
                    })
                && let Some(source) = matching
                    .iter()
                    .filter(|cargo| cargo.inventory != target.inventory)
                    .min_by_key(|cargo| cargo.inventory)
            {
                return Step::Transfer {
                    source: source.inventory,
                    destination: target.inventory,
                    quantity: Quantity(
                        cargo_quantity(source, &self.requirement)
                            .0
                            .min(target.room.0)
                            .min(
                                input.minimum_handoff.0
                                    - cargo_quantity(target, &self.requirement).0,
                            ),
                    ),
                };
            }
            return Step::Blocked(BlockReason::Capacity);
        }
        let needed = u128::from(self.remaining.0.max(input.minimum_handoff.0)).saturating_sub(held);
        let needed = u64::try_from(needed).unwrap_or(u64::MAX);
        let mut context = Composition {
            input,
            primary_good: self.requirement.good,
            path: Vec::new(),
            remaining: 128,
        };
        context.acquire(&self.requirement, needed, input.room)
    }
}
fn handoff<A>(
    target: Destination,
    known: &KnownDestination,
    contacts: &BTreeSet<PersonId>,
    source: InventoryId,
    quantity: Quantity,
    minimum: Quantity,
) -> Step<A> {
    match target {
        Destination::Contact(person) if contacts.contains(&person) => {
            Step::Arrived { source, quantity }
        }
        Destination::Contact(_) => Step::Blocked(BlockReason::MissingContact),
        Destination::Inventory(destination) if source == destination => {
            Step::Arrived { source, quantity }
        }
        Destination::Inventory(destination) => {
            let quantity = Quantity(quantity.0.min(known.room.unwrap_or(quantity).0));
            if quantity < minimum {
                Step::Blocked(BlockReason::DestinationFull)
            } else {
                Step::Transfer {
                    source,
                    destination,
                    quantity,
                }
            }
        }
    }
}
fn cargo_quantity(cargo: &Cargo, requirement: &Requirement) -> Quantity {
    if cargo.good != requirement.good {
        return Quantity(0);
    }
    match requirement.source {
        None => cargo.quantity,
        Some(source) if cargo.inventory == source => cargo.quantity,
        Some(source) if cargo.origin == Some(source) => cargo.quantity.min(cargo.origin_quantity),
        _ => Quantity(0),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct NeedKey {
    good: Good,
    source: Option<InventoryId>,
    placement: PrerequisitePlacement,
    provider: Option<Location>,
}
struct Composition<'a, 'b, A> {
    input: &'a Input<'b, A>,
    primary_good: Good,
    path: Vec<NeedKey>,
    remaining: usize,
}
impl<A: Clone> Composition<'_, '_, A> {
    fn room(&self, good: Good) -> Quantity {
        if good == self.primary_good {
            self.input.room
        } else {
            self.input
                .prerequisite_room
                .get(&good)
                .copied()
                .unwrap_or(Quantity(0))
        }
    }
    fn carried(&self, requirement: &Requirement) -> u128 {
        self.input
            .cargo
            .iter()
            .map(|cargo| u128::from(cargo_quantity(cargo, requirement).0))
            .sum()
    }
    fn acquire(&mut self, requirement: &Requirement, needed: u64, room: Quantity) -> Step<A> {
        self.acquire_for(
            requirement,
            needed,
            room,
            NeedKey {
                good: requirement.good,
                source: requirement.source,
                placement: PrerequisitePlacement::Carried,
                provider: None,
            },
        )
    }
    fn acquire_for(
        &mut self,
        requirement: &Requirement,
        needed: u64,
        room: Quantity,
        key: NeedKey,
    ) -> Step<A> {
        enum Source<'a, A> {
            Stock(&'a KnownStock),
            Offer(&'a AcquisitionOffer<A>),
        }
        let input = self.input;
        let mut sources = Vec::new();
        for stock in input.stocks.iter().filter(|stock| {
            room.0 > 0
                && !input
                    .cargo
                    .iter()
                    .any(|cargo| cargo.inventory == stock.inventory)
                && stock.good == requirement.good
                && stock.accessible
                && stock.quantity.0 > 0
                && stock.observed_at <= input.now
                && requirement
                    .source
                    .is_none_or(|source| source == stock.inventory)
        }) {
            if let Some(cost) = travel_cost(input.routes, input.place, &stock.location) {
                sources.push(((false, cost, 0, stock.inventory.0), Source::Stock(stock)));
            }
        }
        for offer in input.offers.iter().filter(|offer| {
            offer.good == requirement.good
                && offer.accessible
                && offer.observed_at <= input.now
                && (!offer.requires_carrying_room
                    || match offer.quantity {
                        OfferYield::Known(quantity) => quantity <= room,
                        OfferYield::Unknown => room.0 > 0,
                    })
                && requirement.source.is_none_or(|source| {
                    offer.source == Some(source) && offer.quantity != OfferYield::Unknown
                })
        }) {
            if let Some(cost) = travel_cost(input.routes, input.place, &offer.location) {
                sources.push((
                    (offer.quantity == OfferYield::Unknown, cost, 1, offer.key),
                    Source::Offer(offer),
                ));
            }
        }
        sources.sort_by_key(|(key, _)| *key);
        let mut blocked = if room.0 == 0 {
            BlockReason::Capacity
        } else {
            BlockReason::NoSupply
        };
        for (_, source) in sources {
            if self.remaining == 0 {
                return Step::Blocked(BlockReason::PrerequisiteLimit);
            }
            self.remaining -= 1;
            match source {
                Source::Stock(stock) => {
                    return if same_place(input.place, &stock.location) {
                        Step::Carry {
                            source: stock.inventory,
                            quantity: Quantity(needed.min(stock.quantity.0).min(room.0)),
                        }
                    } else {
                        travel(input, &stock.location)
                    };
                }
                Source::Offer(offer) => {
                    if self.path.contains(&key) {
                        blocked = BlockReason::PrerequisiteCycle;
                        continue;
                    }
                    if self.path.len() >= 8 {
                        blocked = BlockReason::PrerequisiteLimit;
                        continue;
                    }
                    self.path.push(key.clone());
                    let mut prerequisite_step = None;
                    let mut prerequisite_block = None;
                    for prerequisite in &offer.requirements {
                        match self.fulfill(prerequisite, &offer.location) {
                            Some(Step::Blocked(reason)) => {
                                prerequisite_block.get_or_insert(reason);
                            }
                            Some(step) => {
                                prerequisite_step = Some(step);
                                break;
                            }
                            None => {}
                        }
                    }
                    if prerequisite_step.is_none() {
                        prerequisite_step = prerequisite_block.map(Step::Blocked);
                    }
                    self.path.pop();
                    if let Some(step) = prerequisite_step {
                        if let Step::Blocked(reason) = step {
                            blocked = reason;
                            continue;
                        }
                        return step;
                    }
                    return if same_place(input.place, &offer.location) {
                        Step::Acquire(offer.action.clone())
                    } else {
                        travel(input, &offer.location)
                    };
                }
            }
        }
        Step::Blocked(blocked)
    }
    /// None means the declared prerequisite is satisfied by the current facts;
    /// a step never assumes that an earlier acquisition or transfer succeeded.
    fn fulfill(&mut self, prerequisite: &Prerequisite, provider: &Location) -> Option<Step<A>> {
        let requirement = &prerequisite.requirement;
        if requirement.quantity.0 == 0 {
            return Some(Step::Blocked(BlockReason::NoSupply));
        }
        let carried = self.carried(requirement);
        match prerequisite.placement {
            PrerequisitePlacement::Carried => {
                if carried >= u128::from(requirement.quantity.0) {
                    return None;
                }
                Some(self.acquire(
                    requirement,
                    (u128::from(requirement.quantity.0) - carried) as u64,
                    self.room(requirement.good),
                ))
            }
            PrerequisitePlacement::AccessibleAtProvider { inventory } => {
                let input = self.input;
                let own_target =
                    inventory.and_then(|id| input.cargo.iter().find(|cargo| cargo.inventory == id));
                let target = inventory.and_then(|id| {
                    input
                        .stocks
                        .iter()
                        .find(|stock| stock.inventory == id && stock.observed_at <= input.now)
                });
                if own_target.is_none()
                    && let Some(target) = target
                    && (!target.accessible || target.good != requirement.good)
                {
                    return Some(Step::Blocked(BlockReason::DeniedDestination));
                }
                let local_stocks = input
                    .stocks
                    .iter()
                    .filter(|stock| {
                        stock.good == requirement.good
                            && stock.accessible
                            && stock.observed_at <= input.now
                            && same_place(&stock.location, provider)
                            && inventory.is_none_or(|id| stock.inventory == id)
                            && requirement
                                .source
                                .is_none_or(|source| stock.inventory == source)
                            && !input
                                .cargo
                                .iter()
                                .any(|cargo| cargo.inventory == stock.inventory)
                    })
                    .map(|stock| u128::from(stock.quantity.0))
                    .sum::<u128>();
                let usable_carried = if inventory.is_none() {
                    carried
                } else {
                    input
                        .cargo
                        .iter()
                        .filter(|cargo| Some(cargo.inventory) == inventory)
                        .map(|cargo| u128::from(cargo_quantity(cargo, requirement).0))
                        .sum()
                };
                if local_stocks >= u128::from(requirement.quantity.0) {
                    return None;
                }
                if local_stocks + usable_carried >= u128::from(requirement.quantity.0) {
                    return if same_place(input.place, provider) {
                        None
                    } else {
                        Some(travel(input, provider))
                    };
                }
                let missing = (u128::from(requirement.quantity.0)
                    .saturating_sub(local_stocks + usable_carried))
                    as u64;
                if let Some(id) = inventory {
                    if own_target.is_none() && target.is_none() {
                        return Some(Step::Blocked(BlockReason::UnknownDestination));
                    }
                    if own_target.is_none()
                        && target.is_some_and(|target| !same_place(&target.location, provider))
                    {
                        return Some(Step::Blocked(BlockReason::UnknownDestination));
                    }
                    if let Some(source) = input
                        .cargo
                        .iter()
                        .filter(|cargo| {
                            cargo.inventory != id && cargo_quantity(cargo, requirement).0 > 0
                        })
                        .min_by_key(|cargo| cargo.inventory)
                    {
                        let location = if own_target.is_some() {
                            input.place
                        } else {
                            provider
                        };
                        if !same_place(input.place, location) {
                            return Some(travel(input, location));
                        }
                        let quantity = Quantity(
                            cargo_quantity(source, requirement).0.min(missing).min(
                                own_target
                                    .map_or_else(
                                        || {
                                            target
                                                .and_then(|target| target.room)
                                                .unwrap_or(Quantity(missing))
                                        },
                                        |target| target.room,
                                    )
                                    .0,
                            ),
                        );
                        if quantity.0 == 0 {
                            return Some(Step::Blocked(BlockReason::DestinationFull));
                        }
                        return Some(Step::Transfer {
                            source: source.inventory,
                            destination: id,
                            quantity,
                        });
                    }
                } else if carried >= u128::from(missing) {
                    return if same_place(input.place, provider) {
                        None
                    } else {
                        Some(travel(input, provider))
                    };
                }
                let needed =
                    u128::from(requirement.quantity.0).saturating_sub(local_stocks + carried);
                Some(self.acquire_for(
                    requirement,
                    u64::try_from(needed).unwrap_or(u64::MAX),
                    self.room(requirement.good),
                    NeedKey {
                        good: requirement.good,
                        source: requirement.source,
                        placement: prerequisite.placement.clone(),
                        provider: Some(provider.clone()),
                    },
                ))
            }
        }
    }
}
fn travel<A>(input: &Input<'_, A>, destination: &Location) -> Step<A> {
    if !input.can_travel {
        return Step::Blocked(BlockReason::CannotTravel);
    }
    if travel_cost(input.routes, input.place, destination).is_none() {
        return Step::Blocked(BlockReason::UnknownRoute);
    }
    Step::Travel(destination.clone())
}
fn stationary(location: &Location) -> bool {
    matches!(location, Location::AtArea(_)) || road(location).is_some_and(|point| point.valid())
}
fn road(location: &Location) -> Option<RoadPosition> {
    if let Location::StoppedOnRoad {
        from,
        to,
        progress,
        duration,
    } = *location
    {
        Some(RoadPosition {
            from,
            to,
            progress,
            duration,
        })
    } else {
        None
    }
}
fn same_place(first: &Location, second: &Location) -> bool {
    if let (Location::AtArea(a), Location::AtArea(b)) = (first, second) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (road(first), road(second)) {
        if !a.valid() || !b.valid() {
            return false;
        }
        let bp = if a.from == b.from && a.to == b.to {
            b.progress.0
        } else if a.from == b.to && a.to == b.from {
            b.duration.0 - b.progress.0
        } else {
            return false;
        };
        return u128::from(a.progress.0) * u128::from(b.duration.0)
            == u128::from(bp) * u128::from(a.duration.0);
    }
    false
}
fn area_cost(routes: &KnownRoutes, from: AreaId, to: AreaId) -> Option<u64> {
    let mut frontier = BTreeSet::from([(0_u64, from)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == to {
            return Some(cost);
        }
        for (next, duration) in routes.get(&area).into_iter().flatten() {
            if duration.0 > 0
                && let Some(total) = cost.checked_add(duration.0)
            {
                frontier.insert((total, *next));
            }
        }
    }
    None
}
fn fraction(duration: u64, numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 {
        return None;
    }
    u64::try_from((u128::from(duration) * u128::from(numerator)).div_ceil(u128::from(denominator)))
        .ok()
}
fn endpoint_cost(routes: &KnownRoutes, point: RoadPosition, endpoint: AreaId) -> Option<u64> {
    if !point.valid() {
        return None;
    }
    let (from, to, distance) = if endpoint == point.from {
        (point.to, point.from, point.progress.0)
    } else if endpoint == point.to {
        (point.from, point.to, point.duration.0 - point.progress.0)
    } else {
        return None;
    };
    if distance == 0 {
        return Some(0);
    }
    let duration = routes.get(&from)?.get(&to)?.0;
    fraction(duration, distance, point.duration.0)
}
fn approach_cost(routes: &KnownRoutes, endpoint: AreaId, point: RoadPosition) -> Option<u64> {
    if !point.valid() {
        return None;
    }
    let (to, distance) = if endpoint == point.from {
        (point.to, point.progress.0)
    } else if endpoint == point.to {
        (point.from, point.duration.0 - point.progress.0)
    } else {
        return None;
    };
    if distance == 0 {
        return Some(0);
    }
    fraction(
        routes.get(&endpoint)?.get(&to)?.0,
        distance,
        point.duration.0,
    )
}
/// Reachability uses only the supplied remembered graph. Geometry identifies
/// frozen road points; physical travel timing and progress remain transport-owned.
fn travel_cost(routes: &KnownRoutes, from: &Location, to: &Location) -> Option<u64> {
    if same_place(from, to) {
        return Some(0);
    }
    match (from, to) {
        (Location::AtArea(a), Location::AtArea(b)) => area_cost(routes, *a, *b),
        (Location::AtArea(a), _) => {
            let point = road(to)?;
            [point.from, point.to]
                .into_iter()
                .filter_map(|endpoint| {
                    area_cost(routes, *a, endpoint)?
                        .checked_add(approach_cost(routes, endpoint, point)?)
                })
                .min()
        }
        (_, Location::AtArea(b)) => {
            let point = road(from)?;
            [point.from, point.to]
                .into_iter()
                .filter_map(|endpoint| {
                    endpoint_cost(routes, point, endpoint)?
                        .checked_add(area_cost(routes, endpoint, *b)?)
                })
                .min()
        }
        _ => {
            let a = road(from)?;
            let b = road(to)?;
            [a.from, a.to]
                .into_iter()
                .flat_map(|start| {
                    [b.from, b.to].into_iter().filter_map(move |end| {
                        endpoint_cost(routes, a, start)?
                            .checked_add(area_cost(routes, start, end)?)?
                            .checked_add(approach_cost(routes, end, b)?)
                    })
                })
                .min()
        }
    }
}
#[cfg(test)]
mod tests;

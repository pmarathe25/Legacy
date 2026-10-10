//! Physical transport rules. Bodies, consent, paths, and supplies are explicit
//! inputs; this crate does not know simulation storage or political authority.
use game_core::{AreaId, Error, Location, PersonId, Quantity, SimDuration, SimTime};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub adult_mass: Quantity,
    pub newborn_mass: Quantity,
    pub max_bearers: usize,
    pub offer_lifetime: SimDuration,
    pub lift_time: SimDuration,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            adult_mass: Quantity(60_000),
            newborn_mass: Quantity(3_000),
            max_bearers: 8,
            offer_lifetime: SimDuration(900_000),
            lift_time: SimDuration(30_000),
        }
    }
}
impl Rules {
    pub fn valid(&self) -> bool {
        self.newborn_mass.0 > 0
            && self.adult_mass >= self.newborn_mass
            && self.adult_mass.0 <= 1_000_000
            && (1..=16).contains(&self.max_bearers)
            && self.offer_lifetime.0 > 0
            && self.lift_time.0 > 0
    }
}
pub fn body_mass(rules: &Rules, age: SimDuration, adulthood: SimDuration) -> Quantity {
    let maturity = age.0.min(adulthood.0);
    let growth = u128::from(rules.adult_mass.0 - rules.newborn_mass.0) * u128::from(maturity)
        / u128::from(adulthood.0.max(1));
    Quantity(rules.newborn_mass.0 + growth as u64)
}

pub use game_core::RoadPosition;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Position {
    Area(AreaId),
    Road(RoadPosition),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PositionKey {
    Area(AreaId),
    Road {
        from: AreaId,
        to: AreaId,
        numerator: u64,
        denominator: u64,
    },
}
impl Position {
    pub fn key(self) -> PositionKey {
        match self {
            Self::Area(area) => PositionKey::Area(area),
            Self::Road(p) => {
                if p.progress.0 == 0 {
                    return PositionKey::Area(p.from);
                }
                if p.progress == p.duration {
                    return PositionKey::Area(p.to);
                }
                let (from, to, progress) = if p.from < p.to {
                    (p.from, p.to, p.progress.0)
                } else {
                    (p.to, p.from, p.duration.0 - p.progress.0)
                };
                let (mut a, mut b) = (progress, p.duration.0);
                while b != 0 {
                    (a, b) = (b, a % b);
                }
                PositionKey::Road {
                    from,
                    to,
                    numerator: progress / a,
                    denominator: p.duration.0 / a,
                }
            }
        }
    }
    pub fn same_point(self, other: Self) -> bool {
        self.key() == other.key()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leg {
    pub from: AreaId,
    pub to: AreaId,
    pub departed: SimTime,
    pub arrives: SimTime,
    pub road_start: Option<RoadPosition>,
}
impl Leg {
    pub fn position(&self, now: SimTime) -> Result<Position, Error> {
        let duration = self
            .arrives
            .0
            .checked_sub(self.departed.0)
            .ok_or(Error::Overflow)?;
        if duration == 0 || self.from == self.to || now < self.departed {
            return Err(Error::InvalidConfiguration(
                "invalid journey interval".into(),
            ));
        }
        if now >= self.arrives {
            return Ok(Position::Area(self.to));
        }
        let elapsed = now.0 - self.departed.0;
        if let Some(start) = self.road_start {
            if !start.valid()
                || !((self.from == start.from && self.to == start.to)
                    || (self.from == start.to && self.to == start.from))
            {
                return Err(Error::InvalidConfiguration(
                    "invalid partial road journey".into(),
                ));
            }
            let progress = if self.to == start.to {
                start.progress.0
                    + (u128::from(start.duration.0 - start.progress.0) * u128::from(elapsed)
                        / u128::from(duration)) as u64
            } else {
                start.progress.0
                    - (u128::from(start.progress.0) * u128::from(elapsed) / u128::from(duration))
                        as u64
            };
            Ok(Position::Road(RoadPosition {
                progress: SimDuration(progress),
                ..start
            }))
        } else {
            Ok(Position::Road(RoadPosition {
                from: self.from,
                to: self.to,
                progress: SimDuration(elapsed),
                duration: SimDuration(duration),
            }))
        }
    }
}
pub fn resume(position: RoadPosition, now: SimTime) -> Result<Leg, Error> {
    resume_on_link(position, position.duration, now)
}
pub fn resume_on_link(
    position: RoadPosition,
    actual_duration: SimDuration,
    now: SimTime,
) -> Result<Leg, Error> {
    retreat(position, position.to, actual_duration, now)
}
pub fn remaining_duration(
    position: RoadPosition,
    from: AreaId,
    to: AreaId,
    actual_duration: SimDuration,
) -> Result<SimDuration, Error> {
    if !position.valid() || actual_duration.0 == 0 {
        return Err(Error::Unavailable);
    }
    let remaining = if from == position.from && to == position.to {
        position.duration.0 - position.progress.0
    } else if from == position.to && to == position.from {
        position.progress.0
    } else {
        return Err(Error::Unavailable);
    };
    let elapsed = (u128::from(remaining) * u128::from(actual_duration.0))
        .div_ceil(u128::from(position.duration.0));
    Ok(SimDuration(
        u64::try_from(elapsed).map_err(|_| Error::Overflow)?,
    ))
}
pub fn retreat(
    position: RoadPosition,
    destination: AreaId,
    actual_duration: SimDuration,
    now: SimTime,
) -> Result<Leg, Error> {
    if !position.valid() || actual_duration.0 == 0 {
        return Err(Error::Unavailable);
    }
    let (from, to, remaining) = if destination == position.to {
        (
            position.from,
            position.to,
            position.duration.0 - position.progress.0,
        )
    } else if destination == position.from {
        (position.to, position.from, position.progress.0)
    } else {
        return Err(Error::Unavailable);
    };
    if remaining == 0 {
        return Err(Error::AlreadyThere);
    }
    let elapsed = u128::from(remaining_duration(position, from, to, actual_duration)?.0);
    let arrives = SimTime(
        now.0
            .checked_add(u64::try_from(elapsed).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?,
    );
    Ok(Leg {
        from,
        to,
        departed: now,
        arrives,
        road_start: Some(position),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    ReturnToArea {
        destination: AreaId,
    },
    ApproachRoad {
        from: AreaId,
        to: AreaId,
        progress: SimDuration,
        duration: SimDuration,
    },
    OfferCarry {
        passenger: PersonId,
    },
    AcceptCarry {
        bearers: Vec<PersonId>,
    },
    RefuseCarry {
        bearer: PersonId,
    },
    JoinCarry {
        passenger: PersonId,
    },
    PutDown {
        passenger: PersonId,
    },
    WithdrawSupport {
        passenger: PersonId,
    },
    Stop,
    Resume,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Authority {
    Consented,
    UnableToConsent,
    Yielded,
    Captive,
    Newborn,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Permission {
    pub passenger_accepted: bool,
    pub unable_to_consent: bool,
    pub yielded: bool,
    pub captive: bool,
    pub newborn: bool,
}
impl Permission {
    pub fn authority(self) -> Result<Authority, Error> {
        if self.passenger_accepted {
            Ok(Authority::Consented)
        } else if self.newborn {
            Ok(Authority::Newborn)
        } else if self.captive {
            Ok(Authority::Captive)
        } else if self.unable_to_consent {
            Ok(Authority::UnableToConsent)
        } else if self.yielded {
            Ok(Authority::Yielded)
        } else {
            Err(Error::Unavailable)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryGroup {
    pub passenger: PersonId,
    pub anchor: PersonId,
    pub bearers: BTreeSet<PersonId>,
    pub authority: Authority,
    pub accepted_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub bearer: PersonId,
    pub passenger: PersonId,
    pub offered_at: SimTime,
    pub expires: SimTime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseKind {
    Accepted,
    Refused,
    WaitingForSupport,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub passenger: PersonId,
    pub bearer: PersonId,
    pub kind: ResponseKind,
    pub reason: String,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportRequest {
    pub passenger: PersonId,
    pub requester: PersonId,
    pub observed_at: SimTime,
    pub expires: SimTime,
}
#[derive(Clone, Copy, Debug)]
pub struct OfferChoice {
    pub need_help: bool,
    pub urgent_own_need: bool,
    pub busy: bool,
    pub trust: i32,
    pub affection: i32,
    pub independence_goal: bool,
    pub sufficient_support: bool,
}
pub fn judge_offer(choice: OfferChoice) -> (ResponseKind, &'static str) {
    if choice.busy {
        return (
            ResponseKind::Refused,
            "I am occupied with another physical duty",
        );
    }
    if choice.urgent_own_need {
        return (
            ResponseKind::Refused,
            "I need to address my immediate bodily needs first",
        );
    }
    if choice.trust < 0 || choice.trust.saturating_add(choice.affection) < 200 {
        return (
            ResponseKind::Refused,
            "I do not trust this offer enough to accept being carried",
        );
    }
    if !choice.need_help || choice.independence_goal {
        return (
            ResponseKind::Refused,
            "I prefer to travel under my own power",
        );
    }
    if !choice.sufficient_support {
        return (
            ResponseKind::WaitingForSupport,
            "I would welcome help, but this load needs more willing bearers",
        );
    }
    (
        ResponseKind::Accepted,
        "I accept help from these willing bearers",
    )
}
#[derive(Clone, Copy, Debug)]
pub struct SupportChoice {
    pub empathy: u16,
    pub trust: i32,
    pub affection: i32,
    pub own_need: bool,
    pub busy: bool,
    pub has_capacity: bool,
}
pub fn willing_support(choice: SupportChoice) -> bool {
    !choice.own_need
        && !choice.busy
        && choice.has_capacity
        && choice.trust >= 0
        && i64::from(choice.empathy) + i64::from(choice.trust) + i64::from(choice.affection.max(0))
            >= 700
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lift {
    pub passenger: PersonId,
    pub bearers: BTreeSet<PersonId>,
    pub authority: Authority,
    pub started: SimTime,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approach {
    pub target: RoadPosition,
    pub started: SimTime,
    pub completes: SimTime,
}
/// Only an endpoint with an actual traversable link can initiate an approach.
/// The desired point is represented on the original directed road, while the
/// returned leg can travel from either physical endpoint.
pub fn approach(
    target: RoadPosition,
    endpoint: AreaId,
    actual_duration: SimDuration,
    now: SimTime,
) -> Result<(Leg, Approach), Error> {
    let start = RoadPosition {
        progress: if endpoint == target.from {
            SimDuration(0)
        } else if endpoint == target.to {
            target.duration
        } else {
            return Err(Error::Unavailable);
        },
        ..target
    };
    approach_from(start, target, actual_duration, actual_duration, now)
}
pub fn approach_from(
    start: RoadPosition,
    target: RoadPosition,
    forward: SimDuration,
    reverse: SimDuration,
    now: SimTime,
) -> Result<(Leg, Approach), Error> {
    if !start.valid()
        || !target.valid()
        || !((start.from == target.from && start.to == target.to)
            || (start.from == target.to && start.to == target.from))
    {
        return Err(Error::Unavailable);
    }
    let numerator = if start.from == target.from {
        start.progress.0
    } else {
        start.duration.0 - start.progress.0
    };
    let a = u128::from(numerator) * u128::from(target.duration.0);
    let b = u128::from(target.progress.0) * u128::from(start.duration.0);
    if a == b {
        return Err(Error::AlreadyThere);
    }
    let (from, to, duration, distance) = if a < b {
        (target.from, target.to, forward, b - a)
    } else {
        (target.to, target.from, reverse, a - b)
    };
    if duration.0 == 0 {
        return Err(Error::NoRoute);
    }
    let denominator = u128::from(start.duration.0) * u128::from(target.duration.0);
    let elapsed = distance
        .checked_mul(u128::from(duration.0))
        .ok_or(Error::Overflow)?
        .div_ceil(denominator);
    let completes = SimTime(
        now.0
            .checked_add(u64::try_from(elapsed).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?,
    );
    let endpoint = retreat(start, to, duration, now)?;
    Ok((
        Leg {
            from,
            to,
            ..endpoint
        },
        Approach {
            target,
            started: now,
            completes,
        },
    ))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub groups: BTreeMap<PersonId, CarryGroup>,
    pub offers: BTreeMap<PersonId, BTreeMap<PersonId, Offer>>,
    pub lifts: BTreeMap<PersonId, Lift>,
    pub approaches: BTreeMap<PersonId, Approach>,
    pub resume_targets: BTreeMap<PersonId, AreaId>,
    pub paused_approaches: BTreeMap<PersonId, RoadPosition>,
    pub responses: Vec<Response>,
    pub known_support: BTreeMap<PersonId, BTreeMap<PersonId, SupportRequest>>,
}
impl World {
    pub fn new(rules: Rules) -> Result<Self, Error> {
        if !rules.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid transport rules".into(),
            ));
        }
        Ok(Self {
            rules,
            groups: BTreeMap::new(),
            offers: BTreeMap::new(),
            lifts: BTreeMap::new(),
            approaches: BTreeMap::new(),
            resume_targets: BTreeMap::new(),
            paused_approaches: BTreeMap::new(),
            responses: Vec::new(),
            known_support: BTreeMap::new(),
        })
    }
    pub fn component(&self, actor: PersonId) -> BTreeSet<PersonId> {
        let mut result = BTreeSet::from([actor]);
        loop {
            let before = result.len();
            for group in self.groups.values() {
                if result.contains(&group.passenger) || !result.is_disjoint(&group.bearers) {
                    result.insert(group.passenger);
                    result.extend(group.bearers.iter().copied());
                }
            }
            if before == result.len() {
                return result;
            }
        }
    }
    pub fn offer(
        &mut self,
        bearer: PersonId,
        passenger: PersonId,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
    ) -> Result<(), Error> {
        let a = bodies.get(&bearer).ok_or(Error::Unavailable)?;
        let b = bodies.get(&passenger).ok_or(Error::Unavailable)?;
        if bearer == passenger
            || !a.can_bear
            || !a.position.same_point(b.position)
            || self.groups.contains_key(&bearer)
            || self.busy(bearer)
            || self.busy(passenger)
        {
            return Err(Error::Unavailable);
        }
        let expires = SimTime(
            now.0
                .checked_add(self.rules.offer_lifetime.0)
                .ok_or(Error::Overflow)?,
        );
        let mut offers = self.offers.get(&passenger).cloned().unwrap_or_default();
        offers.retain(|_, offer| offer.expires > now);
        if offers.len() >= self.rules.max_bearers && !offers.contains_key(&bearer) {
            return Err(Error::Unavailable);
        }
        offers.insert(
            bearer,
            Offer {
                bearer,
                passenger,
                offered_at: now,
                expires,
            },
        );
        self.offers.insert(passenger, offers);
        Ok(())
    }
    pub fn busy(&self, actor: PersonId) -> bool {
        self.lifts
            .values()
            .any(|lift| lift.passenger == actor || lift.bearers.contains(&actor))
    }
    pub fn begin_lift(
        &mut self,
        passenger: PersonId,
        bearers: BTreeSet<PersonId>,
        permission: Permission,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
    ) -> Result<Lift, Error> {
        let authority = permission.authority()?;
        let completes = SimTime(
            now.0
                .checked_add(self.rules.lift_time.0)
                .ok_or(Error::Overflow)?,
        );
        let group = CarryGroup {
            passenger,
            anchor: *bearers.first().ok_or(Error::Unavailable)?,
            bearers: bearers.clone(),
            authority,
            accepted_at: completes,
        };
        self.check_group(&group, bodies, now, true)?;
        let lift = Lift {
            passenger,
            bearers,
            authority,
            started: now,
            completes,
        };
        self.lifts.insert(passenger, lift.clone());
        Ok(lift)
    }
    pub fn complete_lift(
        &mut self,
        passenger: PersonId,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
    ) -> Result<CarryGroup, Error> {
        let lift = self
            .lifts
            .get(&passenger)
            .ok_or(Error::Unavailable)?
            .clone();
        if now != lift.completes {
            return Err(Error::Unavailable);
        }
        let group = CarryGroup {
            passenger,
            anchor: *lift.bearers.first().ok_or(Error::Unavailable)?,
            bearers: lift.bearers,
            authority: lift.authority,
            accepted_at: now,
        };
        self.check_group(&group, bodies, now, false)?;
        self.lifts.remove(&passenger);
        self.offers.remove(&passenger);
        self.groups.insert(passenger, group.clone());
        Ok(group)
    }
    fn check_group(
        &self,
        group: &CarryGroup,
        bodies: &BTreeMap<PersonId, Body>,
        now: SimTime,
        require_offers: bool,
    ) -> Result<(), Error> {
        let passenger = bodies.get(&group.passenger).ok_or(Error::Unavailable)?;
        if group.bearers.is_empty()
            || group.bearers.len() > self.rules.max_bearers
            || group.bearers.contains(&group.passenger)
            || group.bearers.iter().any(|id| self.groups.contains_key(id))
            || require_offers
                && (self.busy(group.passenger) || group.bearers.iter().any(|id| self.busy(*id)))
        {
            return Err(Error::Unavailable);
        }
        for bearer in &group.bearers {
            let body = bodies.get(bearer).ok_or(Error::Unavailable)?;
            if !body.can_bear || !body.position.same_point(passenger.position) {
                return Err(Error::Unavailable);
            }
            if require_offers
                && !self
                    .groups
                    .get(&group.passenger)
                    .is_some_and(|existing| existing.bearers.contains(bearer))
                && !self
                    .offers
                    .get(&group.passenger)
                    .and_then(|offers| offers.get(bearer))
                    .is_some_and(|offer| offer.expires > now)
            {
                return Err(Error::Unavailable);
            }
        }
        if let Some(existing) = self.groups.get(&group.passenger) {
            for bearer in existing.bearers.difference(&group.bearers) {
                if bodies.get(bearer).is_some_and(|body| body.can_hold) {
                    return Err(Error::Unavailable);
                }
            }
        }
        let mut candidate = self.groups.clone();
        candidate.insert(group.passenger, group.clone());
        let mut connected = group.bearers.clone();
        connected.insert(group.passenger);
        loop {
            let size = connected.len();
            for other in candidate.values() {
                if connected.contains(&other.passenger) || !connected.is_disjoint(&other.bearers) {
                    connected.insert(other.passenger);
                    connected.extend(other.bearers.iter().copied());
                }
            }
            if connected.len() == size {
                break;
            }
        }
        let plan = capacity_plan(&candidate, bodies)?;
        if !plan.overloaded.is_disjoint(&connected) || !plan.unable.is_disjoint(&connected) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// A genuine birth places a newborn into the moving physical group. This
    /// does not promise sufficient capacity: the host must stop an overload.
    pub fn attach_newborn(
        &mut self,
        child: PersonId,
        gestator: PersonId,
        now: SimTime,
        bodies: &BTreeMap<PersonId, Body>,
    ) -> Result<CarryGroup, Error> {
        let child_body = bodies.get(&child).ok_or(Error::Unavailable)?;
        let mother = bodies.get(&gestator).ok_or(Error::Unavailable)?;
        if child == gestator
            || !child_body.position.same_point(mother.position)
            || self.groups.contains_key(&child)
        {
            return Err(Error::Unavailable);
        }
        let bearers = self
            .groups
            .get(&gestator)
            .map_or_else(|| BTreeSet::from([gestator]), |group| group.bearers.clone());
        let group = CarryGroup {
            passenger: child,
            anchor: *bearers.first().ok_or(Error::Unavailable)?,
            bearers,
            authority: Authority::Newborn,
            accepted_at: now,
        };
        self.groups.insert(child, group.clone());
        Ok(group)
    }
    pub fn remove_group(&mut self, passenger: PersonId) -> Option<CarryGroup> {
        self.groups.remove(&passenger)
    }
    pub fn cancel_lifts(&mut self, actor: PersonId) -> Vec<Lift> {
        let ids: Vec<_> = self
            .lifts
            .values()
            .filter(|lift| lift.passenger == actor || lift.bearers.contains(&actor))
            .map(|lift| lift.passenger)
            .collect();
        ids.into_iter()
            .filter_map(|id| self.lifts.remove(&id))
            .collect()
    }
    pub fn prune_offers(&mut self, now: SimTime) {
        for offers in self.offers.values_mut() {
            offers.retain(|_, offer| offer.expires > now);
        }
        self.offers.retain(|_, offers| !offers.is_empty());
    }
}
#[derive(Clone, Debug)]
pub struct Body {
    pub position: Position,
    pub mass: Quantity,
    pub cargo: Quantity,
    pub capacity: Quantity,
    pub can_hold: bool,
    pub can_walk: bool,
    pub can_bear: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapacityPlan {
    pub shares: BTreeMap<(PersonId, PersonId), Quantity>,
    pub overloaded: BTreeSet<PersonId>,
    pub unable: BTreeSet<PersonId>,
}
/// Load is shared in proportion to each willing bearer's remaining capacity.
/// Stable ID order apportions fractional units; no parallel winner consumes ID.
pub fn share_load(
    load: Quantity,
    capacities: &BTreeMap<PersonId, Quantity>,
) -> Result<BTreeMap<PersonId, Quantity>, Error> {
    let total = capacities.values().try_fold(0_u64, |sum, capacity| {
        sum.checked_add(capacity.0).ok_or(Error::Overflow)
    })?;
    if total < load.0 {
        return Err(Error::Unavailable);
    }
    if load.0 == 0 {
        return Ok(capacities.keys().map(|id| (*id, Quantity(0))).collect());
    }
    let mut result: BTreeMap<_, _> = capacities
        .iter()
        .map(|(id, capacity)| {
            (
                *id,
                Quantity((u128::from(load.0) * u128::from(capacity.0) / u128::from(total)) as u64),
            )
        })
        .collect();
    let mut remaining = load.0 - result.values().map(|quantity| quantity.0).sum::<u64>();
    for (id, share) in &mut result {
        if remaining == 0 {
            break;
        }
        if share.0 < capacities[id].0 {
            share.0 += 1;
            remaining -= 1;
        }
    }
    debug_assert_eq!(remaining, 0);
    Ok(result)
}
pub fn capacity_plan(
    groups: &BTreeMap<PersonId, CarryGroup>,
    bodies: &BTreeMap<PersonId, Body>,
) -> Result<CapacityPlan, Error> {
    let mut free: BTreeMap<_, _> = bodies
        .iter()
        .map(|(id, body)| (*id, Quantity(body.capacity.0.saturating_sub(body.cargo.0))))
        .collect();
    let mut plan = CapacityPlan::default();
    for (passenger, group) in groups {
        let body = bodies.get(passenger).ok_or(Error::Unavailable)?;
        let load = Quantity(
            body.mass
                .0
                .checked_add(body.cargo.0)
                .ok_or(Error::Overflow)?,
        );
        let mut capacities = BTreeMap::new();
        for bearer in &group.bearers {
            let body = bodies.get(bearer).ok_or(Error::Unavailable)?;
            if !body.can_walk || !body.can_bear {
                plan.unable.insert(*bearer);
            }
            capacities.insert(*bearer, *free.get(bearer).ok_or(Error::Unavailable)?);
        }
        match share_load(load, &capacities) {
            Ok(shares) => {
                for (bearer, share) in shares {
                    free.get_mut(&bearer).unwrap().0 -= share.0;
                    plan.shares.insert((*passenger, bearer), share);
                }
            }
            Err(Error::Unavailable) => {
                plan.overloaded.insert(*passenger);
                // An overload stops movement; it does not remove the physical
                // load. Preserve actual burden so later intake cannot treat a
                // held newborn as weightless while the group is stopped.
                let total: u128 = capacities.values().map(|q| u128::from(q.0)).sum();
                let weights: BTreeMap<_, _> = if total == 0 {
                    capacities.keys().map(|id| (*id, 1_u128)).collect()
                } else {
                    capacities
                        .iter()
                        .map(|(id, q)| (*id, u128::from(q.0)))
                        .collect()
                };
                let total: u128 = weights.values().sum();
                if total == 0 {
                    return Err(Error::Unavailable);
                }
                let mut shares: BTreeMap<_, _> = weights
                    .iter()
                    .map(|(id, weight)| (*id, (u128::from(load.0) * *weight / total) as u64))
                    .collect();
                let mut remainder = load.0 - shares.values().sum::<u64>();
                for share in shares.values_mut() {
                    if remainder == 0 {
                        break;
                    }
                    *share += 1;
                    remainder -= 1;
                }
                for (bearer, share) in shares {
                    free.get_mut(&bearer).unwrap().0 = free[&bearer].0.saturating_sub(share);
                    plan.shares.insert((*passenger, bearer), Quantity(share));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(plan)
}
pub fn transfer_capacity(
    groups: &BTreeMap<PersonId, CarryGroup>,
    bodies: &BTreeMap<PersonId, Body>,
    actor: PersonId,
    amount: Quantity,
) -> Result<bool, Error> {
    if amount.0 == 0 {
        return Ok(true);
    }
    let mut updated = bodies.clone();
    let body = updated.get_mut(&actor).ok_or(Error::Unavailable)?;
    body.cargo.0 = body.cargo.0.checked_add(amount.0).ok_or(Error::Overflow)?;
    if body.cargo > body.capacity {
        return Ok(false);
    }
    let mut component = BTreeSet::from([actor]);
    loop {
        let size = component.len();
        for group in groups.values() {
            if component.contains(&group.passenger) || !component.is_disjoint(&group.bearers) {
                component.insert(group.passenger);
                component.extend(group.bearers.iter().copied());
            }
        }
        if component.len() == size {
            break;
        }
    }
    Ok(capacity_plan(groups, &updated)?
        .overloaded
        .is_disjoint(&component))
}
/// Resolve one immutable location catalog, memoizing shared carrier paths.
/// Unknown anchors and cycles are rejected rather than guessing an endpoint.
pub fn resolve_locations(
    locations: &BTreeMap<PersonId, Location>,
) -> Result<BTreeMap<PersonId, Location>, Error> {
    let mut result: BTreeMap<PersonId, Location> = BTreeMap::new();
    for actor in locations.keys() {
        if result.contains_key(actor) {
            continue;
        }
        let mut current = *actor;
        let mut path = BTreeSet::new();
        let resolved = loop {
            if let Some(location) = result.get(&current) {
                break location.clone();
            }
            if !path.insert(current) {
                return Err(Error::InvalidSave("cyclic physical carrying".into()));
            }
            match locations
                .get(&current)
                .ok_or_else(|| Error::InvalidSave("unknown physical carrier".into()))?
            {
                Location::Carried { carrier } => current = *carrier,
                location => break location.clone(),
            }
        };
        for actor in path {
            result.insert(actor, resolved.clone());
        }
    }
    Ok(result)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub mass: Quantity,
    pub carried_by: Vec<PersonId>,
    pub passengers: Vec<PersonId>,
    pub body_load: Quantity,
    pub offers: Vec<Offer>,
    pub responses: Vec<Response>,
}

pub fn validate(
    world: &World,
    bodies: &BTreeMap<PersonId, Body>,
    now: SimTime,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("invalid transport state".into());
    if !world.rules.valid() {
        return Err(fail());
    }
    for (passenger, group) in &world.groups {
        if *passenger != group.passenger
            || group.accepted_at > now
            || group.bearers.is_empty()
            || group.bearers.len() > world.rules.max_bearers
            || !group.bearers.contains(&group.anchor)
            || group.bearers.contains(passenger)
            || group.bearers.iter().any(|id| world.groups.contains_key(id))
        {
            return Err(fail());
        }
        let body = bodies.get(passenger).ok_or_else(fail)?;
        for bearer in &group.bearers {
            if !bodies
                .get(bearer)
                .ok_or_else(fail)?
                .position
                .same_point(body.position)
            {
                return Err(fail());
            }
        }
    }
    for (passenger, offers) in &world.offers {
        if !bodies.contains_key(passenger)
            || offers.is_empty()
            || offers.len() > world.rules.max_bearers
        {
            return Err(fail());
        }
        for (bearer, offer) in offers {
            if !bodies.contains_key(bearer)
                || bearer == passenger
                || offer.bearer != *bearer
                || offer.passenger != *passenger
                || offer.offered_at > now
                || offer.expires <= offer.offered_at
                || offer.expires.0 - offer.offered_at.0 != world.rules.offer_lifetime.0
            {
                return Err(fail());
            }
        }
    }
    let mut occupied = BTreeSet::new();
    for (passenger, lift) in &world.lifts {
        if *passenger != lift.passenger
            || lift.started > now
            || lift.completes <= now
            || lift.bearers.is_empty()
            || lift.bearers.len() > world.rules.max_bearers
            || lift.bearers.contains(passenger)
            || lift.completes.0.checked_sub(lift.started.0) != Some(world.rules.lift_time.0)
            || !occupied.insert(*passenger)
        {
            return Err(fail());
        }
        let body = bodies.get(passenger).ok_or_else(fail)?;
        for bearer in &lift.bearers {
            if !occupied.insert(*bearer)
                || world.groups.contains_key(bearer)
                || !bodies
                    .get(bearer)
                    .ok_or_else(fail)?
                    .position
                    .same_point(body.position)
            {
                return Err(fail());
            }
        }
    }
    for body in bodies.values() {
        if body.mass.0 == 0
            || matches!(body.position, Position::Road(position) if !position.valid())
        {
            return Err(fail());
        }
    }
    for (actor, approach) in &world.approaches {
        if !bodies.contains_key(actor)
            || world.groups.contains_key(actor)
            || !approach.target.valid()
            || approach.started > now
            || approach.completes <= now
        {
            return Err(fail());
        }
    }
    for (actor, target) in &world.resume_targets {
        let body = bodies.get(actor).ok_or_else(fail)?;
        if !matches!(body.position,Position::Road(point) if *target==point.from||*target==point.to)
        {
            return Err(fail());
        }
    }
    for (actor, target) in &world.paused_approaches {
        if !target.valid() || !bodies.contains_key(actor) || world.approaches.contains_key(actor) {
            return Err(fail());
        }
    }
    if world.responses.len() > 64 {
        return Err(fail());
    }
    for response in &world.responses {
        if !bodies.contains_key(&response.passenger)
            || !bodies.contains_key(&response.bearer)
            || response.observed_at > now
            || response.reason.is_empty()
            || response.reason.len() > 256
        {
            return Err(fail());
        }
    }
    for (observer, requests) in &world.known_support {
        if !bodies.contains_key(observer) || requests.len() > 64 {
            return Err(fail());
        }
        for (passenger, request) in requests {
            if *passenger != request.passenger
                || !bodies.contains_key(passenger)
                || !bodies.contains_key(&request.requester)
                || request.observed_at > now
                || request.expires <= request.observed_at
            {
                return Err(fail());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

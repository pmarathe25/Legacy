//! Physical food intentions can acquire supplies for oneself or a beneficiary.
use game_core::*;
use game_knowledge::{Facts, StoreLocation, known, location_key, patch_key};
use game_sim::*;
use serde::{Deserialize, Serialize};
mod local_meal;
pub use local_meal::{LocalMeal, local_meal};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Plan {
    Explore {
        area: AreaId,
    },
    Meal {
        inventory: InventoryId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        good: Option<GoodId>,
    },
    Forage {
        patch: u64,
        inventory: InventoryId,
        quantity: u64,
        gathering: bool,
    },
}
fn location(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    inventory: InventoryId,
) -> Result<AreaId> {
    let location: StoreLocation = known(facts, &location_key(inventory))
        .ok_or_else(|| Error::Missing("known inventory location".into()))?;
    if location.carrier == Some(person) {
        game_transport::area(tx.get()?, person)
    } else {
        Ok(location.area)
    }
}
pub fn travel(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    let location = tx
        .get::<game_transport::Positions>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("position".into()))?;
    next_step(tx, person, facts, location, target)
}
pub fn step(tx: &mut Transaction<'_>, person: PersonId, plan: &mut Plan) -> Result<bool> {
    proceed(tx, person, plan, true)
}
pub fn acquire(tx: &mut Transaction<'_>, person: PersonId, plan: &mut Plan) -> Result<bool> {
    proceed(tx, person, plan, false)
}
impl Plan {
    pub fn work_duration(&self) -> SimDuration {
        match self {
            Self::Forage {
                gathering: false, ..
            } => game_ecology::GATHERING_TIME,
            _ => SimDuration(0),
        }
    }
    pub fn intended_good(&self, facts: &Facts) -> Option<GoodId> {
        match *self {
            Self::Meal { good, .. } => good,
            Self::Forage { patch, .. } => {
                known::<game_ecology::Patch>(facts, &patch_key(patch)).map(|p| p.good)
            }
            Self::Explore { .. } => None,
        }
    }
    pub fn inventory(&self) -> Option<InventoryId> {
        match *self {
            Self::Meal { inventory, .. } | Self::Forage { inventory, .. } => Some(inventory),
            Self::Explore { .. } => None,
        }
    }
}
fn proceed(
    tx: &mut Transaction<'_>,
    person: PersonId,
    plan: &mut Plan,
    consume: bool,
) -> Result<bool> {
    let facts = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&person)
        .ok_or_else(|| Error::Missing("personal knowledge".into()))?;
    if let Plan::Explore { area } = *plan {
        if let (Some(next), _) = travel(tx, person, facts, area)? {
            tx.attempt(game_transport::Move {
                person,
                destination: next,
            })?;
            return Ok(false);
        }
        tx.call(game_knowledge::ObserveSurroundings { person })?;
        return Ok(true);
    }
    let inventory = match *plan {
        Plan::Meal { inventory, .. } | Plan::Forage { inventory, .. } => inventory,
        Plan::Explore { .. } => unreachable!(),
    };
    let (target, chosen) = match *plan {
        Plan::Meal { good, .. } => (location(tx, person, facts, inventory)?, good),
        Plan::Forage { patch, .. } => {
            let patch = known::<game_ecology::Patch>(facts, &patch_key(patch))
                .ok_or_else(|| Error::Missing("known patch".into()))?;
            (patch.area, Some(patch.good))
        }
        Plan::Explore { .. } => unreachable!(),
    };
    if let (Some(next), _) = travel(tx, person, facts, target)? {
        tx.attempt(game_transport::Move {
            person,
            destination: next,
        })?;
        return Ok(false);
    }
    if let Plan::Forage {
        patch,
        quantity,
        gathering,
        ..
    } = plan
        && !*gathering
    {
        let good = tx.get::<game_ecology::Ecology>()?.patches[patch].good;
        let needed = tx.get::<game_objects::Objects>()?.mass(good, *quantity)?;
        let held = &tx.get::<game_cargo::Cargo>()?.0[&inventory];
        if held.capacity.saturating_sub(held.used()?) < needed
            && tx.registered::<game_stockpiles::StoreSurplus>()
        {
            tx.attempt(game_stockpiles::StoreSurplus {
                person,
                inventory,
                needed_mass: needed,
            })?;
        }
        let minimum = if !consume {
            1
        } else {
            tx.get::<game_objects::Objects>()?
                .by_id(good)?
                .food
                .as_ref()
                .ok_or_else(|| Error::Rejected("chosen patch is not edible".into()))?
                .portion
                .saturating_sub(tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(good))
                .max(1)
        };
        tx.attempt(game_ecology::GatherUpTo {
            person,
            patch: *patch,
            target: *quantity,
            minimum,
            destination: inventory,
        })?;
        *gathering = true;
        return Ok(false);
    }
    if !consume {
        let goods = &tx.get::<game_cargo::Cargo>()?.0[&inventory].goods;
        let useful = chosen.map_or_else(
            || {
                goods.iter().any(|(good, q)| {
                    *q > 0
                        && tx
                            .get::<game_objects::Objects>()
                            .unwrap()
                            .food_rules()
                            .contains_key(good)
                })
            },
            |good| goods.get(&good).is_some_and(|q| *q > 0),
        );
        require(useful, "no usable food at source")?;
    }
    if consume {
        if let Some(good) = chosen {
            let quantity = tx
                .get::<game_objects::Objects>()?
                .by_id(good)?
                .food
                .as_ref()
                .ok_or_else(|| Error::Rejected("chosen good is not food".into()))?
                .portion;
            tx.attempt(game_physiology::ConsumeEdible {
                person,
                inventory,
                good,
                quantity,
            })?;
        } else {
            tx.attempt(game_physiology::Eat { person, inventory })?;
        }
    }
    tx.call(game_knowledge::ObserveStore { person, inventory })?;
    Ok(true)
}
/// Choose the next known road, including resuming an interrupted journey.
pub fn next_step(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    location: &game_transport::Location,
    target: AreaId,
) -> Result<(Option<AreaId>, SimDuration)> {
    use game_transport::Location;
    match location {
        Location::At(area) if *area == target => Ok((None, SimDuration(0))),
        Location::At(area) => {
            let route = game_knowledge::route(facts, *area, target)?;
            Ok((
                route.areas.get(1).copied(),
                game_transport::travel_time(tx, person, route.duration),
            ))
        }
        Location::Road {
            from,
            to,
            elapsed,
            duration,
        } => [
            (*from, elapsed.0),
            (
                *to,
                duration
                    .0
                    .checked_sub(elapsed.0)
                    .ok_or_else(|| Error::Rejected("invalid road progress".into()))?,
            ),
        ]
        .into_iter()
        .filter_map(|(area, offset)| {
            let route = game_knowledge::route(facts, area, target).ok()?;
            Some((
                Some(area),
                SimDuration(
                    offset
                        .checked_add(game_transport::travel_time(tx, person, route.duration).0)?,
                ),
            ))
        })
        .min_by_key(|(area, cost)| (cost.0, *area))
        .ok_or_else(|| Error::Rejected("no known route from road".into())),
        _ => Err(Error::Rejected("already travelling".into())),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoodCare {
    pub beneficiary: PersonId,
    pub home: AreaId,
    pub observed_need: u64,
    #[serde(default)]
    pub observed_at: SimTime,
    pub plan: Plan,
    pub returning: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TendFood {
    pub caregiver: PersonId,
}
impl Request for TendFood {
    const KEY: &'static str = "households.tend-food";
    type Response = ();
}
impl FoodCare {
    pub fn valid_for(&self, caregiver: PersonId) -> bool {
        caregiver != self.beneficiary
            && self.observed_need > 0
            && (self.plan.inventory().is_some()
                || matches!(self.plan, Plan::Explore { .. }) && !self.returning)
    }
}

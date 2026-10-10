use crate::*;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealthIntent {
    Health(HealthAction),
    Move {
        destination: AreaId,
    },
    Carry {
        source: InventoryId,
        quantity: Quantity,
    },
    GatherWater {
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
    },
    RequestResourceAccess {
        resource: ResourceId,
    },
    RequestStockAccess {
        inventory: InventoryId,
    },
}
pub struct RememberedStock {
    pub fact: StockKnowledge,
    pub here: bool,
    pub held: bool,
}
pub struct RememberedSpring {
    pub id: ResourceId,
    pub area: AreaId,
    pub available: Quantity,
    pub accessible: bool,
    pub contamination: u16,
    pub observed_at: SimTime,
    /// Local execution prospects only; never queried for a distant source.
    pub can_gather: bool,
    pub can_request: bool,
}
#[derive(Clone, Copy)]
pub struct WaterValues {
    pub honesty: u16,
    pub risk_tolerance: u16,
}
pub type KnownRoutes = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
pub struct WaterPlanning<'a> {
    pub now: SimTime,
    pub area: AreaId,
    pub stationary: bool,
    pub values: WaterValues,
    pub stocks: &'a [RememberedStock],
    pub springs: &'a [RememberedSpring],
    pub routes: &'a KnownRoutes,
    pub searched: &'a BTreeMap<AreaId, SimTime>,
    pub stock_requests: &'a BTreeSet<InventoryId>,
}
pub fn known_travel_cost(routes: &KnownRoutes, from: AreaId, to: AreaId) -> Option<SimDuration> {
    let mut frontier = BTreeSet::from([(0u64, from)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == to {
            return Some(SimDuration(cost));
        }
        for (destination, duration) in routes.get(&area).into_iter().flat_map(|links| links.iter())
        {
            if let Some(next) = cost.checked_add(duration.0) {
                frontier.insert((next, *destination));
            }
        }
    }
    None
}
pub fn plan_spring(input: &WaterPlanning<'_>, local_only: bool) -> Option<HealthIntent> {
    if !input.stationary {
        return None;
    }
    let may_take = input.values.honesty < 250 && input.values.risk_tolerance >= 700;
    input
        .springs
        .iter()
        .filter(|s| {
            s.observed_at <= input.now
                && s.available.0 >= 1_000
                && (!local_only || s.area == input.area)
        })
        .filter_map(|s| {
            let (cost, intent) = if s.area != input.area {
                if !s.accessible && !may_take {
                    return None;
                }
                (
                    known_travel_cost(input.routes, input.area, s.area)?,
                    HealthIntent::Move {
                        destination: s.area,
                    },
                )
            } else if !s.accessible && !may_take {
                if !s.can_request {
                    return None;
                }
                (
                    SimDuration(0),
                    HealthIntent::RequestResourceAccess { resource: s.id },
                )
            } else {
                if !s.can_gather {
                    return None;
                }
                (
                    SimDuration(0),
                    HealthIntent::GatherWater {
                        resource: s.id,
                        quantity: Quantity(1_000),
                        unauthorized: !s.accessible,
                    },
                )
            };
            Some((
                cost,
                matches!(intent, HealthIntent::RequestResourceAccess { .. }),
                s.contamination,
                s.id,
                intent,
            ))
        })
        .min_by_key(|(cost, request, contamination, id, _)| (*cost, *request, *contamination, *id))
        .map(|(_, _, _, _, intent)| intent)
}
pub fn plan_water_procurement(rules: &Rules, input: &WaterPlanning<'_>) -> Option<HealthIntent> {
    if !input.stationary {
        return None;
    }
    if let Some(intent) = plan_spring(input, true) {
        return Some(intent);
    }
    if let Some((_, stock)) = input
        .stocks
        .iter()
        .filter(|s| {
            s.fact.good == Good::Water
                && s.fact.accessible
                && s.fact.quantity >= rules.drink_quantity
                && s.fact.area != input.area
                && s.fact.learned_at <= input.now
        })
        .filter_map(|s| {
            known_travel_cost(input.routes, input.area, s.fact.area).map(|cost| (cost, s))
        })
        .min_by_key(|(cost, s)| (*cost, s.fact.inventory))
    {
        return Some(HealthIntent::Move {
            destination: stock.fact.area,
        });
    }
    if let Some(intent) = plan_spring(input, false) {
        return Some(intent);
    }
    if let Some(id) = input.stock_requests.iter().find(|id| {
        input.stocks.iter().any(|s| {
            s.fact.inventory == **id
                && s.fact.good == Good::Water
                && s.fact.quantity >= rules.drink_quantity
        })
    }) {
        return Some(HealthIntent::RequestStockAccess { inventory: *id });
    }
    input
        .routes
        .keys()
        .take(16)
        .filter(|destination| **destination != input.area)
        .filter(|destination| {
            input
                .searched
                .get(destination)
                .is_none_or(|at| input.now.0.saturating_sub(at.0) >= rules.water_search_retry.0)
        })
        .filter_map(|destination| {
            known_travel_cost(input.routes, input.area, *destination)
                .map(|cost| (cost, *destination))
        })
        .min_by_key(|(cost, area)| (*cost, *area))
        .map(|(_, destination)| HealthIntent::Move { destination })
}
pub fn plan_water_delivery(rules: &Rules, input: &WaterPlanning<'_>) -> Option<HealthIntent> {
    if let Some(stock) = input
        .stocks
        .iter()
        .filter(|s| {
            s.fact.good == Good::Water
                && s.fact.accessible
                && s.here
                && !s.held
                && s.fact.quantity >= rules.drink_quantity
        })
        .min_by_key(|s| s.fact.inventory)
    {
        return Some(HealthIntent::Carry {
            source: stock.fact.inventory,
            quantity: rules.drink_quantity,
        });
    }
    plan_water_procurement(rules, input)
}
pub struct NeedFacts {
    pub actor: PersonId,
    pub drink_relief: u64,
    pub sheltered: bool,
    pub can_enter_shelter: bool,
}
impl World {
    pub fn plan_need(&self, need: NeedFacts, input: &WaterPlanning<'_>) -> Option<HealthIntent> {
        if self.jobs.contains_key(&need.actor)
            || self
                .jobs
                .values()
                .any(|job| matches!(job.work,Work::Treat {patient,..} if patient==need.actor))
        {
            return None;
        }
        let clinical = self.clinical.get(&need.actor)?;
        if clinical.thirst >= need.drink_relief / 4 {
            let stock = input
                .stocks
                .iter()
                .filter(|s| {
                    s.fact.good == Good::Water
                        && s.fact.accessible
                        && s.here
                        && s.fact.quantity >= self.rules.drink_quantity
                })
                .min_by_key(|s| s.fact.inventory);
            let Some(stock) = stock else {
                return plan_water_procurement(&self.rules, input);
            };
            let safety = self
                .known
                .get(&need.actor)
                .and_then(|k| k.water.get(&stock.fact.inventory));
            if !safety.is_some_and(|s| s.purified)
                && (safety.is_some_and(|s| s.suspected_unsafe) || input.values.risk_tolerance < 500)
            {
                let local = |good, quantity| {
                    input.stocks.iter().find(|s| {
                        s.fact.good == good
                            && s.fact.accessible
                            && s.here
                            && s.fact.quantity >= quantity
                    })
                };
                if let (Some(fuel), Some(tool)) = (
                    local(Good::Timber, self.rules.boiling_fuel),
                    local(Good::Tool, Quantity(1_000)),
                ) {
                    if stock.fact.quantity > self.rules.drink_quantity && !stock.held {
                        return Some(HealthIntent::Carry {
                            source: stock.fact.inventory,
                            quantity: self.rules.drink_quantity,
                        });
                    }
                    return Some(HealthIntent::Health(HealthAction::Boil {
                        source: stock.fact.inventory,
                        fuel: fuel.fact.inventory,
                        tool: tool.fact.inventory,
                    }));
                }
            }
            return Some(HealthIntent::Health(HealthAction::Drink {
                source: stock.fact.inventory,
            }));
        }
        if clinical.cold >= self.rules.cold_harm / 4 && !need.sheltered && need.can_enter_shelter {
            return self
                .known
                .get(&need.actor)
                .into_iter()
                .flat_map(|k| k.shelters.values())
                .filter(|s| {
                    s.area == input.area
                        && s.accessible
                        && s.known_occupants.len() < s.capacity as usize
                })
                .max_by_key(|s| (s.insulation, s.id))
                .map(|s| HealthIntent::Health(HealthAction::EnterShelter { shelter: s.id }));
        }
        None
    }
}
pub struct WillingCare<'a> {
    pub capable: bool,
    pub medicine: u16,
    pub busy: bool,
    pub hungry: bool,
    pub fatigued: bool,
    pub urgent_dependent: bool,
    pub empathy: u16,
    pub trust: i32,
    pub stocks: &'a [RememberedStock],
}
pub fn willing_supplies(
    rules: &Rules,
    facts: WillingCare<'_>,
) -> Option<(InventoryId, InventoryId)> {
    if !facts.capable
        || facts.medicine == 0
        || facts.busy
        || facts.hungry
        || facts.fatigued
        || facts.urgent_dependent
        || i64::from(facts.empathy) + i64::from(facts.trust) < 300
    {
        return None;
    }
    let find = |good, quantity| {
        facts
            .stocks
            .iter()
            .find(|s| {
                s.fact.good == good && s.fact.accessible && s.here && s.fact.quantity >= quantity
            })
            .map(|s| s.fact.inventory)
    };
    Some((
        find(Good::Herb, rules.treatment_herbs)?,
        find(Good::Water, rules.treatment_water)?,
    ))
}
pub struct WorkNeeds {
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub health: u64,
    pub max_health: u64,
}
pub fn should_pause_work(work: &Work, needs: WorkNeeds) -> bool {
    !matches!(work, Work::Drink { .. })
        && (needs.fatigue >= needs.fatigue_limit / 2
            || needs.hunger >= needs.meal_relief
            || needs.health <= needs.max_health / 3)
}
pub fn offer_response(caregiver: PersonId, symptoms: &SymptomsView, trust: i32) -> HealthAction {
    if (symptoms.fever || symptoms.wounded || symptoms.shivering) && trust >= -100 {
        HealthAction::AcceptTreatment { caregiver }
    } else {
        HealthAction::RefuseTreatment { caregiver }
    }
}
pub struct PatientOpportunity {
    pub symptoms: SymptomsView,
    pub contact: bool,
    pub offer_exists: bool,
    pub busy: bool,
}
pub fn proactive_offer(
    actor: PersonId,
    rules: &Rules,
    facts: WillingCare<'_>,
    patients: &[PatientOpportunity],
) -> Option<HealthAction> {
    if !facts.capable
        || facts.medicine == 0
        || facts.busy
        || facts.empathy < 300
        || facts.hungry
        || facts.fatigued
    {
        return None;
    }
    let find = |good, quantity| {
        facts
            .stocks
            .iter()
            .find(|s| {
                s.fact.good == good && s.fact.accessible && s.here && s.fact.quantity >= quantity
            })
            .map(|s| s.fact.inventory)
    };
    let herbs = find(Good::Herb, rules.treatment_herbs)?;
    let water = find(Good::Water, rules.treatment_water)?;
    patients
        .iter()
        .filter(|p| {
            p.symptoms.person != actor
                && (p.symptoms.fever || p.symptoms.wounded || p.symptoms.shivering)
                && p.contact
                && !p.offer_exists
                && !p.busy
        })
        .min_by_key(|p| p.symptoms.person)
        .map(|p| HealthAction::OfferTreatment {
            patient: p.symptoms.person,
            herbs,
            water,
        })
}

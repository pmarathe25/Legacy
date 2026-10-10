use crate::*;
use game_core::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthConsumer {
    Drink,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyNeed {
    pub good: Good,
    pub quantity: Quantity,
    pub source: Option<InventoryId>,
    /// A physically separated held portion is needed for this intervention.
    pub carried: bool,
    pub consumer: HealthConsumer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealthIntent {
    Action(HealthAction),
    Supply(SupplyNeed),
}
pub struct RememberedStock {
    pub fact: StockKnowledge,
    pub here: bool,
    pub held: bool,
}
#[derive(Clone, Copy)]
pub struct IntakeValues {
    pub risk_tolerance: u16,
}
pub struct HealthPlanning<'a> {
    pub now: SimTime,
    pub area: AreaId,
    pub stationary: bool,
    pub values: IntakeValues,
    pub stocks: &'a [RememberedStock],
}
pub struct NeedFacts {
    pub actor: PersonId,
    pub drink_relief: u64,
    pub sheltered: bool,
    pub can_enter_shelter: bool,
}
impl World {
    fn present_supply<'a>(
        &self,
        input: &HealthPlanning<'a>,
        good: Good,
        quantity: Quantity,
    ) -> Option<&'a RememberedStock> {
        input.stocks.iter().find(|stock| {
            stock.fact.good == good
                && stock.fact.accessible
                && stock.here
                && stock.fact.quantity >= quantity
                && stock.fact.observed_at <= input.now
                && stock.fact.learned_at <= input.now
        })
    }
    fn ready_for_need(&self, actor: PersonId) -> bool {
        !self.jobs.contains_key(&actor) && !self.patient_busy(actor)
    }
    fn water_requires_preparation(
        &self,
        actor: PersonId,
        source: InventoryId,
        input: &HealthPlanning<'_>,
    ) -> bool {
        let safety = self
            .known
            .get(&actor)
            .and_then(|known| known.water.get(&source))
            .filter(|safety| safety.observed_at <= input.now);
        !safety.is_some_and(|safety| safety.purified)
            && (safety.is_some_and(|safety| safety.suspected_unsafe)
                || input.values.risk_tolerance < 500)
    }
    /// A physically available preparation, independent of thirst and personal
    /// risk preference. The host revalidates body and toolkit allocation.
    pub fn boiling_action(
        &self,
        actor: PersonId,
        source: InventoryId,
        input: &HealthPlanning<'_>,
    ) -> Option<HealthAction> {
        if !self.ready_for_need(actor) {
            return None;
        }
        let water = input.stocks.iter().find(|stock| {
            stock.fact.inventory == source
                && stock.fact.good == Good::Water
                && stock.fact.accessible
                && stock.here
                && stock.fact.quantity >= self.rules.drink_quantity
                && stock.fact.observed_at <= input.now
                && stock.fact.learned_at <= input.now
        })?;
        if water.fact.quantity > self.rules.drink_quantity && !water.held {
            return None;
        }
        let fuel = self.present_supply(input, Good::Timber, self.rules.boiling_fuel)?;
        let tool = self.present_supply(input, Good::Tool, Quantity(1_000))?;
        Some(HealthAction::Boil {
            source,
            fuel: fuel.fact.inventory,
            tool: tool.fact.inventory,
        })
    }
    pub fn consume_supply(
        &self,
        actor: PersonId,
        source: InventoryId,
        input: &HealthPlanning<'_>,
        consumer: HealthConsumer,
    ) -> Option<HealthAction> {
        if !self.ready_for_need(actor) {
            return None;
        }
        match consumer {
            HealthConsumer::Drink => {
                let stock = input.stocks.iter().find(|stock| {
                    stock.fact.inventory == source
                        && stock.fact.good == Good::Water
                        && stock.fact.accessible
                        && stock.here
                        && stock.fact.quantity >= self.rules.drink_quantity
                        && stock.fact.observed_at <= input.now
                        && stock.fact.learned_at <= input.now
                })?;
                if self.water_requires_preparation(actor, source, input) {
                    if let Some(action) = self.boiling_action(actor, source, input) {
                        return Some(action);
                    }
                    if stock.fact.quantity > self.rules.drink_quantity
                        && !stock.held
                        && self
                            .present_supply(input, Good::Timber, self.rules.boiling_fuel)
                            .is_some()
                        && self
                            .present_supply(input, Good::Tool, Quantity(1_000))
                            .is_some()
                    {
                        return None;
                    }
                }
                Some(HealthAction::Drink { source })
            }
        }
    }
    pub fn plan_need(&self, need: NeedFacts, input: &HealthPlanning<'_>) -> Option<HealthIntent> {
        if !self.ready_for_need(need.actor) {
            return None;
        }
        let clinical = self.clinical.get(&need.actor)?;
        if clinical.thirst >= need.drink_relief / 4 {
            let stock = input
                .stocks
                .iter()
                .filter(|stock| {
                    stock.fact.good == Good::Water
                        && stock.fact.accessible
                        && stock.here
                        && stock.fact.quantity >= self.rules.drink_quantity
                        && stock.fact.observed_at <= input.now
                        && stock.fact.learned_at <= input.now
                })
                .min_by_key(|stock| {
                    (
                        self.water_requires_preparation(need.actor, stock.fact.inventory, input),
                        !stock.held,
                        stock.fact.inventory,
                    )
                });
            if let Some(stock) = stock {
                if let Some(action) = self.consume_supply(
                    need.actor,
                    stock.fact.inventory,
                    input,
                    HealthConsumer::Drink,
                ) {
                    return Some(HealthIntent::Action(action));
                }
                return Some(HealthIntent::Supply(SupplyNeed {
                    good: Good::Water,
                    quantity: self.rules.drink_quantity,
                    source: Some(stock.fact.inventory),
                    carried: true,
                    consumer: HealthConsumer::Drink,
                }));
            }
            return Some(HealthIntent::Supply(SupplyNeed {
                good: Good::Water,
                quantity: self.rules.drink_quantity,
                source: None,
                carried: false,
                consumer: HealthConsumer::Drink,
            }));
        }
        if clinical.cold >= self.rules.cold_harm / 4
            && !need.sheltered
            && need.can_enter_shelter
            && input.stationary
        {
            return self
                .known
                .get(&need.actor)
                .into_iter()
                .flat_map(|known| known.shelters.values())
                .filter(|shelter| {
                    shelter.area == input.area
                        && shelter.accessible
                        && shelter.observed_at <= input.now
                        && shelter.known_occupants.len() < shelter.capacity as usize
                })
                .max_by_key(|shelter| (shelter.insulation, shelter.id))
                .map(|shelter| {
                    HealthIntent::Action(HealthAction::EnterShelter {
                        shelter: shelter.id,
                    })
                });
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

/// Current physical exposure pressure. Individual tolerance already contributes
/// to the accumulated cold state; this is not a source or action preference.
pub fn exposure_urgency(cold: u64, cold_harm: u64) -> i64 {
    if cold_harm == 0 || cold == 0 {
        0
    } else if cold >= cold_harm {
        20_000
    } else if cold >= cold_harm.div_ceil(4) {
        3_000
    } else {
        0
    }
}

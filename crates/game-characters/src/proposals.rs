//! Personal judgments of known collective consequences. Proposal identity is
//! opaque; institutional validity and execution remain with their owners.
use crate::decisions::{Attachment, Personality};
use crate::planning::resource_need_pressure;
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Resource {
        affiliation: InstitutionId,
        gained: bool,
        good: Option<Good>,
        quantity: Quantity,
    },
    Passage {
        beneficiary: InstitutionId,
    },
    Protection {
        beneficiary: InstitutionId,
    },
    Danger {
        provider: InstitutionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence<P> {
    pub proposal: P,
    pub effects: Vec<Effect>,
}

pub struct Context<'a> {
    pub affiliations: &'a BTreeSet<InstitutionId>,
    pub hunger: u64,
    pub hunger_exempt: bool,
    pub meal_relief: u64,
    pub meal_size: Quantity,
    pub hydration_urgency: i64,
    pub drink_quantity: Quantity,
    pub concepts: &'a BTreeMap<ConceptId, Good>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Judgment<P> {
    pub proposal: P,
    pub benefit: i64,
    pub need: i64,
    pub value: i64,
    pub risk: i64,
    pub total: i64,
}
impl<P> Judgment<P> {
    pub fn supports(&self) -> bool {
        self.total >= 0
    }
}

pub fn assess<P: Clone>(
    personality: &Personality,
    values: &BTreeMap<ConceptId, Attachment>,
    context: &Context<'_>,
    evidence: &Evidence<P>,
) -> Judgment<P> {
    let mut result = Judgment {
        proposal: evidence.proposal.clone(),
        benefit: i64::from(personality.empathy) / 3 + i64::from(personality.honesty) / 3,
        need: 0,
        value: 0,
        risk: 0,
        total: 0,
    };
    for effect in &evidence.effects {
        match *effect {
            Effect::Resource {
                affiliation,
                gained,
                good,
                quantity,
            } if context.affiliations.contains(&affiliation) => {
                let quantity = quantity.0.min(100_000);
                // One known meal or drink has a comparable bodily benefit;
                // unfamiliar goods acquire no assumed need or attachment.
                let dose = match good {
                    Some(Good::Food) => context.meal_size.0,
                    Some(Good::Water) => context.drink_quantity.0,
                    _ => 1_000,
                }
                .max(1);
                let amount = (u128::from(quantity) * 1_000 / u128::from(dose)).min(100_000) as i64;
                let pressure = good.map_or(0, |good| {
                    if good == Good::Food && context.hunger_exempt {
                        return 0;
                    }
                    resource_need_pressure(
                        good,
                        context.hunger,
                        context.meal_relief,
                        context.hydration_urgency,
                    )
                });
                let need = if pressure > 0 { amount * 3 } else { 0 };
                let attachment = context
                    .concepts
                    .iter()
                    .filter(|(_, associated)| Some(**associated) == good)
                    .filter_map(|(concept, _)| values.get(concept))
                    .fold(0_i64, |total, value| {
                        total.saturating_add(i64::from(value.weight))
                    });
                let value = (i128::from(amount) * i128::from(attachment) / 1_000)
                    .clamp(i128::from(i64::MIN), i128::from(i64::MAX))
                    as i64;
                let contribution = |value: i64| {
                    if gained {
                        value
                    } else {
                        value.saturating_neg() / 2
                    }
                };
                result.benefit = result.benefit.saturating_add(contribution(amount));
                result.need = result.need.saturating_add(contribution(need));
                result.value = result.value.saturating_add(contribution(value));
            }
            Effect::Passage { beneficiary } if context.affiliations.contains(&beneficiary) => {
                result.benefit = result.benefit.saturating_add(300);
            }
            Effect::Protection { beneficiary } if context.affiliations.contains(&beneficiary) => {
                result.benefit = result.benefit.saturating_add(400);
            }
            Effect::Danger { provider } if context.affiliations.contains(&provider) => {
                result.risk = result
                    .risk
                    .saturating_sub(i64::from(1_000 - personality.risk_tolerance) / 2);
            }
            _ => {}
        }
    }
    result.total = result
        .benefit
        .saturating_add(result.need)
        .saturating_add(result.value)
        .saturating_add(result.risk);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn score<P: Clone>(evidence: &Evidence<P>, hungry: bool, thirsty: bool) -> Judgment<P> {
        assess(
            &Personality::industrious(),
            &BTreeMap::new(),
            &Context {
                affiliations: &BTreeSet::from([InstitutionId(1)]),
                hunger: if hungry { 300_000 } else { 0 },
                hunger_exempt: false,
                meal_relief: 300_000,
                meal_size: Quantity(1_000),
                hydration_urgency: if thirsty { 3_000 } else { 0 },
                drink_quantity: Quantity(250),
                concepts: &BTreeMap::new(),
            },
            evidence,
        )
    }
    fn resource<P>(key: P, good: Option<Good>, quantity: Quantity, gained: bool) -> Evidence<P> {
        Evidence {
            proposal: key,
            effects: vec![Effect::Resource {
                affiliation: InstitutionId(1),
                gained,
                good,
                quantity,
            }],
        }
    }
    #[test]
    fn food_and_water_use_their_own_doses_and_own_need_pressure() {
        let food = resource(1, Some(Good::Food), Quantity(1_000), true);
        let water = resource(2, Some(Good::Water), Quantity(250), true);
        assert_eq!(
            score(&food, true, false).need,
            score(&water, false, true).need
        );
        assert_eq!(score(&food, false, true).need, 0);
        assert_eq!(score(&water, true, false).need, 0);
        assert!(score(&water, false, true).total > score(&water, false, false).total);
        let spent = resource(3, Some(Good::Water), Quantity(250), false);
        assert!(score(&spent, false, true).total < score(&spent, false, false).total);
    }
    #[test]
    fn proposal_identity_is_opaque_and_unknown_goods_supply_no_bodily_benefit() {
        let a = resource("first", None, Quantity(250), true);
        let b = resource(vec![9, 2, 1], None, Quantity(250), true);
        let a = score(&a, true, true);
        let b = score(&b, true, true);
        assert_eq!((a.total, a.need), (b.total, b.need));
        assert_eq!(a.need, 0);
        assert_eq!(a.proposal, "first");
        assert_eq!(b.proposal, vec![9, 2, 1]);
    }
    #[test]
    fn unrelated_affiliations_do_not_gain_cost_or_risk() {
        let unrelated = Evidence {
            proposal: (),
            effects: vec![
                Effect::Resource {
                    affiliation: InstitutionId(2),
                    gained: false,
                    good: Some(Good::Water),
                    quantity: Quantity(100_000),
                },
                Effect::Danger {
                    provider: InstitutionId(2),
                },
            ],
        };
        let baseline = score(
            &Evidence {
                proposal: (),
                effects: vec![],
            },
            true,
            true,
        );
        assert_eq!(score(&unrelated, true, true), baseline);
    }
    #[test]
    fn known_associations_and_individual_risk_judgments_affect_assessment() {
        let affiliations = BTreeSet::from([InstitutionId(1)]);
        let concepts = BTreeMap::from([(ConceptId(4), Good::Water)]);
        let context = Context {
            affiliations: &affiliations,
            hunger: 0,
            hunger_exempt: true,
            meal_relief: 300_000,
            meal_size: Quantity(1_000),
            hydration_urgency: 0,
            drink_quantity: Quantity(250),
            concepts: &concepts,
        };
        let evidence = resource((), Some(Good::Water), Quantity(250), true);
        let values = BTreeMap::from([(
            ConceptId(4),
            Attachment {
                weight: 500,
                experiences: 1,
            },
        )]);
        let person = Personality::industrious();
        assert_eq!(assess(&person, &values, &context, &evidence).value, 500);
        let unknown = resource((), None, Quantity(250), true);
        assert_eq!(assess(&person, &values, &context, &unknown).value, 0);
        let danger = Evidence {
            proposal: (),
            effects: vec![Effect::Danger {
                provider: InstitutionId(1),
            }],
        };
        let mut cautious = person.clone();
        cautious.risk_tolerance = 100;
        let mut daring = person;
        daring.risk_tolerance = 900;
        assert!(
            assess(&cautious, &values, &context, &danger).total
                < assess(&daring, &values, &context, &danger).total
        );
    }
}

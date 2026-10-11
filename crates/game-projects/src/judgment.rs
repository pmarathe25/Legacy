//! Financing judgments consume remembered plans and personal resources only.
use crate::*;
use game_agreements::AgreementTerm;
use game_ecs::Store;
use game_workshop::Definitions;

#[derive(Clone, Copy, Debug)]
pub struct FinancingActorFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub seeks_independence: bool,
    pub desired_good: Good,
    pub ambition: u16,
    pub risk_tolerance: u16,
}
pub trait FinancingKnowledge {
    fn stocks(&self) -> impl Iterator<Item = &StockKnowledge>;
    fn stock(&self, inventory: InventoryId) -> Option<&StockKnowledge>;
    fn custodian(&self, inventory: InventoryId) -> Option<PersonId>;
    fn has_work_access(&self) -> bool;
}
impl Runtime {
    pub fn financing_utility(
        &self,
        store: &Store,
        definitions: &Definitions,
        actor: FinancingActorFacts,
        terms: &[AgreementTerm],
        known: &impl FinancingKnowledge,
    ) -> i64 {
        if !actor.seeks_independence || actor.ambition < 600 {
            return 0;
        }
        let own = |stock: &StockKnowledge| {
            stock.accessible && known.custodian(stock.inventory) == Some(actor.actor)
        };
        if known
            .stocks()
            .any(|stock| own(stock) && stock.good == actor.desired_good && stock.quantity.0 >= 1000)
            || !known.stocks().any(|stock| {
                own(stock) && stock.good == actor.desired_good && stock.quantity.0 == 0
            })
        {
            return 0;
        }
        let Some(memory) = knowledge(store, actor.actor) else {
            return 0;
        };
        for term in terms {
            let AgreementTerm::Delivery {
                creditor,
                source,
                quantity,
                deadline,
                ..
            } = *term
            else {
                continue;
            };
            if creditor != actor.actor || quantity.0 < 1000 {
                continue;
            }
            let Some(plan) = memory.projects.values().find(|plan| plan.output == source) else {
                continue;
            };
            let ProjectKind::Workshop { recipe: id } = &plan.kind else {
                continue;
            };
            let Ok(recipe) = definitions.recipe(id) else {
                continue;
            };
            if recipe.output_good != actor.desired_good {
                continue;
            }
            let Ok(requirement) = recipe.production_requirement(quantity) else {
                continue;
            };
            let Some(time) = self
                .rules
                .construction_labor
                .0
                .saturating_sub(plan.progress_ms)
                .checked_add(requirement.work.0)
                .and_then(|time| time.checked_add(240000))
            else {
                continue;
            };
            if deadline.0.saturating_sub(actor.now.0) < time
                || known
                    .stock(plan.tool)
                    .is_none_or(|tool| tool.good != Good::Tool || tool.quantity.0 < 1000)
            {
                continue;
            }
            if !supplies_feasible(
                plan,
                &self.rules,
                recipe.input_good,
                requirement.input,
                actor.actor,
                terms,
                known,
            ) {
                continue;
            }
            let autonomy = i64::from(actor.ambition) * if known.has_work_access() { 2 } else { 5 };
            return autonomy * i64::from(actor.risk_tolerance) / 1000;
        }
        0
    }
}
fn supplies_feasible(
    plan: &ProjectView,
    rules: &Rules,
    input_good: Good,
    input: Quantity,
    actor: PersonId,
    terms: &[AgreementTerm],
    known: &impl FinancingKnowledge,
) -> bool {
    let prepare = || -> Option<bool> {
        let mut needed = BTreeMap::<(InventoryId, Good), u64>::new();
        if !plan.materials_consumed {
            needed.insert((plan.materials, Good::Timber), rules.construction_timber.0);
        }
        let requirement = needed.entry((plan.input, input_good)).or_default();
        *requirement = requirement.checked_add(input.0)?;
        let mut delivered = BTreeMap::<(InventoryId, Good), u64>::new();
        let mut outgoing = BTreeMap::<InventoryId, u64>::new();
        let mut material_sources = BTreeSet::new();
        for term in terms {
            let AgreementTerm::Delivery {
                debtor,
                creditor,
                source,
                destination,
                quantity,
                ..
            } = *term
            else {
                continue;
            };
            if debtor != actor {
                continue;
            }
            let total = outgoing.entry(source).or_default();
            *total = total.checked_add(quantity.0)?;
            if creditor != plan.sponsor {
                continue;
            }
            let Some(stock) = known.stock(source).filter(|stock| stock.accessible) else {
                continue;
            };
            let credit = delivered.entry((destination, stock.good)).or_default();
            *credit = credit.checked_add(quantity.0)?;
            if stock.good == Good::Timber
                && (destination == plan.materials || destination == plan.input)
            {
                material_sources.insert(source);
            }
        }
        if needed
            .iter()
            .any(|(key, quantity)| delivered.get(key).copied().unwrap_or(0) < *quantity)
        {
            return Some(false);
        }
        if outgoing.iter().any(|(source, quantity)| {
            known
                .stock(*source)
                .is_none_or(|stock| !stock.accessible || stock.quantity.0 < *quantity)
        }) {
            return Some(false);
        }
        // The financier retains personally known material for their own next
        // construction. Prefer the supplied material sources as in the original
        // single-input policy; completed non-timber production may use an
        // unpledged remembered reserve instead.
        let has_reserve = if material_sources.is_empty() && plan.materials_consumed {
            known.stocks().any(|stock| {
                stock.accessible
                    && stock.good == Good::Timber
                    && stock
                        .quantity
                        .0
                        .saturating_sub(outgoing.get(&stock.inventory).copied().unwrap_or(0))
                        >= rules.construction_timber.0
            })
        } else {
            material_sources.iter().any(|source| {
                known.stock(*source).is_some_and(|stock| {
                    stock
                        .quantity
                        .0
                        .saturating_sub(outgoing.get(source).copied().unwrap_or(0))
                        >= rules.construction_timber.0
                })
            })
        };
        Some(has_reserve)
    };
    prepare().unwrap_or(false)
}
#[cfg(test)]
#[path = "judgment_tests.rs"]
mod tests;

//! Workshop equipment selection consumes Cargo's physical allocation interface.
use crate::*;
use game_cargo::ToolAllocations;
use game_ecs::Store;

#[derive(Clone, Copy, Debug)]
pub struct ToolActorFacts {
    pub alive: bool,
    pub health: u64,
    pub stationary_area: Option<AreaId>,
}
#[derive(Clone, Copy, Debug)]
pub struct ToolStockFacts {
    pub accessible: bool,
    pub actual_area: Option<AreaId>,
}
pub trait ToolSelectionFacts {
    fn actor(&self, actor: PersonId) -> Option<ToolActorFacts>;
    fn nearby(&self, actor: PersonId, area: AreaId) -> impl Iterator<Item = InventoryId>;
    fn stock(&self, actor: PersonId, stock: &Inventory) -> ToolStockFacts;
    fn allocations(&self) -> Option<ToolAllocations>;
}
pub fn selected_work_tool(store: &Store, actor: PersonId, site_id: SiteId) -> Option<InventoryId> {
    let job = job(store, actor)?;
    Some(job.tool.unwrap_or(site(store, site_id)?.tool))
}
impl Definitions {
    pub fn select_work_tool(
        &self,
        store: &Store,
        actor: PersonId,
        site_id: SiteId,
        facts: &impl ToolSelectionFacts,
    ) -> Option<InventoryId> {
        let body = facts.actor(actor)?;
        if !body.alive || body.health == 0 {
            return None;
        }
        let site = site(store, site_id)?;
        let area = body.stationary_area?;
        if area != site.area {
            return None;
        }
        let resumed = job(store, actor).filter(|job| job.site == site.id);
        let allocations = facts.allocations()?;
        let candidates = resumed
            .and_then(|job| job.tool)
            .into_iter()
            .chain(std::iter::once(site.tool))
            .chain(facts.nearby(actor, area));
        let mut visited = BTreeSet::new();
        candidates.into_iter().find(|tool| {
            if !visited.insert(*tool) {
                return false;
            }
            let Some(stock) = game_cargo::inventory(store, *tool) else {
                return false;
            };
            if stock.good != Good::Tool {
                return false;
            }
            let stock_facts = facts.stock(actor, stock);
            if !stock_facts.accessible || stock_facts.actual_area != Some(area) {
                return false;
            }
            let consumed = if resumed.is_none() && *tool == site.input {
                self.recipe(&site.recipe)
                    .expect("validated recipe")
                    .input_quantity
            } else {
                Quantity(0)
            };
            allocations.allows_after_consumption(actor, *tool, stock.quantity, consumed)
        })
    }
}

#[cfg(test)]
mod tests;

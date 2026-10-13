//! Livelihood opportunity composition from factual joins and other owners' choices.
use crate::*;
use game_cargo::Inventory;
use game_logistics::Step;

pub const TOOL_STOCK_REQUIREMENT: Quantity = Quantity(1_000);
pub fn tool_stock_usable(facts: Option<ResourceFacts>) -> bool {
    facts.is_some_and(|facts| resource_usable(facts, Good::Tool, TOOL_STOCK_REQUIREMENT))
}
pub fn construction_tool_available(facts: Option<ResourceFacts>, allocated: bool) -> bool {
    tool_stock_usable(facts) && allocated
}
#[derive(Clone, Debug)]
pub struct PlanningRequests {
    pub area: AreaId,
    pub construction: Vec<ProjectId>,
    pub enterprises: Vec<(EnterpriseId, ProjectKind, SiteId)>,
}
#[derive(Clone)]
pub struct EnterpriseOperationFacts<'a, A> {
    pub enterprise: EnterpriseId,
    /// Agriculture owns the operation itself; Projects owns its placement among
    /// the sponsor's construction, procurement, and production opportunities.
    pub cultivation: Option<A>,
    pub input: Option<ResourceFacts>,
    pub tool: Option<ResourceFacts>,
    pub output: Option<&'a Inventory>,
}
pub struct OpportunityFacts<'a, A> {
    pub actor: ActorFacts,
    pub has_mind: bool,
    pub construction: &'a [(ProjectId, ConstructionFacts)],
    pub procurement: Option<Step<A>>,
    pub enterprises: &'a [EnterpriseOperationFacts<'a, A>],
    pub stocks: &'a [WorkshopStock<'a>],
    pub plots: &'a [RememberedPlot],
    pub loading: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectOpportunity<A> {
    Action(ProjectAction),
    Procurement(Step<A>),
    Cultivation(A),
    Workshop(SiteId),
}
impl Runtime {
    pub fn planning_requests(
        &self,
        store: &Store,
        actor: ActorFacts,
        has_mind: bool,
    ) -> Option<PlanningRequests> {
        if !actor.capable || !has_mind {
            return None;
        }
        let area = actor.area?;
        Some(PlanningRequests {
            area,
            construction: self.construction_candidates(store, actor.actor),
            enterprises: self
                .sponsored_enterprises(store, actor.actor, area)
                .into_iter()
                .map(|id| {
                    let asset = enterprise(store, id).expect("selected enterprise exists");
                    (id, asset.kind.clone(), asset.site)
                })
                .collect(),
        })
    }
    pub fn opportunities<A: Clone>(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        facts: OpportunityFacts<'_, A>,
    ) -> Vec<ProjectOpportunity<A>> {
        let Some(requests) = self.planning_requests(store, facts.actor, facts.has_mind) else {
            return Vec::new();
        };
        let mut opportunities = Vec::new();
        for project in requests.construction {
            if facts
                .construction
                .iter()
                .find(|(id, _)| *id == project)
                .is_some_and(|(_, construction)| {
                    construction.actor.actor == facts.actor.actor
                        && self
                            .prepare_construction(store, project, *construction)
                            .is_ok()
                })
            {
                opportunities.push(ProjectOpportunity::Action(ProjectAction::Work { project }));
            }
        }
        if let Some(step) = facts.procurement.and_then(Procurement::active_step) {
            opportunities.push(ProjectOpportunity::Procurement(step));
        }
        for (id, kind, site) in requests.enterprises {
            let Some(operation) = facts
                .enterprises
                .iter()
                .find(|operation| operation.enterprise == id)
            else {
                continue;
            };
            if matches!(kind, ProjectKind::Farm { .. }) {
                if let Some(cultivation) = &operation.cultivation {
                    opportunities.push(ProjectOpportunity::Cultivation(cultivation.clone()));
                }
            } else {
                let Ok(recipe) = game_workshop::site(store, site)
                    .ok_or(Error::Unavailable)
                    .and_then(|site| definitions.resolve_site(site))
                else {
                    continue;
                };
                if operation.output.is_some_and(|output| {
                    recipe.operation_candidate(
                        operation.input.is_some_and(|input| {
                            resource_usable(
                                input,
                                recipe.recipe.input_good,
                                recipe.recipe.input_quantity,
                            )
                        }),
                        tool_stock_usable(operation.tool),
                        output,
                    )
                }) {
                    opportunities.push(ProjectOpportunity::Workshop(site));
                }
            }
        }
        let prospect = WorkshopProspectFacts {
            actor: facts.actor.actor,
            area: requests.area,
            now: facts.actor.now,
            capable: facts.actor.capable,
            loading: facts.loading,
        };
        opportunities.extend(
            self.farm_prospects(store, prospect, facts.stocks, facts.plots)
                .into_iter()
                .map(ProjectOpportunity::Action),
        );
        opportunities.extend(
            self.workshop_prospects(store, definitions, prospect, facts.stocks)
                .unwrap_or_default()
                .into_iter()
                .map(ProjectOpportunity::Action),
        );
        opportunities
    }
}
impl Procurement {
    pub fn minimum_handoff(&self) -> Quantity {
        Quantity(1)
    }
    pub fn active_step<A>(step: Step<A>) -> Option<Step<A>> {
        match step {
            Step::Arrived { .. } | Step::Complete => None,
            step => Some(step),
        }
    }
}

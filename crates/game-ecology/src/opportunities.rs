//! Executable local choices and personally remembered exploration.
use crate::*;
use game_ecs::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatherOpportunity {
    pub resource: ResourceId,
    pub good: Good,
    pub upper_bound: Quantity,
    pub unauthorized: bool,
    pub suggested: Vec<Quantity>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutonomousOptions {
    pub actions: Vec<EcologyAction>,
    pub destinations: Vec<AreaId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionOpportunity {
    pub action: AcquisitionAction,
    pub requirement: game_logistics::Requirement,
}
impl Runtime {
    /// Ecology derives provision goals from personally remembered source kinds.
    /// Characters interprets these requirements as motives, without rescanning
    /// ecological knowledge or inventing source-specific target quantities.
    pub fn provision_opportunities(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        local_actions: &[EcologyAction],
        meal_size: Quantity,
        reachable: impl FnMut(AreaId) -> bool,
    ) -> Vec<ProvisionOpportunity> {
        let options = self.autonomous_options(store, actor, area, local_actions, reachable);
        let Some(known) = self.knowledge(store, actor) else {
            return Vec::new();
        };
        let mut output = Vec::new();
        let mut append = |action: AcquisitionAction, goods: BTreeSet<Good>| {
            for good in goods {
                output.push(ProvisionOpportunity {
                    action: action.clone(),
                    requirement: game_logistics::Requirement {
                        good,
                        quantity: if good == Good::Timber {
                            Quantity(5_000)
                        } else {
                            meal_size
                        },
                        source: None,
                    },
                });
            }
        };
        for action in options.actions {
            let resource = match &action {
                EcologyAction::Gather { resource, .. }
                | EcologyAction::RequestAccess { resource } => *resource,
                EcologyAction::GrantAccess { .. } => continue,
            };
            let goods = known
                .known_resources
                .get(&resource)
                .map(|source| BTreeSet::from([source.kind.good()]))
                .unwrap_or_default();
            append(AcquisitionAction::Resource(action), goods);
        }
        for destination in options.destinations {
            let goods = known
                .known_resources
                .values()
                .filter(|source| source.area == destination && source.available.0 > 0)
                .map(|source| source.kind.good())
                .collect();
            append(AcquisitionAction::Explore { destination }, goods);
        }
        output
    }
    /// Quantity suggestions come from their owner; ecology admits each amount
    /// against actual stock, workspaces, gear, and timing before offering it.
    pub fn local_gather_opportunities(
        &self,
        store: &Store,
        actor: PersonId,
        facts: GatherFacts,
        mut quantity_candidates: impl FnMut(Good, Quantity) -> Vec<Quantity>,
    ) -> Vec<GatherOpportunity> {
        let mut opportunities = Vec::new();
        for (_, source) in self
            .resources(store)
            .filter(|(_, row)| row.area == facts.area)
            .take(16)
        {
            let Ok(upper_bound) = self.gather_upper_bound(store, actor, source.id, facts) else {
                continue;
            };
            let unauthorized = !source.access.contains(&actor);
            let suggested: Vec<_> = quantity_candidates(source.kind.good(), upper_bound)
                .into_iter()
                .filter(|quantity| {
                    self.prepare_gather(store, actor, source.id, *quantity, unauthorized, facts)
                        .is_ok()
                })
                .collect();
            if !suggested.is_empty() {
                opportunities.push(GatherOpportunity {
                    resource: source.id,
                    good: source.kind.good(),
                    upper_bound,
                    unauthorized,
                    suggested,
                });
            }
        }
        opportunities
    }
    /// Discovery intentionally includes cooling-down requests. The request
    /// transition and acquisition planning separately enforce its cooldown.
    pub fn local_access_actions(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        mut actual_area: impl FnMut(PersonId) -> Option<AreaId>,
    ) -> Vec<EcologyAction> {
        self.resources(store)
            .filter(|(_, source)| source.area == area)
            .take(16)
            .filter(|(_, source)| {
                !source.access.contains(&actor)
                    && source.custodian.is_some_and(|custodian| {
                        custodian != actor && actual_area(custodian) == Some(area)
                    })
            })
            .map(|(resource, _)| EcologyAction::RequestAccess { resource })
            .collect()
    }
    /// Local actions need personal source knowledge. Remote travel is based on
    /// remembered stock and routes deemed reachable by the route owner.
    pub fn autonomous_options(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        local_actions: &[EcologyAction],
        mut reachable: impl FnMut(AreaId) -> bool,
    ) -> AutonomousOptions {
        if self.gathering(store, actor).is_some() {
            return AutonomousOptions::default();
        }
        let Some(known) = self.knowledge(store, actor) else {
            return AutonomousOptions::default();
        };
        let actions = local_actions
            .iter()
            .filter(|action| match action {
                EcologyAction::Gather { resource, .. }
                | EcologyAction::RequestAccess { resource } => {
                    known.known_resources.contains_key(resource)
                }
                EcologyAction::GrantAccess { .. } => false,
            })
            .cloned()
            .collect();
        let destinations = known
            .known_resources
            .values()
            .filter(|source| {
                source.available.0 > 0 && source.area != area && reachable(source.area)
            })
            .map(|source| source.area)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        AutonomousOptions {
            actions,
            destinations,
        }
    }
}
#[cfg(test)]
#[path = "opportunity_tests.rs"]
mod tests;

//! Project agency, remembered opportunities, and procurement choices.
use crate::*;
use game_agreements::{AgreementStatus, AgreementView};
use game_logistics::{Delivery, Destination, KnownDestination, Requirement};
use game_workshop::Definitions;

#[derive(Clone, Copy, Debug)]
pub struct AgencyFacts {
    pub alive: bool,
    pub health: u64,
    pub area: Option<AreaId>,
    pub independent: bool,
    pub combat_restricted: bool,
    pub caring: bool,
    pub stage: Option<LifeStage>,
}
pub fn project_area(facts: AgencyFacts) -> Option<AreaId> {
    (facts.alive && facts.health > 0)
        .then_some(facts.area)
        .flatten()
}
pub fn project_capable(facts: AgencyFacts) -> bool {
    project_area(facts).is_some()
        && facts.independent
        && !facts.combat_restricted
        && !facts.caring
        && facts
            .stage
            .is_none_or(|stage| matches!(stage, LifeStage::Adult | LifeStage::Elder))
}
#[derive(Clone, Copy)]
pub struct ResourceFacts {
    pub good: Good,
    pub accessible: bool,
    pub area: Option<AreaId>,
    pub actor_area: Option<AreaId>,
    pub unreserved: Quantity,
}
pub fn construction_fatigue_allows(fatigue: Option<u64>, limit: u64) -> bool {
    fatigue.is_some_and(|fatigue| fatigue < limit)
}
pub fn resource_usable(facts: ResourceFacts, good: Good, quantity: Quantity) -> bool {
    facts.good == good
        && facts.accessible
        && facts.area == facts.actor_area
        && (quantity.0 == 0 || facts.unreserved >= quantity)
}
impl Runtime {
    pub fn construction_candidates(&self, store: &Store, actor: PersonId) -> Vec<ProjectId> {
        let mut rows = projects(store).collect::<Vec<_>>();
        rows.reverse();
        rows.into_iter()
            .filter(|(id, plan)| {
                plan.workers.contains(&actor)
                    && knowledge(store, actor).is_some_and(|known| known.projects.contains_key(id))
            })
            .take(8)
            .map(|(id, _)| id)
            .collect()
    }
    pub fn available_action_candidates(&self, store: &Store, actor: PersonId) -> Vec<ProjectId> {
        projects(store)
            .filter(|(id, _)| {
                knowledge(store, actor).is_some_and(|known| known.projects.contains_key(id))
            })
            .take(16)
            .map(|(id, _)| id)
            .collect()
    }
    pub fn available_actions(
        &self,
        store: &Store,
        actor: ActorFacts,
        construction: &[(ProjectId, ConstructionFacts)],
    ) -> Vec<ProjectAction> {
        if !actor.capable {
            return Vec::new();
        }
        let mut actions = Vec::new();
        for id in self.available_action_candidates(store, actor.actor) {
            let plan = project(store, id).expect("selected project exists");
            if construction
                .iter()
                .find(|(project, _)| *project == id)
                .is_some_and(|(_, facts)| {
                    facts.actor.actor == actor.actor
                        && self.prepare_construction(store, id, *facts).is_ok()
                })
            {
                actions.push(ProjectAction::Work { project: id });
            }
            if plan.sponsor == actor.actor
                && !matches!(
                    plan.status,
                    ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
                )
            {
                actions.push(ProjectAction::Cancel { project: id });
            }
        }
        actions
    }
    pub fn can_seek_new_project(&self, store: &Store, actor: PersonId, now: SimTime) -> bool {
        !knowledge(store, actor).is_some_and(|known| {
            known.projects.values().any(|plan| {
                plan.sponsor == actor
                    && !matches!(
                        plan.status,
                        ProjectStatus::Cancelled | ProjectStatus::Failed
                    )
            })
        }) && !requests(store, actor).is_some_and(|row| row.next_search > now)
    }
    pub fn farm_prospects(
        &self,
        store: &Store,
        facts: WorkshopProspectFacts,
        stocks: &[WorkshopStock<'_>],
        plots: &[RememberedPlot],
    ) -> Vec<ProjectAction> {
        if !facts.capable || !self.can_seek_new_project(store, facts.actor, facts.now) {
            return Vec::new();
        }
        let choose = |good, minimum| {
            stocks
                .iter()
                .filter(|stock| {
                    stock.known.area == facts.area
                        && stock.at_area == Some(facts.area)
                        && stock.custodian == Some(facts.actor)
                        && stock.known.good == good
                        && stock.known.accessible
                        && stock.known.quantity.0 >= minimum
                })
                .min_by_key(|stock| stock.known.inventory)
                .map(|stock| stock.known.inventory)
        };
        let Some(plot) = plots
            .iter()
            .filter(|plot| plot.area == facts.area && plot.accessible)
            .min_by_key(|plot| plot.id)
        else {
            return Vec::new();
        };
        let (Some(materials), Some(tool), Some(seed), Some(water), Some(food)) = (
            choose(Good::Timber, 0),
            choose(Good::Tool, 1_000),
            choose(Good::Seed, 0),
            choose(Good::Water, 0),
            choose(Good::Food, 0),
        ) else {
            return Vec::new();
        };
        vec![ProjectAction::ProposeFarm {
            plot: plot.id,
            materials,
            tool,
            seed,
            water,
            food,
            beneficiaries: BTreeSet::from([facts.actor]),
            workers: BTreeSet::from([facts.actor]),
            finish_by: SimTime(
                facts.now.0.saturating_add(
                    self.rules
                        .construction_labor
                        .0
                        .saturating_mul(4)
                        .saturating_add(600_000),
                ),
            ),
        }]
    }
    pub fn sponsored_enterprises(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
    ) -> Vec<EnterpriseId> {
        enterprises(store)
            .filter(|(_, asset)| asset.sponsor == actor && asset.area == area)
            .map(|(id, _)| id)
            .collect()
    }
    pub fn procurement_target(
        &self,
        store: &Store,
        definitions: &Definitions,
        actor: PersonId,
        agreements: &[AgreementView],
    ) -> Option<ProcurementTarget> {
        let plan = knowledge(store, actor)?
            .projects
            .values()
            .rev()
            .find(|plan| {
                matches!(plan.kind, ProjectKind::Workshop { .. })
                    && plan.sponsor == actor
                    && !matches!(
                        plan.status,
                        ProjectStatus::Failed | ProjectStatus::Cancelled
                    )
            })?;
        if !plan.materials_consumed && plan.support.iter().any(|id| {
            agreements.iter().find(|agreement| agreement.id == *id).is_some_and(|agreement| {
                agreement.status == AgreementStatus::Proposed || agreement.obligations.iter().any(|row| {
                    matches!(row.term, game_agreements::AgreementTerm::Delivery { destination, .. }
                        if destination == plan.materials) && !row.defaulted && row.performed < row.term.amount()
                })
            })
        }) { return None; }
        let (inventory, required) = if plan.materials_consumed {
            let ProjectKind::Workshop { recipe } = &plan.kind else {
                return None;
            };
            (plan.input, definitions.recipe(recipe).ok()?.input_quantity)
        } else {
            (plan.materials, self.rules.construction_timber)
        };
        Some(ProcurementTarget {
            inventory,
            required,
            observed_at: plan.observed_at,
        })
    }
    pub fn procurement(
        &self,
        store: &Store,
        target: ProcurementTarget,
        facts: ProcurementFacts<'_>,
    ) -> Option<Procurement> {
        let ProcurementFacts {
            actor,
            stocks,
            destination,
            observed_location,
            now,
            actual_quantity,
        } = facts;
        let fact = stocks.get(&target.inventory)?;
        let available =
            if destination.observed_at == now && &destination.location == observed_location {
                actual_quantity
            } else {
                fact.quantity
            };
        let missing = target.required.0.saturating_sub(available.0);
        if missing == 0 {
            return None;
        }
        let delivery = Delivery::new(
            Requirement {
                good: fact.good,
                quantity: Quantity(missing),
                source: None,
            },
            Destination::Inventory(target.inventory),
        )
        .ok()?;
        let hints = knowledge(store, actor)?
            .timber_hints
            .iter()
            .filter(|area| {
                !stocks
                    .values()
                    .any(|fact| fact.area == **area && fact.good == delivery.requirement.good)
            })
            .copied()
            .collect();
        Some(Procurement {
            delivery,
            hints,
            observed_at: target.observed_at,
        })
    }
}
#[derive(Clone, Copy)]
pub struct RememberedPlot {
    pub id: SiteId,
    pub area: AreaId,
    pub accessible: bool,
}
#[derive(Clone, Copy)]
pub struct ProcurementTarget {
    pub inventory: InventoryId,
    pub required: Quantity,
    pub observed_at: SimTime,
}
pub struct ProcurementFacts<'a> {
    pub actor: PersonId,
    pub stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
    pub destination: &'a KnownDestination,
    pub observed_location: &'a Location,
    pub now: SimTime,
    pub actual_quantity: Quantity,
}
pub struct Procurement {
    pub delivery: Delivery,
    pub hints: Vec<AreaId>,
    pub observed_at: SimTime,
}

/// A person's remembered proposal names this plot; this does not consult live
/// project readiness, materials, or the remotely visible land itself.
pub fn remembered_farm_plan(store: &Store, actor: PersonId, plot: SiteId) -> bool {
    knowledge(store, actor).is_some_and(|known| {
        known.projects.values().any(
            |plan| matches!(plan.kind, ProjectKind::Farm { plot: source, .. } if source == plot),
        )
    })
}
/// Established-site evidence remains the observer's dated personal knowledge.
pub fn remembered_established_site(store: &Store, actor: PersonId, site: SiteId) -> bool {
    knowledge(store, actor)
        .is_some_and(|known| known.enterprises.values().any(|asset| asset.site == site))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_agency_rejects_death_childhood_care_and_restrictions() {
        let facts = AgencyFacts {
            alive: true,
            health: 1,
            area: Some(AreaId(1)),
            independent: true,
            combat_restricted: false,
            caring: false,
            stage: Some(LifeStage::Adult),
        };
        assert!(project_capable(facts));
        for changed in [
            AgencyFacts {
                alive: false,
                ..facts
            },
            AgencyFacts { health: 0, ..facts },
            AgencyFacts {
                area: None,
                ..facts
            },
            AgencyFacts {
                independent: false,
                ..facts
            },
            AgencyFacts {
                combat_restricted: true,
                ..facts
            },
            AgencyFacts {
                caring: true,
                ..facts
            },
            AgencyFacts {
                stage: Some(LifeStage::Child),
                ..facts
            },
        ] {
            assert!(!project_capable(changed));
        }
    }
}

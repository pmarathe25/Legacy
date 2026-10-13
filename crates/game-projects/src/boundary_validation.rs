//! Save references and replay evidence from factual collaborator projections.
use crate::*;
use game_cargo::{InventoryLocation, inventory};

#[derive(Clone, Copy)]
pub struct FarmFieldFacts {
    pub area: AreaId,
    pub seed: InventoryId,
    pub food: InventoryId,
    pub tool: InventoryId,
    pub water: InventoryId,
}
#[derive(Clone, Copy)]
pub struct ConstructionActivity {
    pub project: ProjectId,
    pub completes: SimTime,
}
#[derive(Clone, Copy)]
pub enum AccountingEvent {
    ConstructionStarted {
        project: ProjectId,
    },
    Produced {
        sequence: u64,
        site: SiteId,
        good: Good,
        quantity: Quantity,
    },
    InputUsed {
        enterprise: EnterpriseId,
        site: SiteId,
        quantity: Quantity,
    },
}
pub struct ValidationFacts {
    pub now: SimTime,
    pub next_id: u64,
    pub next_event: u64,
    pub dependencies_present: bool,
    pub actors: BTreeSet<PersonId>,
    pub areas: BTreeSet<AreaId>,
    pub actor_locations: BTreeMap<PersonId, Location>,
    pub alive: BTreeSet<PersonId>,
    pub construction_activities: BTreeMap<PersonId, ConstructionActivity>,
    pub scheduled: BTreeSet<(SimTime, PersonId)>,
    pub plots: BTreeMap<SiteId, AreaId>,
    pub fields: BTreeMap<SiteId, FarmFieldFacts>,
    pub events: Vec<AccountingEvent>,
}
impl Runtime {
    pub fn validate_composition(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        facts: &ValidationFacts,
    ) -> Result<(), Error> {
        let bad = || Error::InvalidSave("invalid workshop project or enterprise".into());
        if !facts.dependencies_present
            || self.last_event >= facts.next_event
            || knowledges(store).any(|(actor, row)| {
                !facts.actors.contains(&actor)
                    || row
                        .timber_hints
                        .iter()
                        .any(|area| !facts.areas.contains(area))
            })
            || request_rows(store).any(|(actor, _)| !facts.actors.contains(&actor))
        {
            return Err(bad());
        }
        self.validate_owned(store, definitions, facts.now)
            .map_err(|_| bad())?;
        for (id, plan) in projects(store) {
            if id != plan.id
                || id.0 >= facts.next_id
                || !facts.actors.contains(&plan.sponsor)
                || !facts.areas.contains(&plan.area)
                || plan
                    .beneficiaries
                    .iter()
                    .chain(&plan.workers)
                    .chain(plan.contributions.keys())
                    .any(|actor| !facts.actors.contains(actor))
                || plan.support.iter().any(|id| {
                    game_agreements::agreement(store, *id)
                        .is_none_or(|agreement| !agreement.parties.contains(&plan.sponsor))
                })
            {
                return Err(bad());
            }
            let (input_good, output_good) = match &plan.kind {
                ProjectKind::Farm { .. } => (Good::Seed, Good::Food),
                ProjectKind::Workshop { recipe } => {
                    let recipe = definitions.recipe(recipe).map_err(|_| bad())?;
                    (recipe.input_good, recipe.output_good)
                }
            };
            for (id, good) in [
                (plan.materials, Good::Timber),
                (plan.tool, Good::Tool),
                (plan.input, input_good),
                (plan.output, output_good),
            ] {
                if inventory(store, id).is_none_or(|stock| {
                    stock.good != good || stock.location != InventoryLocation::Area(plan.area)
                }) {
                    return Err(bad());
                }
            }
            if let ProjectKind::Farm { plot, water } = plan.kind {
                let live_plot = facts.plots.get(&plot) == Some(&plan.area);
                let converted = matches!(
                    plan.status,
                    ProjectStatus::Cancelled | ProjectStatus::Failed
                ) && facts
                    .fields
                    .get(&plot)
                    .is_some_and(|field| field.area == plan.area)
                    && enterprises(store).any(|(_, asset)| {
                        asset.project != plan.id
                            && asset.site == plot
                            && asset.area == plan.area
                            && matches!(asset.kind,
                            ProjectKind::Farm { plot: source, .. } if source == plot)
                    });
                if inventory(store, water).is_none_or(|stock| {
                    stock.good != Good::Water
                        || stock.location != InventoryLocation::Area(plan.area)
                }) || plan.enterprise.is_none() && !live_plot && !converted
                {
                    return Err(bad());
                }
            }
            if plan.materials_consumed
                && !facts.events.iter().any(|event| {
                    matches!(event,
                AccountingEvent::ConstructionStarted { project } if *project == id)
                })
            {
                return Err(bad());
            }
        }
        for (actor, job) in constructions(store) {
            let plan = project(store, job.project).ok_or_else(bad)?;
            if !facts.actors.contains(&actor)
                || facts.actor_locations.get(&actor) != Some(&Location::AtArea(plan.area))
                || !facts.alive.contains(&actor)
                || !facts
                    .construction_activities
                    .get(&actor)
                    .is_some_and(|activity| {
                        activity.project == job.project && activity.completes == job.completes
                    })
                || !facts.scheduled.contains(&(job.completes, actor))
            {
                return Err(bad());
            }
        }
        if facts
            .construction_activities
            .keys()
            .any(|actor| construction(store, *actor).is_none())
        {
            return Err(bad());
        }
        for (id, asset) in enterprises(store) {
            let plan = project(store, asset.project).ok_or_else(bad)?;
            if id.0 >= facts.next_id
                || asset
                    .labor
                    .keys()
                    .any(|actor| !facts.actors.contains(actor))
                || asset.operating_agreements.iter().any(|id| {
                    game_agreements::agreement(store, *id)
                        .is_none_or(|agreement| !agreement.parties.contains(&asset.sponsor))
                })
            {
                return Err(bad());
            }
            if let ProjectKind::Farm { plot, water } = plan.kind {
                let field = facts.fields.get(&plot).ok_or_else(bad)?;
                if asset.site != plot
                    || field.area != plan.area
                    || field.seed != plan.input
                    || field.food != plan.output
                    || field.tool != plan.tool
                    || field.water != water
                {
                    return Err(bad());
                }
            } else {
                let site = game_workshop::site(store, asset.site).ok_or_else(bad)?;
                if site.area != plan.area
                    || site.input != plan.input
                    || site.tool != plan.tool
                    || site.output != plan.output
                    || !matches!(&plan.kind, ProjectKind::Workshop { recipe } if &site.recipe == recipe)
                    || definitions.resolve_site(site).is_err()
                {
                    return Err(bad());
                }
            }
            let mut produced = BTreeMap::<Good, u64>::new();
            let mut inputs = BTreeMap::<Good, u64>::new();
            for event in &facts.events {
                match *event {
                    AccountingEvent::Produced {
                        sequence,
                        site,
                        good,
                        quantity,
                    } if sequence <= self.last_event
                        && site == asset.site
                        && (quantity.0 > 0 || matches!(asset.kind, ProjectKind::Farm { .. })) =>
                    {
                        let total = produced.entry(good).or_default();
                        *total = total.checked_add(quantity.0).ok_or_else(bad)?;
                    }
                    AccountingEvent::InputUsed {
                        enterprise,
                        site,
                        quantity,
                    } if enterprise == id
                        && site == asset.site
                        && quantity.0 > 0
                        && matches!(asset.kind, ProjectKind::Workshop { .. }) =>
                    {
                        let ProjectKind::Workshop { recipe } = &asset.kind else {
                            unreachable!()
                        };
                        let good = definitions.recipe(recipe).map_err(|_| bad())?.input_good;
                        let total = inputs.entry(good).or_default();
                        *total = total.checked_add(quantity.0).ok_or_else(bad)?;
                    }
                    _ => {}
                }
            }
            if produced != asset.produced
                || matches!(asset.kind, ProjectKind::Workshop { .. }) && inputs != asset.inputs
            {
                return Err(bad());
            }
        }
        for (_, known) in knowledges(store) {
            for (id, view) in &known.projects {
                let plan = project(store, *id).ok_or_else(bad)?;
                if *id != view.id
                    || view.kind != plan.kind
                    || view.sponsor != plan.sponsor
                    || view.area != plan.area
                    || view.materials != plan.materials
                    || view.tool != plan.tool
                    || view.input != plan.input
                    || view.output != plan.output
                    || view.beneficiaries != plan.beneficiaries
                    || view.workers != plan.workers
                    || !view.support.is_subset(&plan.support)
                    || view.progress_ms > plan.progress_ms
                    || view.observed_at > facts.now
                    || view.materials_consumed && !plan.materials_consumed
                    || view.enterprise.is_some() && view.enterprise != plan.enterprise
                {
                    return Err(bad());
                }
            }
            for (id, view) in &known.enterprises {
                let asset = enterprise(store, *id).ok_or_else(bad)?;
                if *id != view.id
                    || view.kind != asset.kind
                    || view.project != asset.project
                    || view.sponsor != asset.sponsor
                    || view.area != asset.area
                    || view.site != asset.site
                    || view.observed_at > facts.now
                    || view.beneficiaries != asset.beneficiaries
                    || view.workers != asset.workers
                {
                    return Err(bad());
                }
                if let Some(accounts) = &view.accounts
                    && (accounts.inputs.iter().any(|(good, quantity)| {
                        asset
                            .inputs
                            .get(good)
                            .is_none_or(|actual| quantity > actual)
                    }) || accounts.produced.iter().any(|(good, quantity)| {
                        asset
                            .produced
                            .get(good)
                            .is_none_or(|actual| quantity > actual)
                    }) || !accounts
                        .operating_agreements
                        .is_subset(&asset.operating_agreements)
                        || accounts.labor.iter().any(|(actor, elapsed)| {
                            asset.labor.get(actor).is_none_or(|actual| elapsed > actual)
                        }))
                {
                    return Err(bad());
                }
            }
        }
        Ok(())
    }
}

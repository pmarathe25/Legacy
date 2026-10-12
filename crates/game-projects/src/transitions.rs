use super::*;
use game_agreements::{Agreement, AgreementStatus, AgreementTerm};

#[derive(Clone, Copy, Debug)]
pub struct ActorFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub area: Option<AreaId>,
    pub capable: bool,
    pub knows_project: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct ConstructionFacts {
    pub actor: ActorFacts,
    pub land_accessible: bool,
    pub tool_available: bool,
    pub materials_available: Quantity,
}
#[derive(Clone, Copy, Debug)]
pub struct StockFact {
    pub good: Good,
    pub area: Option<AreaId>,
}
pub struct ProposalFacts<'a> {
    pub actor: ActorFacts,
    pub people: &'a BTreeSet<PersonId>,
    pub stocks: &'a BTreeMap<InventoryId, StockFact>,
    pub land_area: Option<AreaId>,
    pub land_accessible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkshopAsset {
    pub recipe: game_workshop::RecipeId,
    pub site: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub tool: InventoryId,
    pub output: InventoryId,
    pub access: BTreeSet<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetEffect {
    Workshop(WorkshopAsset),
    Farm {
        plot: SiteId,
        area: AreaId,
        seed: InventoryId,
        tool: InventoryId,
        water: InventoryId,
        food: InventoryId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    InputUsed {
        enterprise: EnterpriseId,
        site: SiteId,
        quantity: Quantity,
    },
    Interrupt {
        actor: PersonId,
    },
    Consume {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Proposed {
        project: ProjectId,
        sponsor: PersonId,
    },
    Supported {
        project: ProjectId,
        actor: PersonId,
        agreement: AgreementId,
    },
    ConstructionStarted {
        project: ProjectId,
        actor: PersonId,
        completes: SimTime,
    },
    Cancelled {
        project: ProjectId,
        actor: PersonId,
    },
    Failed {
        project: ProjectId,
    },
    Completed {
        project: ProjectId,
        enterprise: EnterpriseId,
        site: SiteId,
        actor: PersonId,
        asset: AssetEffect,
    },
}

/// Sparse native row edits and the exact owner read guards used to prepare them.
pub struct PreparedChange {
    rules: Rules,
    pub(crate) projects: BTreeMap<ProjectId, (Option<Project>, Option<Project>)>,
    pub(crate) enterprises: BTreeMap<EnterpriseId, (Option<Enterprise>, Option<Enterprise>)>,
    jobs: BTreeMap<PersonId, (Option<Construction>, Option<Construction>)>,
    job_membership: Option<Vec<(PersonId, Construction)>>,
    project_job_membership: BTreeMap<ProjectId, Vec<PersonId>>,
    area_slots: BTreeMap<AreaId, Vec<ProjectId>>,
    farm_slots: BTreeMap<SiteId, Vec<ProjectId>>,
    enterprise_sites: BTreeMap<SiteId, Vec<EnterpriseId>>,
    recipe_reads: BTreeMap<game_workshop::RecipeId, game_workshop::Recipe>,
    cursor: Option<(u64, u64)>,
    effects: Vec<Effect>,
}
fn area_slots(store: &Store, area: AreaId) -> Vec<ProjectId> {
    projects(store)
        .filter(|(_, row)| row.area == area && row.materials_consumed)
        .map(|(id, _)| id)
        .collect()
}
fn farm_slots(store: &Store, plot: SiteId) -> Vec<ProjectId> {
    projects(store)
        .filter(|(_, row)| {
            matches!(&row.kind,ProjectKind::Farm{plot:id,..} if *id==plot)
                && !matches!(row.status, ProjectStatus::Failed | ProjectStatus::Cancelled)
        })
        .map(|(id, _)| id)
        .collect()
}
fn enterprise_sites(store: &Store, site: SiteId) -> Vec<EnterpriseId> {
    enterprises(store)
        .filter(|(_, row)| row.site == site)
        .map(|(id, _)| id)
        .collect()
}
impl PreparedChange {
    pub(crate) fn new(runtime: &Runtime) -> Self {
        Self {
            rules: runtime.rules.clone(),
            projects: BTreeMap::new(),
            enterprises: BTreeMap::new(),
            jobs: BTreeMap::new(),
            job_membership: None,
            project_job_membership: BTreeMap::new(),
            area_slots: BTreeMap::new(),
            farm_slots: BTreeMap::new(),
            enterprise_sites: BTreeMap::new(),
            recipe_reads: BTreeMap::new(),
            cursor: None,
            effects: Vec::new(),
        }
    }
    pub(crate) fn project(&mut self, store: &Store, id: ProjectId, after: Project) {
        self.projects
            .insert(id, (project(store, id).cloned(), Some(after)));
    }
    pub(crate) fn enterprise(&mut self, store: &Store, id: EnterpriseId, after: Enterprise) {
        self.enterprises
            .insert(id, (enterprise(store, id).cloned(), Some(after)));
    }
    fn job(&mut self, store: &Store, actor: PersonId, after: Option<Construction>) {
        self.jobs
            .insert(actor, (construction(store, actor).cloned(), after));
    }
    pub(crate) fn guard_jobs(&mut self, store: &Store) {
        self.job_membership = Some(
            constructions(store)
                .map(|(actor, row)| (actor, row.clone()))
                .collect(),
        );
    }
    fn guard_area_slots(&mut self, store: &Store, area: AreaId) {
        self.area_slots.insert(area, area_slots(store, area));
    }
    fn guard_farm_slots(&mut self, store: &Store, plot: SiteId) {
        self.farm_slots.insert(plot, farm_slots(store, plot));
    }
    pub(crate) fn guard_enterprise_sites(&mut self, store: &Store, site: SiteId) {
        self.enterprise_sites
            .insert(site, enterprise_sites(store, site));
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub(crate) fn validate_rows(&self, runtime: &Runtime, store: &Store) -> Result<(), Error> {
        if self.projects.iter().any(|(id, (before, after))| {
            before.is_none() && after.is_some() && store.contains(StableId(id.0))
        }) || self.enterprises.iter().any(|(id, (before, after))| {
            before.is_none() && after.is_some() && store.contains(StableId(id.0))
        }) || runtime.rules != self.rules
            || self
                .cursor
                .is_some_and(|(before, _)| runtime.last_event != before)
            || self
                .projects
                .iter()
                .any(|(id, (before, _))| project(store, *id) != before.as_ref())
            || self
                .enterprises
                .iter()
                .any(|(id, (before, _))| enterprise(store, *id) != before.as_ref())
            || self
                .jobs
                .iter()
                .any(|(actor, (before, _))| construction(store, *actor) != before.as_ref())
            || self.job_membership.as_ref().is_some_and(|before| {
                constructions(store)
                    .map(|(actor, row)| (actor, row.clone()))
                    .collect::<Vec<_>>()
                    != *before
            })
            || self.project_job_membership.iter().any(|(project, before)| {
                constructions(store)
                    .filter(|(_, job)| job.project == *project)
                    .map(|(actor, _)| actor)
                    .collect::<Vec<_>>()
                    != *before
            })
            || self
                .area_slots
                .iter()
                .any(|(area, before)| area_slots(store, *area) != *before)
            || self
                .farm_slots
                .iter()
                .any(|(plot, before)| farm_slots(store, *plot) != *before)
            || self
                .enterprise_sites
                .iter()
                .any(|(site, before)| enterprise_sites(store, *site) != *before)
        {
            return Err(Error::Unavailable);
        }
        for (id, (_, after)) in &self.projects {
            if let Some(row) = after {
                row.validate()?;
                if row.id != *id || row.progress_ms > runtime.rules.construction_labor.0 {
                    return Err(Error::Unavailable);
                }
            }
        }
        for (id, (_, after)) in &self.enterprises {
            if let Some(row) = after {
                row.validate()?;
                if row.id != *id {
                    return Err(Error::Unavailable);
                }
            }
        }
        for (_, after) in self.jobs.values() {
            if let Some(row) = after {
                row.validate()?;
            }
        }
        Ok(())
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        definitions: &game_workshop::Definitions,
    ) -> Result<(), Error> {
        self.validate_rows(runtime, store)?;
        for (id, before) in &self.recipe_reads {
            if definitions.recipe(id)? != before {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub(crate) fn stage_rows(
        mut self,
        runtime: &Runtime,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ProjectFinalization, Error> {
        self.validate_rows(runtime, store)?;
        let mut writes = game_ecs::ReadFootprint::new();
        for id in self.projects.keys() {
            writes.protect::<Project>(StableId(id.0));
        }
        for id in self.enterprises.keys() {
            writes.protect::<Enterprise>(StableId(id.0));
        }
        for id in self.jobs.keys() {
            writes.protect::<Construction>(StableId(id.0));
        }
        if !writes.is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        for (id, (_, after)) in &self.projects {
            if let Some(row) = after {
                batch.insert(StableId(id.0), row.clone());
            } else {
                batch.remove::<Project>(StableId(id.0));
            }
        }
        for (id, (_, after)) in &self.enterprises {
            if let Some(row) = after {
                batch.insert(StableId(id.0), row.clone());
            } else {
                batch.remove::<Enterprise>(StableId(id.0));
            }
        }
        for (actor, (_, after)) in &self.jobs {
            if let Some(row) = after {
                batch.insert(StableId(actor.0), row.clone());
            } else {
                batch.remove::<Construction>(StableId(actor.0));
            }
        }
        let effects = std::mem::take(&mut self.effects);
        let finish = ProjectFinalization {
            key: batch.key(),
            writes,
            source: self,
            effects,
        };
        finish.check_footprint(store, batch)?;
        Ok(finish)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        definitions: &game_workshop::Definitions,
        batch: &mut Batch,
    ) -> Result<ProjectFinalization, Error> {
        self.validate(runtime, store, definitions)?;
        self.stage_rows(runtime, store, batch)
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
        definitions: &game_workshop::Definitions,
    ) -> Result<Vec<Effect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, definitions, &mut batch)?;
        finish.validate_batch(runtime, store, definitions, &batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish(runtime))
    }
}
/// Consume after the host admits the combined batch in the same serialized window.
pub struct ProjectFinalization {
    key: game_ecs::BatchKey,
    writes: game_ecs::ReadFootprint,
    source: PreparedChange,
    effects: Vec<Effect>,
}
impl ProjectFinalization {
    pub fn validate_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        definitions: &game_workshop::Definitions,
        batch: &Batch,
    ) -> Result<(), Error> {
        self.source.validate(runtime, store, definitions)?;
        self.check_footprint(store, batch)
    }
    pub(crate) fn validate_advance_batch(
        &self,
        runtime: &Runtime,
        store: &Store,
        batch: &Batch,
    ) -> Result<(), Error> {
        if !self.source.recipe_reads.is_empty() {
            return Err(Error::Unavailable);
        }
        self.source.validate_rows(runtime, store)?;
        self.check_footprint(store, batch)
    }
    fn check_footprint(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        let source = &self.source;
        if self.key != batch.key()
            || !self.writes.has_one_edit_per_row(batch)
            || source.projects.iter().any(|(id, (before, after))| {
                before.is_none() && after.is_some() && batch.edit_count(StableId(id.0)) != 1
            })
            || source.enterprises.iter().any(|(id, (before, after))| {
                before.is_none() && after.is_some() && batch.edit_count(StableId(id.0)) != 1
            })
        {
            return Err(Error::Unavailable);
        }
        for id in batch
            .edited_ids::<Project>()
            .filter(|id| !source.projects.contains_key(&ProjectId(id.0)))
        {
            if let Some(row) = project(store, ProjectId(id.0))
                && (source
                    .area_slots
                    .get(&row.area)
                    .is_some_and(|ids| ids.contains(&row.id))
                    || matches!(&row.kind,ProjectKind::Farm{plot,..} if source.farm_slots.get(plot).is_some_and(|ids|ids.contains(&row.id))))
            {
                return Err(Error::Unavailable);
            }
        }
        for (_, row) in batch
            .insertions::<Project>()
            .filter(|(id, _)| !source.projects.contains_key(&ProjectId(id.0)))
        {
            if row.materials_consumed && source.area_slots.contains_key(&row.area)
                || matches!(&row.kind,ProjectKind::Farm{plot,..} if source.farm_slots.contains_key(plot) && !matches!(row.status, ProjectStatus::Failed | ProjectStatus::Cancelled))
            {
                return Err(Error::Unavailable);
            }
        }
        for id in batch
            .edited_ids::<Enterprise>()
            .filter(|id| !source.enterprises.contains_key(&EnterpriseId(id.0)))
        {
            if enterprise(store, EnterpriseId(id.0)).is_some_and(|row| {
                source
                    .enterprise_sites
                    .get(&row.site)
                    .is_some_and(|ids| ids.contains(&row.id))
            }) {
                return Err(Error::Unavailable);
            }
        }
        if batch.insertions::<Enterprise>().any(|(id, row)| {
            !source.enterprises.contains_key(&EnterpriseId(id.0))
                && source.enterprise_sites.contains_key(&row.site)
        }) {
            return Err(Error::Unavailable);
        }
        for id in batch
            .edited_ids::<Construction>()
            .filter(|id| !source.jobs.contains_key(&PersonId(id.0)))
        {
            if source.job_membership.is_some()
                || construction(store, PersonId(id.0))
                    .is_some_and(|row| source.project_job_membership.contains_key(&row.project))
            {
                return Err(Error::Unavailable);
            }
        }
        if batch.insertions::<Construction>().any(|(id, row)| {
            !source.jobs.contains_key(&PersonId(id.0))
                && source.project_job_membership.contains_key(&row.project)
        }) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn finish(self, runtime: &mut Runtime) -> Vec<Effect> {
        if let Some((_, cursor)) = self.source.cursor {
            runtime.last_event = cursor;
        }
        self.effects
    }
}

fn terminal(status: ProjectStatus) -> bool {
    matches!(
        status,
        ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
    )
}

impl Runtime {
    pub fn boundary(&self, store: &Store, now: SimTime, limit: SimTime) -> SimTime {
        crate::projects(store)
            .map(|(_, row)| row)
            .filter(|p| !terminal(p.status) && p.finish_by > now)
            .map(|p| p.finish_by)
            .min()
            .unwrap_or(limit)
            .min(limit)
    }
    pub fn allocation_bound(&self, store: &Store, next: SimTime) -> Result<u64, Error> {
        crate::constructions(store)
            .map(|(_, row)| row)
            .filter(|j| j.completes == next)
            .try_fold(0u64, |total, job| {
                let project = crate::project(store, job.project).ok_or(Error::Unavailable)?;
                total
                    .checked_add(if matches!(&project.kind, ProjectKind::Workshop { .. }) {
                        2
                    } else {
                        1
                    })
                    .ok_or(Error::Overflow)
            })
    }
    pub fn prepare_proposal(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        id: ProjectId,
        action: &ProjectAction,
        facts: ProposalFacts<'_>,
    ) -> Result<PreparedChange, Error> {
        let actor = facts.actor;
        let (kind, area, materials, tool, input, output, beneficiaries, workers, finish_by) =
            match action {
                ProjectAction::Propose {
                    recipe,
                    area,
                    materials,
                    tool,
                    input,
                    output,
                    beneficiaries,
                    workers,
                    finish_by,
                } => (
                    ProjectKind::Workshop {
                        recipe: recipe.clone(),
                    },
                    *area,
                    *materials,
                    *tool,
                    *input,
                    *output,
                    beneficiaries,
                    workers,
                    *finish_by,
                ),
                ProjectAction::ProposeFarm {
                    plot,
                    materials,
                    tool,
                    seed,
                    water,
                    food,
                    beneficiaries,
                    workers,
                    finish_by,
                } => (
                    ProjectKind::Farm {
                        plot: *plot,
                        water: *water,
                    },
                    facts.land_area.ok_or(Error::Unavailable)?,
                    *materials,
                    *tool,
                    *seed,
                    *food,
                    beneficiaries,
                    workers,
                    *finish_by,
                ),
                _ => return Err(Error::Unavailable),
            };
        if id.0 == 0
            || crate::project(store, id).is_some()
            || !actor.capable
            || actor.area != Some(area)
            || finish_by <= actor.now
            || beneficiaries.is_empty()
            || !beneficiaries.contains(&actor.actor)
            || !workers.contains(&actor.actor)
            || beneficiaries.len() > 8
            || workers.len() > 8
            || beneficiaries
                .iter()
                .chain(workers)
                .any(|p| !facts.people.contains(p))
        {
            return Err(Error::Unavailable);
        }
        let mut selected_recipe = None;
        let mut stocks = vec![(materials, Good::Timber), (tool, Good::Tool)];
        if let ProjectKind::Workshop { recipe: id } = &kind {
            let recipe = definitions.recipe(id)?;
            recipe.validate()?;
            selected_recipe = Some((id.clone(), recipe.clone()));
            stocks.extend([(input, recipe.input_good), (output, recipe.output_good)]);
        }
        if let ProjectKind::Farm { plot, water } = &kind {
            if !facts.land_accessible
                || crate::projects(store).map(|(_, row)| row).any(|p| {
                    matches!(&p.kind,ProjectKind::Farm {plot:id,..} if id==plot)
                        && !matches!(p.status, ProjectStatus::Failed | ProjectStatus::Cancelled)
                })
            {
                return Err(Error::Unavailable);
            }
            stocks = vec![
                (materials, Good::Timber),
                (tool, Good::Tool),
                (input, Good::Seed),
                (*water, Good::Water),
                (output, Good::Food),
            ];
        } else if tool == output || output == materials || input == output {
            return Err(Error::Unavailable);
        }
        if stocks.iter().any(|(id, good)| {
            facts
                .stocks
                .get(id)
                .is_none_or(|s| s.good != *good || s.area != Some(area))
        }) {
            return Err(Error::Unavailable);
        }
        let project = Project {
            kind,
            id,
            sponsor: actor.actor,
            area,
            materials,
            tool,
            input,
            output,
            beneficiaries: beneficiaries.clone(),
            workers: workers.clone(),
            support: BTreeSet::new(),
            status: ProjectStatus::SeekingSupport,
            blocker: Some(ProjectBlocker::Materials),
            progress_ms: 0,
            contributions: BTreeMap::new(),
            materials_consumed: false,
            finish_by,
            enterprise: None,
        };
        let mut change = PreparedChange::new(self);
        if let ProjectKind::Farm { plot, .. } = &project.kind {
            change.guard_farm_slots(store, *plot);
        }
        if let Some((id, recipe)) = selected_recipe {
            change.recipe_reads.insert(id, recipe);
        }
        change.project(store, id, project);
        change.effects.push(Effect::Proposed {
            project: id,
            sponsor: actor.actor,
        });
        Ok(change)
    }
    pub fn prepare_support(
        &self,
        store: &Store,
        project: ProjectId,
        contract: &Agreement,
        facts: ActorFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = crate::project(store, project).ok_or(Error::Unavailable)?;
        if !facts.capable
            || !facts.knows_project
            || facts.area != Some(plan.area)
            || !contract.parties.contains(&facts.actor)
            || !contract.parties.contains(&plan.sponsor)
            || !contract.accepted.contains(&facts.actor)
            || matches!(
                contract.status,
                AgreementStatus::Rejected | AgreementStatus::Withdrawn
            )
            || !contract.terms.iter().any(|term| match term {
                AgreementTerm::SiteAccess { site, .. } => {
                    matches!(&plan.kind,ProjectKind::Farm {plot,..} if *plot==*site)
                }
                AgreementTerm::ConstructionLabor {
                    project: target, ..
                } => *target == project,
                AgreementTerm::Delivery { destination, .. } => *destination == plan.materials,
                AgreementTerm::Access { inventory, .. } => {
                    *inventory == plan.tool || *inventory == plan.materials
                }
                _ => false,
            })
        {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedChange::new(self);
        let mut plan = plan.clone();
        plan.support.insert(contract.id);
        if let Some(id) = plan.enterprise {
            let mut enterprise = crate::enterprise(store, id)
                .ok_or(Error::Unavailable)?
                .clone();
            enterprise.operating_agreements.insert(contract.id);
            change.enterprise(store, id, enterprise);
        }
        change.project(store, project, plan);
        change.effects.push(Effect::Supported {
            project,
            actor: facts.actor,
            agreement: contract.id,
        });
        Ok(change)
    }
    pub fn prepare_operation(
        &self,
        store: &Store,
        enterprise: EnterpriseId,
        contract: &Agreement,
        facts: ActorFacts,
        known: bool,
    ) -> Result<PreparedChange, Error> {
        let asset = crate::enterprise(store, enterprise).ok_or(Error::Unavailable)?;
        if !facts.capable
            || !known
            || facts.area != Some(asset.area)
            || !contract.parties.contains(&facts.actor)
            || !contract.parties.contains(&asset.sponsor)
            || !contract.accepted.contains(&facts.actor)
            || matches!(
                contract.status,
                AgreementStatus::Rejected | AgreementStatus::Withdrawn
            )
            || !contract
                .terms
                .iter()
                .any(|t| matches!(t,AgreementTerm::Labor {site,..} if *site==asset.site))
        {
            return Err(Error::Unavailable);
        }
        let mut after = asset.clone();
        after.operating_agreements.insert(contract.id);
        let mut change = PreparedChange::new(self);
        change.enterprise(store, enterprise, after);
        Ok(change)
    }
    pub fn prepare_construction(
        &self,
        store: &Store,
        project: ProjectId,
        facts: ConstructionFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = crate::project(store, project).ok_or(Error::Unavailable)?;
        let actor = facts.actor;
        if !actor.capable
            || !actor.knows_project
            || actor.area != Some(plan.area)
            || !plan.workers.contains(&actor.actor)
            || !facts.land_accessible
            || !facts.tool_available
            || terminal(plan.status)
            || actor.now >= plan.finish_by
            || crate::constructions(store)
                .any(|(worker, job)| worker != actor.actor && job.project == project)
            || (!plan.materials_consumed
                && (facts.materials_available < self.rules.construction_timber
                    || crate::projects(store)
                        .map(|(_, row)| row)
                        .filter(|p| p.area == plan.area && p.materials_consumed)
                        .count()
                        >= self.rules.workshops_per_area))
        {
            return Err(Error::Unavailable);
        }
        let remaining = self
            .rules
            .construction_labor
            .0
            .checked_sub(plan.progress_ms)
            .ok_or(Error::Overflow)?;
        if remaining == 0 {
            return Err(Error::Unavailable);
        }
        let completes = SimTime(actor.now.0.checked_add(remaining).ok_or(Error::Overflow)?);
        if completes > plan.finish_by {
            return Err(Error::Unavailable);
        }
        let mut change = self.prepare_interrupt(store, actor.actor)?;
        change.guard_jobs(store);
        change.guard_area_slots(store, plan.area);
        if !plan.materials_consumed {
            change.effects.push(Effect::Consume {
                actor: actor.actor,
                source: plan.materials,
                quantity: self.rules.construction_timber,
            });
        }
        let mut after = plan.clone();
        after.materials_consumed = true;
        after.status = ProjectStatus::Active;
        after.blocker = None;
        change.project(store, project, after);
        change.job(
            store,
            actor.actor,
            Some(Construction { project, completes }),
        );
        change
            .effects
            .insert(0, Effect::Interrupt { actor: actor.actor });
        change.effects.push(Effect::ConstructionStarted {
            project,
            actor: actor.actor,
            completes,
        });
        Ok(change)
    }
    pub fn prepare_interrupt(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Result<PreparedChange, Error> {
        self.prepare_interrupt_many(store, &[actor])
    }
    pub fn prepare_interrupt_many(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        let mut seen = BTreeSet::new();
        for actor in actors {
            if !seen.insert(*actor) {
                continue;
            }
            if let Some(job) = crate::construction(store, *actor) {
                let members = constructions(store)
                    .filter(|(_, row)| row.project == job.project)
                    .map(|(actor, _)| actor)
                    .collect::<Vec<_>>();
                if members != [*actor] {
                    return Err(Error::Unavailable);
                }
                change.project_job_membership.insert(job.project, members);
                let mut project = crate::project(store, job.project)
                    .ok_or(Error::Unavailable)?
                    .clone();
                if !terminal(project.status) {
                    project.status = ProjectStatus::Ready;
                    project.blocker = None;
                }
                change.project(store, project.id, project);
                change.job(store, *actor, None);
            }
        }
        Ok(change)
    }
    pub fn prepare_cancel(
        &self,
        store: &Store,
        project: ProjectId,
        facts: ActorFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = crate::project(store, project).ok_or(Error::Unavailable)?;
        if !facts.capable
            || !facts.knows_project
            || plan.sponsor != facts.actor
            || terminal(plan.status)
        {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedChange::new(self);
        change.guard_jobs(store);
        for (actor, job) in crate::constructions(store) {
            if job.project == project {
                change.job(store, actor, None);
                change.effects.push(Effect::Interrupt { actor });
            }
        }
        let mut after = plan.clone();
        after.status = ProjectStatus::Cancelled;
        change.project(store, project, after);
        change.effects.push(Effect::Cancelled {
            project,
            actor: facts.actor,
        });
        Ok(change)
    }
    pub fn prepare_completion(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        actor: PersonId,
        facts: CompletionFacts,
    ) -> Result<PreparedChange, Error> {
        let job = crate::construction(store, actor).ok_or(Error::Unavailable)?;
        let plan = crate::project(store, job.project).ok_or(Error::Unavailable)?;
        if facts.now < job.completes || plan.progress_ms != self.rules.construction_labor.0 {
            return Err(Error::Unavailable);
        }
        if !facts.capable || !facts.asset_available {
            let mut change = self.prepare_interrupt(store, actor)?;
            change.effects.push(Effect::Interrupt { actor });
            return Ok(change);
        }
        let site = match &plan.kind {
            ProjectKind::Workshop { .. } => facts.site,
            ProjectKind::Farm { plot, .. } => *plot,
        };
        if facts.enterprise.0 == 0
            || crate::enterprise(store, facts.enterprise).is_some()
            || crate::enterprises(store)
                .map(|(_, row)| row)
                .any(|e| e.site == site)
        {
            return Err(Error::Unavailable);
        }
        let asset = match &plan.kind {
            ProjectKind::Workshop { recipe } => {
                let selected = definitions.recipe(recipe)?;
                selected.validate()?;
                AssetEffect::Workshop(WorkshopAsset {
                    recipe: recipe.clone(),
                    site,
                    area: plan.area,
                    input: plan.input,
                    tool: plan.tool,
                    output: plan.output,
                    access: plan.workers.clone(),
                })
            }
            ProjectKind::Farm { plot, water } => AssetEffect::Farm {
                plot: *plot,
                area: plan.area,
                seed: plan.input,
                tool: plan.tool,
                water: *water,
                food: plan.output,
            },
        };
        let mut change = PreparedChange::new(self);
        if let ProjectKind::Workshop { recipe } = &plan.kind {
            change
                .recipe_reads
                .insert(recipe.clone(), definitions.recipe(recipe)?.clone());
        }
        change.guard_enterprise_sites(store, site);
        change.job(store, actor, None);
        let mut after = plan.clone();
        after.status = ProjectStatus::Completed;
        after.enterprise = Some(facts.enterprise);
        after.blocker = None;
        change.project(store, plan.id, after);
        change.enterprise(
            store,
            facts.enterprise,
            Enterprise {
                kind: plan.kind.clone(),
                inputs: BTreeMap::new(),
                produced: BTreeMap::new(),
                id: facts.enterprise,
                project: plan.id,
                sponsor: plan.sponsor,
                area: plan.area,
                site,
                beneficiaries: plan.beneficiaries.clone(),
                workers: plan.workers.clone(),
                status: EnterpriseStatus::Ready,
                blocker: None,
                labor: BTreeMap::new(),
                operating_agreements: plan.support.clone(),
            },
        );
        change.effects.push(Effect::Completed {
            project: plan.id,
            enterprise: facts.enterprise,
            site,
            actor,
            asset,
        });
        Ok(change)
    }
    pub fn prepare_input(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        site: SiteId,
        good: Good,
        quantity: Quantity,
    ) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        change.guard_enterprise_sites(store, site);
        if let Some(asset) = crate::enterprises(store)
            .map(|(_, row)| row)
            .find(|e| e.site == site)
        {
            let mut after = asset.clone();
            let total = after.inputs.entry(good).or_default();
            *total = total.checked_add(quantity.0).ok_or(Error::Overflow)?;
            if let ProjectKind::Workshop { recipe: id } = &asset.kind {
                let recipe = definitions.recipe(id)?;
                if good != recipe.input_good {
                    return Err(Error::Unavailable);
                }
                change.recipe_reads.insert(id.clone(), recipe.clone());
                change.effects.push(Effect::InputUsed {
                    enterprise: asset.id,
                    site,
                    quantity,
                });
            }
            change.enterprise(store, asset.id, after);
        }
        Ok(change)
    }
    pub fn prepare_outputs(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        outputs: &[(SiteId, Good, Quantity)],
        cursor: Option<u64>,
    ) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        for (site, good, quantity) in outputs {
            change.guard_enterprise_sites(store, *site);
            if let Some(asset) = crate::enterprises(store)
                .map(|(_, row)| row)
                .find(|e| e.site == *site)
            {
                let mut after = change
                    .enterprises
                    .get(&asset.id)
                    .and_then(|(_, a)| a.clone())
                    .unwrap_or_else(|| asset.clone());
                let total = after.produced.entry(*good).or_default();
                *total = total.checked_add(quantity.0).ok_or(Error::Overflow)?;
                if let ProjectKind::Workshop { recipe: id } = &asset.kind {
                    let recipe = definitions.recipe(id)?;
                    if *good != recipe.output_good {
                        return Err(Error::Unavailable);
                    }
                    change.recipe_reads.insert(id.clone(), recipe.clone());
                }
                change.enterprise(store, asset.id, after);
            }
        }
        if let Some(cursor) = cursor {
            if cursor < self.last_event {
                return Err(Error::Unavailable);
            }
            change.cursor = Some((self.last_event, cursor));
        }
        Ok(change)
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit Store and recipe authority accompany the existing ordered closing facts"
    )]
    pub fn prepare_reconcile(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        now: SimTime,
        projects: &[ProjectCondition],
        enterprises: &[EnterpriseCondition],
        outputs: &[(SiteId, Good, Quantity)],
        cursor: u64,
    ) -> Result<PreparedChange, Error> {
        let mut change = self.prepare_outputs(store, definitions, outputs, Some(cursor))?;
        change.guard_jobs(store);
        for plan in crate::projects(store).map(|(_, row)| row) {
            if terminal(plan.status) {
                continue;
            }
            let mut after = plan.clone();
            if plan.finish_by <= now {
                for (actor, job) in crate::constructions(store) {
                    if job.project == plan.id {
                        change.job(store, actor, None);
                        change.effects.push(Effect::Interrupt { actor });
                    }
                }
                after.status = ProjectStatus::Failed;
                after.blocker = Some(ProjectBlocker::Deadline);
                change.effects.push(Effect::Failed { project: plan.id });
            } else {
                let facts = projects
                    .iter()
                    .find(|f| f.project == plan.id)
                    .ok_or(Error::Unavailable)?;
                let mut interrupted = false;
                for (actor, job) in crate::constructions(store) {
                    if job.project == plan.id && !facts.usable_worker_tools.contains(&actor) {
                        change.job(store, actor, None);
                        change.effects.push(Effect::Interrupt { actor });
                        interrupted = true;
                    }
                }
                if plan.status != ProjectStatus::Active || interrupted {
                    after.blocker = if !facts.sponsor_tool {
                        Some(ProjectBlocker::Tool)
                    } else if !plan.materials_consumed
                        && facts.materials_available < self.rules.construction_timber
                    {
                        Some(ProjectBlocker::Materials)
                    } else {
                        None
                    };
                    after.status = if after.blocker.is_some() {
                        ProjectStatus::Blocked
                    } else {
                        ProjectStatus::Ready
                    };
                }
            }
            if after != *plan {
                change.project(store, plan.id, after);
            }
        }
        for asset in crate::enterprises(store).map(|(_, row)| row) {
            let facts = enterprises
                .iter()
                .find(|f| f.enterprise == asset.id)
                .ok_or(Error::Unavailable)?;
            let blocker = match &asset.kind {
                ProjectKind::Farm { .. } => {
                    if !facts.workers.iter().any(|p| p.capable && p.land_accessible) {
                        Some(ProjectBlocker::Workers)
                    } else if facts.crop_failed {
                        Some(ProjectBlocker::Materials)
                    } else {
                        None
                    }
                }
                ProjectKind::Workshop { recipe: id } => {
                    let recipe = definitions.recipe(id)?;
                    recipe.validate()?;
                    change.recipe_reads.insert(id.clone(), recipe.clone());
                    if facts.output_room < recipe.output_quantity {
                        Some(ProjectBlocker::OutputCapacity)
                    } else if !facts.workers.iter().any(|p| {
                        p.capable
                            && p.tool_available
                            && p.materials_available >= recipe.input_quantity
                    }) {
                        Some(ProjectBlocker::Materials)
                    } else {
                        None
                    }
                }
            };
            let mut after = change
                .enterprises
                .get(&asset.id)
                .and_then(|(_, a)| a.clone())
                .unwrap_or_else(|| asset.clone());
            after.status = if facts.active {
                EnterpriseStatus::Active
            } else if blocker.is_some() {
                EnterpriseStatus::Blocked
            } else {
                EnterpriseStatus::Ready
            };
            after.blocker = if facts.active && matches!(&asset.kind, ProjectKind::Workshop { .. }) {
                None
            } else {
                blocker
            };
            if after != *asset {
                change.enterprise(store, asset.id, after);
            }
        }
        Ok(change)
    }
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if self.construction_timber.0 == 0
            || self.construction_labor.0 == 0
            || !(1..=16).contains(&self.workshops_per_area)
        {
            return Err(Error::InvalidConfiguration(
                "invalid project construction rules".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CompletionFacts {
    pub now: SimTime,
    pub capable: bool,
    pub asset_available: bool,
    pub site: SiteId,
    pub enterprise: EnterpriseId,
}
pub struct ProjectCondition {
    pub project: ProjectId,
    pub sponsor_tool: bool,
    pub materials_available: Quantity,
    pub usable_worker_tools: BTreeSet<PersonId>,
}
#[derive(Clone, Copy, Debug)]
pub struct WorkerCondition {
    pub capable: bool,
    pub land_accessible: bool,
    pub tool_available: bool,
    pub materials_available: Quantity,
}
pub struct EnterpriseCondition {
    pub enterprise: EnterpriseId,
    pub active: bool,
    pub workers: Vec<WorkerCondition>,
    pub crop_failed: bool,
    pub output_room: Quantity,
}

pub struct AccountObligation {
    pub term: AgreementTerm,
    pub performed: u64,
    pub accrued: u64,
    pub defaulted: bool,
    pub delivery_good: Option<Good>,
}
pub fn account_view(
    enterprise: &Enterprise,
    obligations: &[AccountObligation],
) -> Result<EnterpriseAccountView, Error> {
    let mut accounts = EnterpriseAccountView {
        inputs: enterprise.inputs.clone(),
        produced: enterprise.produced.clone(),
        labor: enterprise.labor.clone(),
        operating_agreements: enterprise.operating_agreements.clone(),
        promised_labor_ms: 0,
        performed_labor_ms: 0,
        promised_payments: BTreeMap::new(),
        performed_payments: BTreeMap::new(),
        defaults: 0,
    };
    for obligation in obligations {
        let payment = match obligation.term {
            AgreementTerm::Labor { site, millis, .. } if site == enterprise.site => {
                accounts.promised_labor_ms = accounts
                    .promised_labor_ms
                    .checked_add(u128::from(millis))
                    .ok_or(Error::Overflow)?;
                accounts.performed_labor_ms = accounts
                    .performed_labor_ms
                    .checked_add(u128::from(obligation.performed))
                    .ok_or(Error::Overflow)?;
                None
            }
            AgreementTerm::OutputShare { debtor, good, .. } if debtor == enterprise.sponsor => {
                Some((good, obligation.accrued))
            }
            AgreementTerm::Delivery {
                debtor, quantity, ..
            } if debtor == enterprise.sponsor => Some((
                obligation.delivery_good.ok_or(Error::Unavailable)?,
                quantity.0,
            )),
            _ => None,
        };
        if let Some((good, promised)) = payment {
            let total = accounts.promised_payments.entry(good).or_default();
            *total = total
                .checked_add(u128::from(promised))
                .ok_or(Error::Overflow)?;
            let total = accounts.performed_payments.entry(good).or_default();
            *total = total
                .checked_add(u128::from(obligation.performed))
                .ok_or(Error::Overflow)?;
        }
        accounts.defaults = accounts
            .defaults
            .checked_add(u64::from(obligation.defaulted))
            .ok_or(Error::Overflow)?;
    }
    Ok(accounts)
}

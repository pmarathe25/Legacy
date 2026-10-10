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
    pub site: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub input_quantity: Quantity,
    pub tool: InventoryId,
    pub output: InventoryId,
    pub output_quantity: Quantity,
    pub duration: SimDuration,
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

/// Owned-record comparison occurs before any patch is applied. Physical effects
/// are returned only after that commit, for the enclosing world transaction.
#[derive(Clone, Debug)]
pub struct PreparedChange {
    rules: Rules,
    projects: BTreeMap<ProjectId, (Option<Project>, Option<Project>)>,
    enterprises: BTreeMap<EnterpriseId, (Option<Enterprise>, Option<Enterprise>)>,
    jobs: BTreeMap<PersonId, (Option<Construction>, Option<Construction>)>,
    project_catalog: Option<BTreeMap<ProjectId, Project>>,
    job_catalog: Option<BTreeMap<PersonId, Construction>>,
    cursor: Option<(u64, u64)>,
    effects: Vec<Effect>,
}
impl PreparedChange {
    fn new(world: &World) -> Self {
        Self {
            rules: world.rules.clone(),
            projects: BTreeMap::new(),
            enterprises: BTreeMap::new(),
            jobs: BTreeMap::new(),
            project_catalog: None,
            job_catalog: None,
            cursor: None,
            effects: Vec::new(),
        }
    }
    fn project(&mut self, world: &World, id: ProjectId, after: Project) {
        self.projects
            .insert(id, (world.projects.get(&id).cloned(), Some(after)));
    }
    fn enterprise(&mut self, world: &World, id: EnterpriseId, after: Enterprise) {
        self.enterprises
            .insert(id, (world.enterprises.get(&id).cloned(), Some(after)));
    }
    fn job(&mut self, world: &World, actor: PersonId, after: Option<Construction>) {
        self.jobs
            .insert(actor, (world.jobs.get(&actor).cloned(), after));
    }
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
    pub fn commit(self, world: &mut World) -> Result<Vec<Effect>, Error> {
        if world.rules != self.rules
            || self
                .project_catalog
                .as_ref()
                .is_some_and(|before| before != &world.projects)
            || self
                .job_catalog
                .as_ref()
                .is_some_and(|before| before != &world.jobs)
            || self
                .projects
                .iter()
                .any(|(id, (before, _))| world.projects.get(id) != before.as_ref())
            || self
                .enterprises
                .iter()
                .any(|(id, (before, _))| world.enterprises.get(id) != before.as_ref())
            || self
                .jobs
                .iter()
                .any(|(id, (before, _))| world.jobs.get(id) != before.as_ref())
            || self
                .cursor
                .is_some_and(|(before, _)| world.last_event != before)
        {
            return Err(Error::Unavailable);
        }
        for (id, (_, after)) in self.projects {
            if let Some(record) = after {
                world.projects.insert(id, record);
            } else {
                world.projects.remove(&id);
            }
        }
        for (id, (_, after)) in self.enterprises {
            if let Some(record) = after {
                world.enterprises.insert(id, record);
            } else {
                world.enterprises.remove(&id);
            }
        }
        for (id, (_, after)) in self.jobs {
            if let Some(record) = after {
                world.jobs.insert(id, record);
            } else {
                world.jobs.remove(&id);
            }
        }
        if let Some((_, after)) = self.cursor {
            world.last_event = after;
        }
        Ok(self.effects)
    }
}
fn terminal(status: ProjectStatus) -> bool {
    matches!(
        status,
        ProjectStatus::Completed | ProjectStatus::Failed | ProjectStatus::Cancelled
    )
}

impl World {
    pub fn boundary(&self, now: SimTime, limit: SimTime) -> SimTime {
        self.projects
            .values()
            .filter(|p| !terminal(p.status) && p.finish_by > now)
            .map(|p| p.finish_by)
            .min()
            .unwrap_or(limit)
            .min(limit)
    }
    pub fn allocation_bound(&self, next: SimTime) -> Result<u64, Error> {
        self.jobs
            .values()
            .filter(|j| j.completes == next)
            .try_fold(0u64, |total, job| {
                let project = self.projects.get(&job.project).ok_or(Error::Unavailable)?;
                total
                    .checked_add(if matches!(project.kind, ProjectKind::Workshop) {
                        2
                    } else {
                        1
                    })
                    .ok_or(Error::Overflow)
            })
    }
    pub fn validate_owned(&self, now: SimTime) -> Result<(), Error> {
        self.rules.validate()?;
        let fail = || Error::InvalidSave("invalid owned project lifecycle or account".into());
        let mut exclusive = BTreeMap::<PersonId, u128>::new();
        for (id, p) in &self.projects {
            if *id != p.id
                || p.beneficiaries.is_empty()
                || p.beneficiaries.len() > 8
                || p.workers.is_empty()
                || p.workers.len() > 8
                || !p.beneficiaries.contains(&p.sponsor)
                || !p.workers.contains(&p.sponsor)
                || p.progress_ms > self.rules.construction_labor.0
                || p.contributions
                    .values()
                    .map(|q| u128::from(*q))
                    .sum::<u128>()
                    != u128::from(p.progress_ms)
                || !p.materials_consumed && p.progress_ms != 0
                || p.tool == p.output
                || (p.status == ProjectStatus::Completed) != p.enterprise.is_some()
                || p.status == ProjectStatus::Completed
                    && p.progress_ms != self.rules.construction_labor.0
                || p.status == ProjectStatus::Failed
                    && (p.finish_by > now || p.blocker != Some(ProjectBlocker::Deadline))
                || p.enterprise
                    .is_some_and(|id| self.enterprises.get(&id).is_none_or(|e| e.project != p.id))
            {
                return Err(fail());
            }
            for (actor, elapsed) in &p.contributions {
                *exclusive.entry(*actor).or_default() += u128::from(*elapsed);
            }
        }
        let mut busy = BTreeSet::new();
        for (actor, job) in &self.jobs {
            let p = self.projects.get(&job.project).ok_or_else(fail)?;
            if !busy.insert(job.project)
                || !p.materials_consumed
                || p.status != ProjectStatus::Active
                || !p.workers.contains(actor)
                || job.completes <= now
                || job.completes > p.finish_by
                || job.completes.0 - now.0 != self.rules.construction_labor.0 - p.progress_ms
            {
                return Err(fail());
            }
        }
        if self
            .projects
            .values()
            .any(|p| p.status == ProjectStatus::Active && !busy.contains(&p.id))
        {
            return Err(fail());
        }
        let mut sites = BTreeSet::new();
        for (id, e) in &self.enterprises {
            let p = self.projects.get(&e.project).ok_or_else(fail)?;
            if *id != e.id
                || !sites.insert(e.site)
                || p.enterprise != Some(*id)
                || e.kind != p.kind
                || e.area != p.area
                || e.sponsor != p.sponsor
                || e.beneficiaries != p.beneficiaries
                || e.workers != p.workers
            {
                return Err(fail());
            }
            match e.kind {
                ProjectKind::Farm { plot, .. } => {
                    if e.site != plot
                        || e.tools_produced != 0
                        || e.timber_consumed != 0
                        || e.inputs
                            .keys()
                            .any(|g| !matches!(g, Good::Seed | Good::Water))
                    {
                        return Err(fail());
                    }
                }
                ProjectKind::Workshop => {
                    if e.timber_consumed % self.rules.recipe_timber.0 != 0
                        || e.tools_produced % self.rules.recipe_tools.0 != 0
                        || u128::from(e.timber_consumed)
                            < u128::from(e.tools_produced / self.rules.recipe_tools.0)
                                * u128::from(self.rules.recipe_timber.0)
                    {
                        return Err(fail());
                    }
                }
            }
            for (actor, elapsed) in &e.labor {
                *exclusive.entry(*actor).or_default() += u128::from(*elapsed);
            }
        }
        if exclusive
            .values()
            .any(|elapsed| *elapsed > u128::from(now.0))
        {
            return Err(fail());
        }
        Ok(())
    }
    pub fn prepare_proposal(
        &self,
        id: ProjectId,
        action: &ProjectAction,
        facts: ProposalFacts<'_>,
    ) -> Result<PreparedChange, Error> {
        let actor = facts.actor;
        let (kind, area, materials, tool, input, output, beneficiaries, workers, finish_by) =
            match action {
                ProjectAction::Propose {
                    area,
                    materials,
                    tool,
                    input,
                    output,
                    beneficiaries,
                    workers,
                    finish_by,
                } => (
                    ProjectKind::Workshop,
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
            || self.projects.contains_key(&id)
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
        let mut stocks = vec![
            (materials, Good::Timber),
            (tool, Good::Tool),
            (input, Good::Timber),
            (output, Good::Tool),
        ];
        if let ProjectKind::Farm { plot, water } = kind {
            if !facts.land_accessible
                || self.projects.values().any(|p| {
                    matches!(p.kind,ProjectKind::Farm {plot:id,..} if id==plot)
                        && !matches!(p.status, ProjectStatus::Failed | ProjectStatus::Cancelled)
                })
            {
                return Err(Error::Unavailable);
            }
            stocks = vec![
                (materials, Good::Timber),
                (tool, Good::Tool),
                (input, Good::Seed),
                (water, Good::Water),
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
        change.project_catalog = Some(self.projects.clone());
        change.project(self, id, project);
        change.effects.push(Effect::Proposed {
            project: id,
            sponsor: actor.actor,
        });
        Ok(change)
    }
    pub fn prepare_support(
        &self,
        project: ProjectId,
        contract: &Agreement,
        facts: ActorFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = self.projects.get(&project).ok_or(Error::Unavailable)?;
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
                    matches!(plan.kind,ProjectKind::Farm {plot,..} if plot==*site)
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
            let mut enterprise = self.enterprises.get(&id).ok_or(Error::Unavailable)?.clone();
            enterprise.operating_agreements.insert(contract.id);
            change.enterprise(self, id, enterprise);
        }
        change.project(self, project, plan);
        change.effects.push(Effect::Supported {
            project,
            actor: facts.actor,
            agreement: contract.id,
        });
        Ok(change)
    }
    pub fn prepare_operation(
        &self,
        enterprise: EnterpriseId,
        contract: &Agreement,
        facts: ActorFacts,
        known: bool,
    ) -> Result<PreparedChange, Error> {
        let asset = self
            .enterprises
            .get(&enterprise)
            .ok_or(Error::Unavailable)?;
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
        change.enterprise(self, enterprise, after);
        Ok(change)
    }
    pub fn prepare_construction(
        &self,
        project: ProjectId,
        facts: ConstructionFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = self.projects.get(&project).ok_or(Error::Unavailable)?;
        let actor = facts.actor;
        if !actor.capable
            || !actor.knows_project
            || actor.area != Some(plan.area)
            || !plan.workers.contains(&actor.actor)
            || !facts.land_accessible
            || !facts.tool_available
            || terminal(plan.status)
            || actor.now >= plan.finish_by
            || self
                .jobs
                .iter()
                .any(|(worker, job)| *worker != actor.actor && job.project == project)
            || (!plan.materials_consumed
                && (facts.materials_available < self.rules.construction_timber
                    || self
                        .projects
                        .values()
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
        let mut change = self.prepare_interrupt(actor.actor)?;
        change.job_catalog = Some(self.jobs.clone());
        change.project_catalog = Some(self.projects.clone());
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
        change.project(self, project, after);
        change.job(self, actor.actor, Some(Construction { project, completes }));
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
    pub fn prepare_interrupt(&self, actor: PersonId) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        if let Some(job) = self.jobs.get(&actor) {
            let mut project = self
                .projects
                .get(&job.project)
                .ok_or(Error::Unavailable)?
                .clone();
            if !terminal(project.status) {
                project.status = ProjectStatus::Ready;
                project.blocker = None;
            }
            change.project(self, project.id, project);
            change.job(self, actor, None);
        }
        Ok(change)
    }
    pub fn prepare_cancel(
        &self,
        project: ProjectId,
        facts: ActorFacts,
    ) -> Result<PreparedChange, Error> {
        let plan = self.projects.get(&project).ok_or(Error::Unavailable)?;
        if !facts.capable
            || !facts.knows_project
            || plan.sponsor != facts.actor
            || terminal(plan.status)
        {
            return Err(Error::Unavailable);
        }
        let mut change = PreparedChange::new(self);
        change.job_catalog = Some(self.jobs.clone());
        for (actor, job) in &self.jobs {
            if job.project == project {
                change.job(self, *actor, None);
                change.effects.push(Effect::Interrupt { actor: *actor });
            }
        }
        let mut after = plan.clone();
        after.status = ProjectStatus::Cancelled;
        change.project(self, project, after);
        change.effects.push(Effect::Cancelled {
            project,
            actor: facts.actor,
        });
        Ok(change)
    }
    pub fn prepare_completion(
        &self,
        actor: PersonId,
        facts: CompletionFacts,
    ) -> Result<PreparedChange, Error> {
        let job = self.jobs.get(&actor).ok_or(Error::Unavailable)?;
        let plan = self.projects.get(&job.project).ok_or(Error::Unavailable)?;
        if facts.now < job.completes || plan.progress_ms != self.rules.construction_labor.0 {
            return Err(Error::Unavailable);
        }
        if !facts.capable || !facts.asset_available {
            let mut change = self.prepare_interrupt(actor)?;
            change.effects.push(Effect::Interrupt { actor });
            return Ok(change);
        }
        let site = match plan.kind {
            ProjectKind::Workshop => facts.site,
            ProjectKind::Farm { plot, .. } => plot,
        };
        if facts.enterprise.0 == 0
            || self.enterprises.contains_key(&facts.enterprise)
            || self.enterprises.values().any(|e| e.site == site)
        {
            return Err(Error::Unavailable);
        }
        let asset = match plan.kind {
            ProjectKind::Workshop => AssetEffect::Workshop(WorkshopAsset {
                site,
                area: plan.area,
                input: plan.input,
                input_quantity: self.rules.recipe_timber,
                tool: plan.tool,
                output: plan.output,
                output_quantity: self.rules.recipe_tools,
                duration: self.rules.recipe_time,
                access: plan.workers.clone(),
            }),
            ProjectKind::Farm { plot, water } => AssetEffect::Farm {
                plot,
                area: plan.area,
                seed: plan.input,
                tool: plan.tool,
                water,
                food: plan.output,
            },
        };
        let mut change = PreparedChange::new(self);
        change.job(self, actor, None);
        let mut after = plan.clone();
        after.status = ProjectStatus::Completed;
        after.enterprise = Some(facts.enterprise);
        after.blocker = None;
        change.project(self, plan.id, after);
        change.enterprise(
            self,
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
                timber_consumed: 0,
                tools_produced: 0,
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
        site: SiteId,
        good: Good,
        quantity: Quantity,
    ) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        if let Some(asset) = self.enterprises.values().find(|e| e.site == site) {
            let mut after = asset.clone();
            let total = after.inputs.entry(good).or_default();
            *total = total.checked_add(quantity.0).ok_or(Error::Overflow)?;
            if matches!(asset.kind, ProjectKind::Workshop) && good == Good::Timber {
                after.timber_consumed = after
                    .timber_consumed
                    .checked_add(quantity.0)
                    .ok_or(Error::Overflow)?;
                change.effects.push(Effect::InputUsed {
                    enterprise: asset.id,
                    site,
                    quantity,
                });
            }
            change.enterprise(self, asset.id, after);
        }
        Ok(change)
    }
    pub fn prepare_outputs(
        &self,
        outputs: &[(SiteId, Good, Quantity)],
        cursor: Option<u64>,
    ) -> Result<PreparedChange, Error> {
        let mut change = PreparedChange::new(self);
        for (site, good, quantity) in outputs {
            if let Some(asset) = self.enterprises.values().find(|e| e.site == *site) {
                let mut after = change
                    .enterprises
                    .get(&asset.id)
                    .and_then(|(_, a)| a.clone())
                    .unwrap_or_else(|| asset.clone());
                let total = after.produced.entry(*good).or_default();
                *total = total.checked_add(quantity.0).ok_or(Error::Overflow)?;
                if *good == Good::Tool {
                    after.tools_produced = after
                        .tools_produced
                        .checked_add(quantity.0)
                        .ok_or(Error::Overflow)?;
                }
                change.enterprise(self, asset.id, after);
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
    pub fn prepare_reconcile(
        &self,
        now: SimTime,
        projects: &[ProjectCondition],
        enterprises: &[EnterpriseCondition],
        outputs: &[(SiteId, Good, Quantity)],
        cursor: u64,
    ) -> Result<PreparedChange, Error> {
        let mut change = self.prepare_outputs(outputs, Some(cursor))?;
        change.job_catalog = Some(self.jobs.clone());
        for plan in self.projects.values() {
            if terminal(plan.status) {
                continue;
            }
            let mut after = plan.clone();
            if plan.finish_by <= now {
                for (actor, job) in &self.jobs {
                    if job.project == plan.id {
                        change.job(self, *actor, None);
                        change.effects.push(Effect::Interrupt { actor: *actor });
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
                for (actor, job) in &self.jobs {
                    if job.project == plan.id && !facts.usable_worker_tools.contains(actor) {
                        change.job(self, *actor, None);
                        change.effects.push(Effect::Interrupt { actor: *actor });
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
                change.project(self, plan.id, after);
            }
        }
        for asset in self.enterprises.values() {
            let facts = enterprises
                .iter()
                .find(|f| f.enterprise == asset.id)
                .ok_or(Error::Unavailable)?;
            let blocker = match asset.kind {
                ProjectKind::Farm { .. } => {
                    if !facts.workers.iter().any(|p| p.capable && p.land_accessible) {
                        Some(ProjectBlocker::Workers)
                    } else if facts.crop_failed {
                        Some(ProjectBlocker::Materials)
                    } else {
                        None
                    }
                }
                ProjectKind::Workshop => {
                    if facts.output_room < facts.required_output {
                        Some(ProjectBlocker::OutputCapacity)
                    } else if !facts.workers.iter().any(|p| {
                        p.capable
                            && p.tool_available
                            && p.materials_available >= facts.required_input
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
            after.blocker = if facts.active && matches!(asset.kind, ProjectKind::Workshop) {
                None
            } else {
                blocker
            };
            if after != *asset {
                change.enterprise(self, asset.id, after);
            }
        }
        Ok(change)
    }
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        if [
            self.construction_timber.0,
            self.construction_labor.0,
            self.recipe_timber.0,
            self.recipe_tools.0,
            self.recipe_time.0,
        ]
        .contains(&0)
            || self
                .recipe_timber
                .0
                .checked_mul(3)
                .and_then(|q| q.checked_add(self.construction_timber.0))
                .is_none()
            || self.recipe_tools.0.checked_mul(3).is_none()
            || !(1..=16).contains(&self.workshops_per_area)
        {
            return Err(Error::InvalidConfiguration(
                "invalid project technology rules".into(),
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
    pub required_input: Quantity,
    pub required_output: Quantity,
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
        timber_consumed: enterprise.timber_consumed,
        tools_produced: enterprise.tools_produced,
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

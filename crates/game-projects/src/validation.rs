use crate::*;
pub fn validate_runtime(
    runtime: &Runtime,
    store: &Store,
    definitions: &game_workshop::Definitions,
    now: SimTime,
) -> Result<(), Error> {
    runtime.validate_owned(store, definitions, now)
}
impl Runtime {
    pub fn validate_owned(
        &self,
        store: &Store,
        definitions: &game_workshop::Definitions,
        now: SimTime,
    ) -> Result<(), Error> {
        self.rules.validate()?;
        let bad = || Error::InvalidSave("invalid owned project lifecycle or account".into());
        let mut exclusive = BTreeMap::<PersonId, u128>::new();
        for (id, row) in projects(store) {
            row.validate().map_err(|_| bad())?;
            if id != row.id
                || row.progress_ms > self.rules.construction_labor.0
                || row.status == ProjectStatus::Completed
                    && row.progress_ms != self.rules.construction_labor.0
                || row.status == ProjectStatus::Failed
                    && (row.finish_by > now || row.blocker != Some(ProjectBlocker::Deadline))
                || row
                    .enterprise
                    .is_some_and(|id| enterprise(store, id).is_none_or(|e| e.project != row.id))
            {
                return Err(bad());
            }
            if let ProjectKind::Workshop { recipe } = &row.kind {
                definitions.recipe(recipe)?.validate()?;
            }
            for (actor, elapsed) in &row.contributions {
                let total = exclusive.entry(*actor).or_default();
                *total = total.checked_add(u128::from(*elapsed)).ok_or_else(bad)?;
            }
        }
        let mut busy = BTreeSet::new();
        for (actor, job) in constructions(store) {
            job.validate().map_err(|_| bad())?;
            let row = project(store, job.project).ok_or_else(bad)?;
            if !busy.insert(job.project)
                || !row.materials_consumed
                || row.status != ProjectStatus::Active
                || !row.workers.contains(&actor)
                || job.completes <= now
                || job.completes > row.finish_by
                || job.completes.0 - now.0 != self.rules.construction_labor.0 - row.progress_ms
            {
                return Err(bad());
            }
        }
        if projects(store)
            .any(|(_, row)| row.status == ProjectStatus::Active && !busy.contains(&row.id))
        {
            return Err(bad());
        }
        let mut sites = BTreeSet::new();
        for (id, row) in enterprises(store) {
            row.validate().map_err(|_| bad())?;
            let plan = project(store, row.project).ok_or_else(bad)?;
            if id != row.id
                || !sites.insert(row.site)
                || plan.enterprise != Some(id)
                || row.kind != plan.kind
                || row.area != plan.area
                || row.sponsor != plan.sponsor
                || row.beneficiaries != plan.beneficiaries
                || row.workers != plan.workers
            {
                return Err(bad());
            }
            match &row.kind {
                ProjectKind::Farm { plot, .. } => {
                    if row.site != *plot
                        || row
                            .inputs
                            .keys()
                            .any(|good| !matches!(good, Good::Seed | Good::Water))
                    {
                        return Err(bad());
                    }
                }
                ProjectKind::Workshop { recipe: id } => {
                    let recipe = definitions.recipe(id)?;
                    recipe.validate()?;
                    let consumed = row.inputs.get(&recipe.input_good).copied().unwrap_or(0);
                    let produced = row.produced.get(&recipe.output_good).copied().unwrap_or(0);
                    if row.inputs.keys().any(|good| *good != recipe.input_good)
                        || row.produced.keys().any(|good| *good != recipe.output_good)
                        || consumed % recipe.input_quantity.0 != 0
                        || produced % recipe.output_quantity.0 != 0
                        || u128::from(consumed)
                            < u128::from(produced / recipe.output_quantity.0)
                                * u128::from(recipe.input_quantity.0)
                    {
                        return Err(bad());
                    }
                }
            }
            for (actor, elapsed) in &row.labor {
                let total = exclusive.entry(*actor).or_default();
                *total = total.checked_add(u128::from(*elapsed)).ok_or_else(bad)?;
            }
        }
        if exclusive
            .values()
            .any(|elapsed| *elapsed > u128::from(now.0))
        {
            return Err(bad());
        }
        for (_, row) in knowledges(store) {
            row.validate().map_err(|_| bad())?;
            if row
                .projects
                .values()
                .any(|view| view.observed_at > now || project(store, view.id).is_none())
                || row
                    .enterprises
                    .values()
                    .any(|view| view.observed_at > now || enterprise(store, view.id).is_none())
            {
                return Err(bad());
            }
        }
        Ok(())
    }
}

/// Reject native rows when this owner has no runtime in the composition.
pub fn validate_absent(store: &Store) -> Result<(), Error> {
    if projects(store).next().is_some()
        || enterprises(store).next().is_some()
        || constructions(store).next().is_some()
        || knowledges(store).next().is_some()
        || request_rows(store).next().is_some()
    {
        return Err(Error::InvalidSave(
            "project components have no runtime".into(),
        ));
    }
    Ok(())
}

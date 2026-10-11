use super::*;
use game_ecs::Store;

#[derive(Clone, Copy, Debug)]
pub struct ResourceInput<'a> {
    pub resource: &'a Resource,
    pub weather: &'a WeatherFacts,
}
impl ResourceInput<'_> {
    pub fn evaluate(self) -> Result<Resource, Error> {
        self.resource.validate()?;
        self.weather.validate()?;
        let mut source = self.resource.clone();
        let weather = self.weather;
        source.soil_water = source
            .soil_water
            .saturating_add(weather.rain)
            .min(source.water_capacity)
            .saturating_sub(weather.evaporation);
        let warmth = i64::from(weather.temperature - source.minimum_temperature)
            .max(0)
            .min(i64::from(
                source.optimum_temperature - source.minimum_temperature,
            )) as u64
            * 1_000
            / u64::try_from(source.optimum_temperature - source.minimum_temperature)
                .map_err(|_| Error::Overflow)?;
        let moisture = (u128::from(source.soil_water.min(source.optimal_water)) * 1_000
            / u128::from(source.optimal_water)) as u64;
        let growth = u128::from(source.daily_growth.0) * u128::from(warmth) * u128::from(moisture)
            + u128::from(source.growth_remainder);
        let available = source.capacity.0 - source.remaining.0;
        let added = u64::try_from((growth / 1_000_000).min(u128::from(available)))
            .map_err(|_| Error::Overflow)?;
        source.growth_remainder = if added == available {
            0
        } else {
            (growth % 1_000_000) as u64
        };
        let decay =
            (u128::from(source.remaining.0) * u128::from(source.decay_permille) / 1_000) as u64;
        let previous = source.remaining.0 - decay;
        source.remaining.0 = previous.saturating_add(added).min(source.capacity.0);
        if source.kind == ResourceKind::Spring && added > 0 {
            source.contamination = ((u128::from(previous) * u128::from(source.contamination)
                + u128::from(added) * u128::from(source.runoff_contamination))
                / u128::from(source.remaining.0.max(1))) as u16;
        }
        Ok(source)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PlotInput<'a> {
    pub plot: &'a Plot,
    pub weather: &'a WeatherFacts,
    pub rules: &'a EcologyRules,
}
impl PlotInput<'_> {
    pub fn evaluate(self) -> Result<Plot, Error> {
        self.plot.validate()?;
        self.weather.validate()?;
        self.rules.validate()?;
        let mut plot = self.plot.clone();
        plot.soil_water = plot
            .soil_water
            .saturating_add(if self.weather.temperature > 0 {
                self.weather.rain
            } else {
                0
            })
            .min(plot.water_capacity)
            .saturating_sub(self.weather.evaporation);
        if self.weather.temperature > self.rules.base_temperature && plot.soil_water > 0 {
            plot.fertility = plot
                .fertility
                .saturating_add(self.rules.soil_recovery)
                .min(1_000);
        }
        Ok(plot)
    }
}
#[derive(Clone, Debug)]
struct Row<T> {
    before: T,
    after: Option<T>,
}
/// Sparse prepared rows; job ledgers and personal knowledge are never replaced.
#[derive(Clone, Debug)]
pub struct PreparedAdvance {
    opening_day: u64,
    next_day: u64,
    rules: EcologyRules,
    resources: Vec<(ResourceId, Row<Resource>)>,
    plots: Vec<(SiteId, Row<Plot>)>,
    membership: bool,
}
impl PreparedAdvance {
    pub fn changed_resource_rows(&self) -> usize {
        self.resources
            .iter()
            .filter(|(_, row)| row.after.is_some())
            .count()
    }
    pub fn changed_plot_rows(&self) -> usize {
        self.plots
            .iter()
            .filter(|(_, row)| row.after.is_some())
            .count()
    }
    pub fn changed_resources(&self) -> impl Iterator<Item = (&ResourceId, &Resource)> {
        self.resources
            .iter()
            .filter_map(|(id, row)| Some((id, row.after.as_ref()?)))
    }
    pub fn changed_plots(&self) -> impl Iterator<Item = (&SiteId, &Plot)> {
        self.plots
            .iter()
            .filter_map(|(id, row)| Some((id, row.after.as_ref()?)))
    }
}
fn unchanged_resource_metadata(a: &Resource, b: &Resource) -> bool {
    a.id == b.id
        && a.area == b.area
        && a.kind == b.kind
        && a.capacity == b.capacity
        && a.daily_growth == b.daily_growth
        && a.decay_permille == b.decay_permille
        && a.water_capacity == b.water_capacity
        && a.optimal_water == b.optimal_water
        && a.minimum_temperature == b.minimum_temperature
        && a.optimum_temperature == b.optimum_temperature
        && a.runoff_contamination == b.runoff_contamination
        && a.labor_per_unit == b.labor_per_unit
        && a.workers == b.workers
        && a.access == b.access
        && a.custodian == b.custodian
}
fn unchanged_plot_metadata(a: &Plot, b: &Plot) -> bool {
    a.id == b.id
        && a.area == b.area
        && a.water_capacity == b.water_capacity
        && a.access == b.access
        && a.custodian == b.custodian
}
impl Runtime {
    pub fn prepare_advance(
        &self,
        store: &Store,
        queries: &Queries,
        day: Option<&LocalWeatherDay>,
        evaluate_resources: impl FnOnce(
            &[(ResourceId, ResourceInput<'_>)],
        ) -> Result<Vec<(ResourceId, Resource)>, Error>,
        evaluate_plots: impl FnOnce(&[(SiteId, PlotInput<'_>)]) -> Result<Vec<(SiteId, Plot)>, Error>,
    ) -> Result<PreparedAdvance, Error> {
        self.rules.validate()?;
        let mut prepared = PreparedAdvance {
            opening_day: self.day,
            next_day: self.day,
            rules: self.rules.clone(),
            resources: Vec::new(),
            plots: Vec::new(),
            membership: false,
        };
        let Some(day) = day else {
            return Ok(prepared);
        };
        for weather in day.cells.values() {
            weather.validate()?;
        }
        if day.day == self.day {
            return Ok(prepared);
        }
        if day.day != self.day.checked_add(1).ok_or(Error::Overflow)? {
            return Err(Error::InvalidSave("ecology skipped a calendar day".into()));
        }
        let mut resources: Vec<_> = queries
            .resources
            .iter(store)?
            .map(|(id, row)| (ResourceId(id.0), row))
            .collect();
        let mut plots: Vec<_> = queries
            .plots
            .iter(store)?
            .map(|(id, row)| (SiteId(id.0), row))
            .collect();
        resources.sort_unstable_by_key(|(id, _)| *id);
        plots.sort_unstable_by_key(|(id, _)| *id);
        for (id, row) in &resources {
            row.validate()?;
            if *id != row.id {
                return Err(Error::InvalidSave("inconsistent resource entity".into()));
            }
        }
        for (id, row) in &plots {
            row.validate()?;
            if *id != row.id {
                return Err(Error::InvalidSave("inconsistent plot entity".into()));
            }
        }
        let resource_inputs: Vec<_> = resources
            .iter()
            .filter_map(|(id, row)| {
                Some((
                    *id,
                    ResourceInput {
                        resource: row,
                        weather: day.cells.get(&row.area)?,
                    },
                ))
            })
            .collect();
        let plot_inputs: Vec<_> = plots
            .iter()
            .filter_map(|(id, row)| {
                Some((
                    *id,
                    PlotInput {
                        plot: row,
                        weather: day.cells.get(&row.area)?,
                        rules: &self.rules,
                    },
                ))
            })
            .collect();
        let resource_output = evaluate_resources(&resource_inputs)?;
        let plot_output = evaluate_plots(&plot_inputs)?;
        if resource_output.len() != resource_inputs.len()
            || resource_output
                .iter()
                .zip(&resource_inputs)
                .any(|((id, _), (expected, _))| id != expected)
            || plot_output.len() != plot_inputs.len()
            || plot_output
                .iter()
                .zip(&plot_inputs)
                .any(|((id, _), (expected, _))| id != expected)
        {
            return Err(Error::InvalidConfiguration(
                "ecology executor changed stable batch keys".into(),
            ));
        }
        for ((_, output), (_, input)) in resource_output.iter().zip(&resource_inputs) {
            output.validate()?;
            if !unchanged_resource_metadata(output, input.resource) {
                return Err(Error::InvalidConfiguration(
                    "renewal changed unrelated resource data".into(),
                ));
            }
        }
        for ((_, output), (_, input)) in plot_output.iter().zip(&plot_inputs) {
            output.validate()?;
            if !unchanged_plot_metadata(output, input.plot) {
                return Err(Error::InvalidConfiguration(
                    "land recovery changed unrelated plot data".into(),
                ));
            }
        }
        drop(resource_inputs);
        drop(plot_inputs);
        let mut resource_output = resource_output.into_iter().peekable();
        for (id, before) in resources {
            let after = if resource_output.peek().is_some_and(|(key, _)| *key == id) {
                Some(resource_output.next().expect("peeked output exists").1)
            } else {
                None
            };
            prepared.resources.push((
                id,
                Row {
                    before: before.clone(),
                    after,
                },
            ));
        }
        let mut plot_output = plot_output.into_iter().peekable();
        for (id, before) in plots {
            let after = if plot_output.peek().is_some_and(|(key, _)| *key == id) {
                Some(plot_output.next().expect("peeked output exists").1)
            } else {
                None
            };
            prepared.plots.push((
                id,
                Row {
                    before: before.clone(),
                    after,
                },
            ));
        }
        prepared.membership = true;
        prepared.next_day = day.day;
        Ok(prepared)
    }
    pub fn prepare_advance_serial(
        &self,
        store: &Store,
        queries: &Queries,
        day: Option<&LocalWeatherDay>,
    ) -> Result<PreparedAdvance, Error> {
        self.prepare_advance(
            store,
            queries,
            day,
            |rows| {
                rows.iter()
                    .map(|(id, input)| Ok((*id, input.evaluate()?)))
                    .collect()
            },
            |rows| {
                rows.iter()
                    .map(|(id, input)| Ok((*id, input.evaluate()?)))
                    .collect()
            },
        )
    }
    pub fn validate_advance(&self, store: &Store, prepared: &PreparedAdvance) -> Result<(), Error> {
        if self.day != prepared.opening_day
            || self.rules != prepared.rules
            || (prepared.membership
                && (self
                    .resources(store)
                    .map(|(id, _)| id)
                    .ne(prepared.resources.iter().map(|(id, _)| *id))
                    || self
                        .plots(store)
                        .map(|(id, _)| id)
                        .ne(prepared.plots.iter().map(|(id, _)| *id))))
            || prepared
                .resources
                .iter()
                .any(|(id, row)| self.resource(store, *id) != Some(&row.before))
            || prepared
                .plots
                .iter()
                .any(|(id, row)| self.plot(store, *id) != Some(&row.before))
        {
            return Err(Error::InvalidSave("stale ecology component batch".into()));
        }
        Ok(())
    }
    pub fn commit_advance(
        &mut self,
        store: &mut Store,
        prepared: PreparedAdvance,
    ) -> Result<(), Error> {
        self.validate_advance(store, &prepared)?;
        for (id, row) in prepared.resources {
            if let Some(after) = row.after {
                *self.resource_mut(store, id).expect("admitted resource") = after;
            }
        }
        for (id, row) in prepared.plots {
            if let Some(after) = row.after {
                *self.plot_mut(store, id).expect("admitted plot") = after;
            }
        }
        self.day = prepared.next_day;
        Ok(())
    }
}

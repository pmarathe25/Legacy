use super::*;

/// Immutable entity inputs. The executor may evaluate these in parallel; the
/// feature supplies the system logic and stable site keys.
#[derive(Clone, Copy, Debug)]
pub struct GrowthInput<'a> {
    pub crop: &'a Crop,
    pub soil: &'a Soil,
    pub rules: &'a CropRules,
    pub weather: &'a WeatherFacts,
}
impl GrowthInput<'_> {
    pub fn evaluate(self) -> Result<GrowthOutput, Error> {
        let (crop, soil) = grow(self.crop, self.soil, self.rules, self.weather)?;
        Ok(GrowthOutput { crop, soil })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrowthOutput {
    pub crop: Crop,
    pub soil: Soil,
}
#[derive(Clone, Debug)]
struct Change<T> {
    before: T,
    after: T,
}
/// Only rows read or changed by this batch are retained. Field ownership and
/// unrelated component stores are never replaced when the batch is committed.
#[derive(Clone, Debug)]
pub struct PreparedAdvance {
    opening_day: u64,
    next_day: u64,
    crops: Vec<(SiteId, Change<Crop>)>,
    soils: Vec<(SiteId, Change<Soil>)>,
    jobs: Vec<(PersonId, Change<Cultivation>)>,
    fields: Vec<(SiteId, (SiteId, AreaId))>,
    rules: BTreeMap<CropId, CropRules>,
}
impl PreparedAdvance {
    pub fn changed_crops(&self) -> impl Iterator<Item = (&SiteId, &Crop)> {
        self.crops.iter().map(|(id, change)| (id, &change.after))
    }
    pub fn failed_crops(&self) -> impl Iterator<Item = SiteId> + '_ {
        self.crops.iter().filter_map(|(id, change)| {
            (change.before.phase != CropPhase::Failed && change.after.phase == CropPhase::Failed)
                .then_some(*id)
        })
    }
    pub fn changed_crop_rows(&self) -> usize {
        self.crops.len()
    }
    pub fn changed_job_rows(&self) -> usize {
        self.jobs.len()
    }
    pub fn growth_rows(&self) -> usize {
        self.soils.len()
    }
}
fn remember_rules(rules: &mut BTreeMap<CropId, CropRules>, id: &CropId, definition: &CropRules) {
    if !rules.contains_key(id) {
        rules.insert(id.clone(), definition.clone());
    }
}
fn merge_site_rows<T>(left: Vec<(SiteId, T)>, right: Vec<(SiteId, T)>) -> Vec<(SiteId, T)> {
    if right.is_empty() {
        return left;
    }
    if left.is_empty() {
        return right;
    }
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let mut left = left.into_iter().peekable();
    let mut right = right.into_iter().peekable();
    while let (Some((left_id, _)), Some((right_id, _))) = (left.peek(), right.peek()) {
        if left_id < right_id {
            merged.push(left.next().expect("peeked row exists"));
        } else {
            merged.push(right.next().expect("peeked row exists"));
        }
    }
    merged.extend(left);
    merged.extend(right);
    merged
}
impl Runtime {
    pub fn next_boundary(&self, now: SimTime, limit: SimTime) -> SimTime {
        self.jobs
            .values()
            .filter_map(|job| (job.completes > now).then_some(job.completes))
            .min()
            .map_or(limit, |time| time.min(limit))
    }

    pub fn prepare_advance(
        &self,
        store: &game_ecs::Store,
        queries: &Queries,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
        evaluate: impl FnOnce(
            &[(SiteId, GrowthInput<'_>)],
        ) -> Result<Vec<(SiteId, GrowthOutput)>, Error>,
    ) -> Result<PreparedAdvance, Error> {
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        let mut prepared = PreparedAdvance {
            opening_day: self.day,
            next_day: self.day,
            crops: Vec::new(),
            soils: Vec::new(),
            jobs: Vec::new(),
            fields: Vec::new(),
            rules: BTreeMap::new(),
        };
        let bad = || Error::InvalidSave("inconsistent cultivation batch".into());
        let mut occupied = BTreeSet::new();
        // Only active cultivation needs keyed preview lookup. Daily growth
        // stages dense rows directly into ordered buffers.
        let mut labor_preview: BTreeMap<SiteId, (Change<Crop>, (SiteId, AreaId))> = BTreeMap::new();
        for (actor, job) in &self.jobs {
            if !occupied.insert(job.field) || next > job.completes {
                return Err(bad());
            }
            let crop = self.crop(store, job.field).ok_or_else(bad)?;
            let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
            crop.validate(rules)?;
            if elapsed == 0 {
                continue;
            }
            let field = self.field(store, job.field).ok_or_else(bad)?;
            let mut next_crop = crop.clone();
            let mut next_job = job.clone();
            next_job.labor_ms = next_job
                .labor_ms
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?;
            let contribution = next_crop.contributions.entry(*actor).or_default();
            *contribution = contribution.checked_add(elapsed).ok_or(Error::Overflow)?;
            let progress = match job.operation {
                FarmOperation::Prepare => Some(&mut next_crop.preparation_ms),
                FarmOperation::Plant => Some(&mut next_crop.planting_ms),
                FarmOperation::Harvest => Some(&mut next_crop.harvest_ms),
                _ => None,
            };
            if let Some(progress) = progress {
                *progress = progress.checked_add(elapsed).ok_or(Error::Overflow)?;
            }
            next_crop.validate(rules)?;
            remember_rules(&mut prepared.rules, &crop.definition, rules);
            labor_preview.insert(
                job.field,
                (
                    Change {
                        before: crop.clone(),
                        after: next_crop,
                    },
                    (field.id, field.area),
                ),
            );
            prepared.jobs.push((
                *actor,
                Change {
                    before: job.clone(),
                    after: next_job,
                },
            ));
        }
        if let Some(day) = day {
            if day.day != self.day.checked_add(1).ok_or(Error::Overflow)? {
                return Err(bad());
            }
            let mut joined = Vec::new();
            for (stable_id, field, opening_crop, soil) in queries.growth.iter(store)? {
                let Some(weather) = day.cells.get(&field.area) else {
                    continue;
                };
                joined.push((SiteId(stable_id.0), field, opening_crop, soil, weather));
            }
            // Native storage order cannot determine authoritative publication.
            joined.sort_unstable_by_key(|(id, ..)| *id);
            let mut inputs = Vec::with_capacity(joined.len());
            for (id, _, opening_crop, soil, weather) in &joined {
                let crop = labor_preview
                    .get(id)
                    .map_or(*opening_crop, |(change, _)| &change.after);
                let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
                inputs.push((
                    *id,
                    GrowthInput {
                        crop,
                        soil,
                        rules,
                        weather,
                    },
                ));
            }
            let outputs = evaluate(&inputs)?;
            if inputs.len() != outputs.len()
                || inputs
                    .iter()
                    .zip(&outputs)
                    .any(|((id, _), (output_id, _))| id != output_id)
            {
                return Err(Error::InvalidConfiguration(
                    "crop executor changed stable batch keys".into(),
                ));
            }
            // Check structural system contracts before any rows can be published.
            for ((_, input), (_, output)) in inputs.iter().zip(&outputs) {
                output.crop.validate(input.rules)?;
                output.soil.validate()?;
                if output.crop.definition != input.crop.definition
                    || output.crop.preparation_ms != input.crop.preparation_ms
                    || output.crop.planting_ms != input.crop.planting_ms
                    || output.crop.harvest_ms != input.crop.harvest_ms
                    || output.crop.tending != input.crop.tending
                    || output.crop.contributions != input.crop.contributions
                {
                    return Err(Error::InvalidConfiguration(
                        "crop growth changed unrelated components".into(),
                    ));
                }
            }
            drop(inputs);
            prepared.crops.reserve(outputs.len());
            prepared.soils.reserve(outputs.len());
            prepared.fields.reserve(outputs.len());
            // Capture opening rows from the actual native join, without repeat
            // stable-ID index lookups or a tree allocation for each guard.
            for ((id, field, opening_crop, soil, _), (_, output)) in joined.into_iter().zip(outputs)
            {
                prepared.fields.push((id, (field.id, field.area)));
                let rules = &self.catalog.crops[&opening_crop.definition];
                remember_rules(&mut prepared.rules, &opening_crop.definition, rules);
                prepared.crops.push((
                    id,
                    Change {
                        before: labor_preview
                            .remove(&id)
                            .map_or_else(|| opening_crop.clone(), |(change, _)| change.before),
                        after: output.crop,
                    },
                ));
                prepared.soils.push((
                    id,
                    Change {
                        before: soil.clone(),
                        after: output.soil,
                    },
                ));
            }
            prepared.next_day = day.day;
        }
        let mut labor_crops = Vec::new();
        let mut labor_fields = Vec::new();
        for (id, (change, location)) in labor_preview {
            labor_crops.push((id, change));
            labor_fields.push((id, location));
        }
        prepared.crops = merge_site_rows(prepared.crops, labor_crops);
        prepared.fields = merge_site_rows(prepared.fields, labor_fields);
        // A zero-duration growth boundary still reads an existing job's claim.
        for (actor, job) in &self.jobs {
            if prepared
                .crops
                .binary_search_by_key(&job.field, |(key, _)| *key)
                .is_ok()
                && let Err(index) = prepared.jobs.binary_search_by_key(actor, |(key, _)| *key)
            {
                prepared.jobs.insert(
                    index,
                    (
                        *actor,
                        Change {
                            before: job.clone(),
                            after: job.clone(),
                        },
                    ),
                );
            }
        }
        Ok(prepared)
    }

    pub fn prepare_advance_serial(
        &self,
        store: &game_ecs::Store,
        queries: &Queries,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
    ) -> Result<PreparedAdvance, Error> {
        self.prepare_advance(store, queries, from, next, day, |inputs| {
            inputs
                .iter()
                .map(|(id, input)| Ok((*id, input.evaluate()?)))
                .collect()
        })
    }

    /// Checks the complete read set without publishing component rows.
    pub fn validate_advance(
        &self,
        store: &game_ecs::Store,
        prepared: &PreparedAdvance,
    ) -> Result<(), Error> {
        if self.day != prepared.opening_day
            || prepared
                .crops
                .iter()
                .any(|(id, change)| self.crop(store, *id) != Some(&change.before))
            || prepared
                .soils
                .iter()
                .any(|(id, change)| self.soil(store, *id) != Some(&change.before))
            || prepared
                .jobs
                .iter()
                .any(|(id, change)| self.jobs.get(id) != Some(&change.before))
            || prepared.fields.iter().any(|(id, expected)| {
                self.field(store, *id).map(|field| (field.id, field.area)) != Some(*expected)
            })
            || prepared
                .rules
                .iter()
                .any(|(id, rules)| self.catalog.crops.get(id) != Some(rules))
            || self.jobs.iter().any(|(actor, job)| {
                prepared
                    .crops
                    .binary_search_by_key(&job.field, |(key, _)| *key)
                    .is_ok()
                    && prepared
                        .jobs
                        .binary_search_by_key(actor, |(key, _)| *key)
                        .is_err()
            })
        {
            return Err(Error::InvalidSave(
                "stale cultivation component batch".into(),
            ));
        }
        Ok(())
    }

    /// Publishes only prepared rows after validating the complete read set.
    pub fn commit_advance(
        &mut self,
        store: &mut game_ecs::Store,
        prepared: PreparedAdvance,
    ) -> Result<(), Error> {
        self.validate_advance(store, &prepared)?;
        for (id, change) in prepared.crops {
            *self.crop_mut(store, id).expect("preflighted crop exists") = change.after;
        }
        for (id, change) in prepared.soils {
            *self.soil_mut(store, id).expect("preflighted soil exists") = change.after;
        }
        for (id, change) in prepared.jobs {
            self.jobs.insert(id, change.after);
        }
        self.day = prepared.next_day;
        Ok(())
    }
}

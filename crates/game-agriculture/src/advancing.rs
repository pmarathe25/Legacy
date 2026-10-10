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
    crops: BTreeMap<SiteId, Change<Crop>>,
    soils: BTreeMap<SiteId, Change<Soil>>,
    jobs: BTreeMap<PersonId, Change<Cultivation>>,
    fields: BTreeMap<SiteId, (SiteId, AreaId)>,
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
impl World {
    pub fn next_boundary(&self, now: SimTime, limit: SimTime) -> SimTime {
        self.jobs
            .values()
            .filter_map(|job| (job.completes > now).then_some(job.completes))
            .min()
            .map_or(limit, |time| time.min(limit))
    }

    pub fn prepare_advance(
        &self,
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
            crops: BTreeMap::new(),
            soils: BTreeMap::new(),
            jobs: BTreeMap::new(),
            fields: BTreeMap::new(),
            rules: BTreeMap::new(),
        };
        let bad = || Error::InvalidSave("inconsistent cultivation batch".into());
        let mut occupied = BTreeSet::new();
        for (actor, job) in &self.jobs {
            if !occupied.insert(job.field) || next > job.completes {
                return Err(bad());
            }
            let crop = self.crops.get(&job.field).ok_or_else(bad)?;
            let rules = self.catalog.crops.get(&crop.definition).ok_or_else(bad)?;
            crop.validate(rules)?;
            if elapsed == 0 {
                continue;
            }
            let field = self.fields.get(&job.field).ok_or_else(bad)?;
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
            prepared.fields.insert(job.field, (field.id, field.area));
            prepared
                .rules
                .entry(crop.definition.clone())
                .or_insert_with(|| rules.clone());
            prepared.crops.insert(
                job.field,
                Change {
                    before: crop.clone(),
                    after: next_crop,
                },
            );
            prepared.jobs.insert(
                *actor,
                Change {
                    before: job.clone(),
                    after: next_job,
                },
            );
        }
        if let Some(day) = day {
            if day.day != self.day.checked_add(1).ok_or(Error::Overflow)? {
                return Err(bad());
            }
            let mut inputs = Vec::new();
            for (id, field) in &self.fields {
                let Some(opening_crop) = self.crops.get(id) else {
                    continue;
                };
                let (Some(soil), Some(weather)) = (self.soils.get(id), day.cells.get(&field.area))
                else {
                    continue;
                };
                let crop = prepared
                    .crops
                    .get(id)
                    .map_or(opening_crop, |change| &change.after);
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
            for (id, output) in outputs {
                let opening_crop = &self.crops[&id];
                let rules = &self.catalog.crops[&opening_crop.definition];
                prepared.fields.entry(id).or_insert_with(|| {
                    let field = &self.fields[&id];
                    (field.id, field.area)
                });
                prepared
                    .rules
                    .entry(opening_crop.definition.clone())
                    .or_insert_with(|| rules.clone());
                match prepared.crops.entry(id) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().after = output.crop
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(Change {
                            before: opening_crop.clone(),
                            after: output.crop,
                        });
                    }
                }
                prepared.soils.insert(
                    id,
                    Change {
                        before: self.soils[&id].clone(),
                        after: output.soil,
                    },
                );
            }
            prepared.next_day = day.day;
        }
        // A zero-duration growth boundary still reads an existing job's claim.
        for (actor, job) in &self.jobs {
            if prepared.crops.contains_key(&job.field) {
                prepared.jobs.entry(*actor).or_insert_with(|| Change {
                    before: job.clone(),
                    after: job.clone(),
                });
            }
        }
        Ok(prepared)
    }

    pub fn prepare_advance_serial(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
    ) -> Result<PreparedAdvance, Error> {
        self.prepare_advance(from, next, day, |inputs| {
            inputs
                .iter()
                .map(|(id, input)| Ok((*id, input.evaluate()?)))
                .collect()
        })
    }

    /// Checks the complete read set without publishing component rows.
    pub fn validate_advance(&self, prepared: &PreparedAdvance) -> Result<(), Error> {
        if self.day != prepared.opening_day
            || prepared
                .crops
                .iter()
                .any(|(id, change)| self.crops.get(id) != Some(&change.before))
            || prepared
                .soils
                .iter()
                .any(|(id, change)| self.soils.get(id) != Some(&change.before))
            || prepared
                .jobs
                .iter()
                .any(|(id, change)| self.jobs.get(id) != Some(&change.before))
            || prepared.fields.iter().any(|(id, expected)| {
                self.fields.get(id).map(|field| (field.id, field.area)) != Some(*expected)
            })
            || prepared
                .rules
                .iter()
                .any(|(id, rules)| self.catalog.crops.get(id) != Some(rules))
            || self.jobs.iter().any(|(actor, job)| {
                prepared.crops.contains_key(&job.field) && !prepared.jobs.contains_key(actor)
            })
        {
            return Err(Error::InvalidSave(
                "stale cultivation component batch".into(),
            ));
        }
        Ok(())
    }

    /// Publishes only prepared rows after validating the complete read set.
    pub fn commit_advance(&mut self, prepared: PreparedAdvance) -> Result<(), Error> {
        self.validate_advance(&prepared)?;
        for (id, change) in prepared.crops {
            self.crops.insert(id, change.after);
        }
        for (id, change) in prepared.soils {
            self.soils.insert(id, change.after);
        }
        for (id, change) in prepared.jobs {
            self.jobs.insert(id, change.after);
        }
        self.day = prepared.next_day;
        Ok(())
    }

    /// Snapshot convenience for callers that need a separate future world.
    /// Runtime integration should prepare and commit the sparse row changes.
    pub fn advance(
        &self,
        from: SimTime,
        next: SimTime,
        day: Option<&LocalWeatherDay>,
    ) -> Result<Self, Error> {
        let prepared = self.prepare_advance_serial(from, next, day)?;
        let mut next = self.clone();
        next.commit_advance(prepared)?;
        Ok(next)
    }
}

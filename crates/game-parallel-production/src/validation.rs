use crate::*;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

type Worker = PersonSnapshot;
impl Production {
    pub fn validate(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
    ) -> Result<(), String> {
        self.validate_people(regions, travelers, now).map(|_| ())
    }
    pub fn validate_people(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
    ) -> Result<Vec<Worker>, String> {
        let people = game_parallel_index::snapshot_people(regions, travelers);
        self.validate_index(regions, &people, now)?;
        Ok(people)
    }
    /// Validate this owner against a sorted persistent person snapshot.
    pub fn validate_index(
        &self,
        regions: &[Region],
        people: &[PersonSnapshot],
        now: u64,
    ) -> Result<(), String> {
        let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
        if region_ids.len() != regions.len() {
            return Err("duplicate region".into());
        }
        if people.par_windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || people
                .par_iter()
                .any(|person| person.1.is_some_and(|region| !region_ids.contains(&region)))
        {
            return Err("invalid person index".into());
        }
        let mut goods = HashSet::new();
        if self.goods.iter().any(|g| {
            g.name.trim().is_empty()
                || !goods.insert(g.id)
                || g.food.as_ref().is_some_and(|food| {
                    food.portion == 0
                        || food.hunger_relief == 0
                        || food.nutrients.keys().any(|key| key.trim().is_empty())
                })
        }) {
            return Err("invalid goods".into());
        }
        let mut recipes = HashSet::new();
        for recipe in &self.recipes {
            if recipe.name.trim().is_empty()
                || recipe.ticks == 0
                || !recipes.insert(recipe.id)
                || !valid_items(&recipe.inputs, &goods)
                || !valid_items(&recipe.outputs, &goods)
            {
                return Err("invalid recipe".into());
            }
        }
        let mut categorized = HashSet::new();
        if self.recipe_categories.iter().any(|row| {
            row.category.trim().is_empty()
                || !recipes.contains(&row.recipe)
                || !categorized.insert(row.recipe)
        }) {
            return Err("invalid recipe skill category".into());
        }
        let mut inventory_regions = HashSet::new();
        for inventory in &self.inventories {
            if !region_ids.contains(&inventory.region)
                || !inventory_regions.insert(inventory.region)
            {
                return Err("invalid inventory region".into());
            }
            let mut seen = HashSet::new();
            if inventory.stocks.iter().any(|s| {
                !goods.contains(&s.good)
                    || !seen.insert(s.good)
                    || s.capacity == 0
                    || s.quantity > s.capacity
            }) {
                return Err("invalid stock".into());
            }
        }
        let inventories: HashMap<_, _> = self.inventories.iter().map(|i| (i.region, i)).collect();
        let mut job_ids: Vec<_> = self.jobs.par_iter().map(|j| j.id).collect();
        if job_ids.par_windows(2).any(|pair| pair[0] > pair[1]) {
            job_ids.par_sort_unstable();
        }
        if job_ids.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("invalid job".into());
        }
        let mut workers: Vec<_> = self
            .jobs
            .par_iter()
            .filter(|j| j.phase == JobPhase::Working)
            .map(|j| j.worker)
            .collect();
        if workers.par_windows(2).any(|pair| pair[0] > pair[1]) {
            workers.par_sort_unstable();
        }
        if workers.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("invalid job".into());
        }
        if self.jobs.par_iter().any(|job| {
            let Some(inventory) = inventories.get(&job.region) else {
                return true;
            };
            person_index(people, job.worker).is_none()
                || !recipes.contains(&job.recipe)
                || !valid_items(&job.outputs, &goods)
                || job.outputs.iter().any(|a| {
                    !inventory
                        .stocks
                        .iter()
                        .any(|s| s.good == a.good && a.quantity <= s.capacity)
                })
                || match job.phase {
                    JobPhase::Working => {
                        job.remaining == 0
                            || !valid_items(&job.reserved, &goods)
                            || job
                                .reserved
                                .iter()
                                .any(|a| !inventory.stocks.iter().any(|s| s.good == a.good))
                    }
                    JobPhase::Waiting => job.remaining != 0 || !job.reserved.is_empty(),
                }
        }) {
            return Err("invalid job".into());
        }
        if self.events.iter().any(|e| e.tick > now) {
            return Err("invalid production event".into());
        }
        Ok(())
    }

    /// Cancel unfinished work only when every reserved input fits back in storage.
    pub fn cancel(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
        job_id: u64,
    ) -> Result<Self, String> {
        self.validate(regions, travelers, now)?;
        self.cancel_validated(job_id)
    }
    pub fn cancel_indexed(
        &self,
        regions: &[Region],
        people: &[PersonSnapshot],
        now: u64,
        job_id: u64,
    ) -> Result<Self, String> {
        self.validate_index(regions, people, now)?;
        self.cancel_validated(job_id)
    }
    fn cancel_validated(&self, job_id: u64) -> Result<Self, String> {
        let job = self
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .ok_or("unknown job")?;
        if job.phase != JobPhase::Working {
            return Err("completed goods cannot be cancelled".into());
        }
        let inventory = self
            .inventories
            .iter()
            .find(|i| i.region == job.region)
            .unwrap();
        for item in &job.reserved {
            let stock = inventory
                .stocks
                .iter()
                .find(|s| s.good == item.good)
                .unwrap();
            if item.quantity > stock.capacity - stock.quantity {
                return Err("reserved inputs do not fit in storage".into());
            }
        }
        let mut next = self.clone();
        let target = next
            .inventories
            .iter_mut()
            .find(|i| i.region == job.region)
            .unwrap();
        for item in &job.reserved {
            target
                .stocks
                .iter_mut()
                .find(|s| s.good == item.good)
                .unwrap()
                .quantity += item.quantity;
        }
        next.jobs.retain(|j| j.id != job_id);
        Ok(next)
    }

    /// A living local replacement may resume unfinished work with held inputs.
    pub fn reassign(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
        job_id: u64,
        worker: u64,
    ) -> Result<Self, String> {
        let people = self.validate_people(regions, travelers, now)?;
        self.reassign_validated(&people, job_id, worker)
    }
    pub fn reassign_indexed(
        &self,
        regions: &[Region],
        people: &[PersonSnapshot],
        now: u64,
        job_id: u64,
        worker: u64,
    ) -> Result<Self, String> {
        self.validate_index(regions, people, now)?;
        self.reassign_validated(people, job_id, worker)
    }
    fn reassign_validated(
        &self,
        people: &[PersonSnapshot],
        job_id: u64,
        worker: u64,
    ) -> Result<Self, String> {
        let job = self
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .ok_or("unknown job")?;
        if job.phase != JobPhase::Working {
            return Err("job is not working".into());
        }
        if self
            .jobs
            .iter()
            .any(|j| j.id != job_id && j.worker == worker && j.phase == JobPhase::Working)
        {
            return Err("worker is busy".into());
        }
        if !person_index(people, worker)
            .is_some_and(|index| people[index].1 == Some(job.region) && people[index].2 > 0)
        {
            return Err("worker must be living and local".into());
        }
        let mut next = self.clone();
        next.jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .unwrap()
            .worker = worker;
        Ok(next)
    }
}

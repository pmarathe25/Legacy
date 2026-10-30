//! Checked cancellation and reassignment for fused work slots.
use game_parallel::World;
use game_parallel_fused_data::{Block, NONE, uniform_output};
use game_parallel_fused_metadata::Store;
pub struct Context<'a> {
    pub world: &'a mut World,
    pub blocks: &'a mut [Block],
    pub positions: &'a [(u64, usize, usize)],
    pub jobs: &'a mut Store,
    pub waiting: &'a [usize],
    pub live_count: &'a mut usize,
    pub revision: &'a mut u64,
}
impl Context<'_> {
    fn person(&self, id: u64) -> Result<(usize, usize), String> {
        let index = self
            .positions
            .binary_search_by_key(&id, |row| row.0)
            .map_err(|_| "unknown worker")?;
        Ok((self.positions[index].1, self.positions[index].2))
    }
    fn active(&self, id: u64) -> Result<(usize, usize, usize), String> {
        let handle = self.jobs.find(id).ok_or("unknown job")?;
        let (block, offset) = self.person(self.jobs.row(handle).worker)?;
        if self.blocks[block].work[offset] == handle {
            return Ok((handle, block, offset));
        }
        if self
            .waiting
            .binary_search_by_key(&id, |&handle| self.jobs.row(handle).id)
            .is_ok()
        {
            return Err("completed goods cannot be changed".into());
        }
        Err("unknown job".into())
    }
    fn resolve_interrupted(&mut self, id: u64) {
        let Some(link) = self
            .world
            .autowork
            .as_ref()
            .and_then(|a| a.requests.as_ref())
            .and_then(|r| r.work_links.iter().find(|link| link.job == id))
            .cloned()
        else {
            return;
        };
        let visible = self
            .positions
            .binary_search_by_key(&link.requester, |row| row.0)
            .ok()
            .is_some_and(|i| {
                let (_, block, offset) = self.positions[i];
                let person = &self.blocks[block];
                person.journey.is_none()
                    && person.health[offset] > 0
                    && self.world.regions[person.region].id == link.region
            });
        self.world
            .autowork
            .as_mut()
            .unwrap()
            .requests
            .as_mut()
            .unwrap()
            .resolve(&[(id, false)], self.world.tick, |_| visible);
    }
    pub fn cancel(&mut self, id: u64) -> Result<(), String> {
        if self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
            .is_some_and(|person| {
                self.jobs
                    .find(id)
                    .is_some_and(|handle| self.jobs.row(handle).worker == person)
            })
        {
            return Err("player control is suspended".into());
        }
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        let (handle, block, offset) = self.active(id)?;
        let row = self.jobs.row(handle);
        let bundle = &self.jobs.bundles[row.bundle as usize];
        let production = self
            .world
            .production
            .as_ref()
            .ok_or("production is disabled")?;
        let inventory = production
            .inventories
            .iter()
            .find(|inventory| inventory.region == row.region)
            .ok_or("missing inventory")?;
        if bundle.reserved.iter().any(|item| {
            inventory
                .stocks
                .iter()
                .find(|stock| stock.good == item.good)
                .is_none_or(|stock| item.quantity > stock.capacity - stock.quantity)
        }) {
            return Err("reserved inputs do not fit in storage".into());
        }
        let live = self
            .live_count
            .checked_sub(1)
            .ok_or("invalid live job count")?;
        let production = self.world.production.as_mut().unwrap();
        let inventory = production
            .inventories
            .iter_mut()
            .find(|inventory| inventory.region == row.region)
            .unwrap();
        for item in &bundle.reserved {
            inventory
                .stocks
                .iter_mut()
                .find(|stock| stock.good == item.good)
                .unwrap()
                .quantity += item.quantity;
        }
        self.blocks[block].work[offset] = NONE;
        self.blocks[block].remaining[offset] = 0;
        self.blocks[block].work_local[offset] = false;
        self.blocks[block].uniform_output = uniform_output(&self.blocks[block].work, self.jobs);
        *self.live_count = live;
        *self.revision = revision;
        if live == 0 {
            *self.jobs = Store::empty();
        }
        self.resolve_interrupted(id);
        Ok(())
    }
    pub fn reassign(&mut self, id: u64, worker: u64) -> Result<(), String> {
        game_parallel_fused_conflict::check(self.world, &[worker])?;
        if self
            .world
            .lineage
            .as_ref()
            .and_then(|lineage| lineage.suspended_person(self.world.tick))
            .is_some_and(|person| {
                person == worker
                    || self
                        .jobs
                        .find(id)
                        .is_some_and(|handle| self.jobs.row(handle).worker == person)
            })
        {
            return Err("player control is suspended".into());
        }
        if let (Some(property), Some(handle)) = (&self.world.property, self.jobs.find(id)) {
            let job = self.jobs.row(handle);
            property.check_jobs(
                &self.world.production.as_ref().unwrap().recipes,
                &[game_parallel::StartWork {
                    id,
                    worker,
                    region: job.region,
                    recipe: job.recipe,
                    units: 1,
                }],
            )?;
        }
        let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
        let (handle, old_block, old_offset) = self.active(id)?;
        let (new_block, new_offset) = self.person(worker)?;
        let target = &self.blocks[new_block];
        if target.journey.is_some()
            || target.health[new_offset] == 0
            || self.world.regions[target.region].id != self.jobs.row(handle).region
        {
            return Err("worker must be living and local".into());
        }
        if target.work[new_offset] != NONE && target.work[new_offset] != handle {
            return Err("worker is busy".into());
        }
        let changed_worker = self.jobs.row(handle).worker != worker;
        let remaining = self.blocks[old_block].remaining[old_offset];
        self.blocks[old_block].work[old_offset] = NONE;
        self.blocks[old_block].remaining[old_offset] = 0;
        self.blocks[old_block].work_local[old_offset] = false;
        self.blocks[new_block].work[new_offset] = handle;
        self.blocks[new_block].remaining[new_offset] = remaining;
        self.blocks[new_block].work_local[new_offset] = true;
        self.jobs.row_mut(handle).worker = worker;
        self.blocks[old_block].uniform_output =
            uniform_output(&self.blocks[old_block].work, self.jobs);
        if old_block != new_block {
            self.blocks[new_block].uniform_output =
                uniform_output(&self.blocks[new_block].work, self.jobs);
        }
        *self.revision = revision;
        if changed_worker {
            self.resolve_interrupted(id);
        }
        Ok(())
    }
}

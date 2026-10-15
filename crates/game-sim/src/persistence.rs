use crate::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    version: u32,
    plugins: BTreeMap<String, u32>,
    resources: BTreeMap<String, Box<serde_json::value::RawValue>>,
    time: SimTime,
    seed: u64,
    revision: u64,
    next_id: u64,
    next_notice: u64,
    jobs: Vec<Job>,
    notices: VecDeque<Notice>,
    receipts: BTreeMap<u64, Receipt>,
}
impl Host {
    fn validate_saved_state(&self) -> Result<()> {
        let mut seen = std::collections::BTreeSet::new();
        for (id, codec) in &self.registry.resources {
            (codec.validate)(&self.data[id])?;
            for allocated in (codec.ids)(&self.data[id]) {
                require(
                    allocated > 0 && allocated < self.next_id && seen.insert(allocated),
                    "invalid or reused saved identity",
                )?;
            }
        }
        for job in self.jobs.values() {
            require(
                seen.insert(job.id.0),
                "schedule identity collides with an entity",
            )?;
        }
        for action in self.get::<Actions>()?.0.values() {
            require(
                self.registry.request_owners.get(&action.cancel.key) == Some(&action.owner),
                "invalid saved action owner",
            )?;
            let decoder = self
                .registry
                .decoders
                .get(&action.cancel.key)
                .ok_or_else(|| Error::InvalidSave("unknown cancellation".into()))?;
            decoder(action.cancel.payload.clone())?;
        }
        for receipt in self.receipts.values() {
            let decoder = self
                .registry
                .decoders
                .get(&receipt.request.key)
                .ok_or_else(|| Error::InvalidSave("unknown cached command".into()))?;
            decoder(receipt.request.payload.clone())?;
        }
        for check in &self.registry.save_checks {
            check(self)?;
        }
        Ok(())
    }
    pub fn save(&self) -> Result<String> {
        self.validate_saved_state()?;
        let resources = self
            .registry
            .resources
            .iter()
            .map(|(id, codec)| Ok((codec.key.into(), (codec.encode)(&self.data[id])?)))
            .collect::<Result<_>>()?;
        serde_json::to_string(&Save {
            version: 1,
            plugins: self.registry.plugins.clone(),
            resources,
            time: self.time,
            seed: self.seed,
            revision: self.revision,
            next_id: self.next_id,
            next_notice: self.next_notice,
            jobs: self.jobs.values().cloned().collect(),
            notices: self.notices.clone(),
            receipts: self.receipts.clone(),
        })
        .map_err(registry::json_error)
    }
}
impl Builder {
    pub fn load(self, source: &str, workers: usize) -> Result<Host> {
        let save: Save = serde_json::from_str(source).map_err(registry::json_error)?;
        require(
            save.version == 1 && save.plugins == self.registry.plugins,
            "incompatible save manifest",
        )?;
        require(
            save.resources.len() == self.registry.resources.len(),
            "save resource set differs",
        )?;
        let mut host = self.build(save.seed, workers)?;
        for (id, codec) in &host.registry.resources {
            let value = save
                .resources
                .get(codec.key)
                .ok_or_else(|| Error::InvalidSave(format!("missing {}", codec.key)))?;
            host.data.insert(*id, (codec.decode)(value.get())?);
        }
        let mut seen = std::collections::BTreeSet::new();
        for job in save.jobs {
            require(
                job.at >= save.time && job.id.0 < save.next_id && seen.insert(job.id),
                "invalid saved schedule",
            )?;
            require(
                host.registry.handlers.contains_key(&job.request.key),
                "unknown saved request",
            )?;
            host.registry.decoders[&job.request.key](job.request.payload.clone())?;
            let key = (job.at, job.priority, job.id);
            host.job_keys.insert(job.id, key);
            host.jobs.insert(key, job);
        }
        require(
            save.next_id > 0 && save.next_notice > 0,
            "invalid saved counters",
        )?;
        for notice in &save.notices {
            require(
                notice.at <= save.time && notice.sequence < save.next_notice,
                "invalid saved event",
            )?;
        }
        for (id, codec) in &host.registry.resources {
            (codec.validate)(&host.data[id])?;
        }
        host.time = save.time;
        host.revision = save.revision;
        host.next_id = save.next_id;
        host.next_notice = save.next_notice;
        host.notices = save.notices;
        host.receipts = save.receipts;
        host.validate_saved_state()?;
        Ok(host)
    }
}

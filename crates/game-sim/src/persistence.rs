use crate::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Save<R> {
    version: u32,
    plugins: BTreeMap<String, u32>,
    resources: BTreeMap<String, R>,
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
                self.registry
                    .request_owners
                    .get(action.cancel.key())
                    .map(|id| &self.registry.owners[*id])
                    == Some(&action.owner),
                "invalid saved action owner",
            )?;
            require(action.cancel.bound(), "unbound cancellation")?;
        }
        for receipt in self.receipts.values() {
            let decoder = self
                .registry
                .decoders
                .get(&receipt.request.key)
                .ok_or_else(|| Error::InvalidSave("unknown cached command".into()))?;
            decoder(pending::Input::Value(receipt.request.payload.clone()))?;
        }
        for check in &self.registry.save_checks {
            check(self)?;
        }
        Ok(())
    }
    pub fn save(&self) -> Result<String> {
        let mut bytes = Vec::new();
        self.save_to(&mut bytes)?;
        Ok(String::from_utf8(bytes).expect("JSON is UTF-8"))
    }
    pub fn save_to(&self, out: &mut dyn std::io::Write) -> Result<()> {
        self.validate_saved_state()?;
        let metadata = serde_json::to_string(&Save {
            version: 1,
            plugins: self.registry.plugins.clone(),
            resources: BTreeMap::<String, ()>::new(),
            time: self.time,
            seed: self.seed,
            revision: self.revision,
            next_id: self.next_id,
            next_notice: self.next_notice,
            jobs: self.jobs.values().cloned().collect(),
            notices: self.notices.clone(),
            receipts: self.receipts.clone(),
        })
        .map_err(registry::json_error)?;
        let (prefix, suffix) = metadata
            .split_once("\"resources\":{}")
            .expect("resource slot");
        let io = |e: std::io::Error| Error::InvalidSave(e.to_string());
        out.write_all(prefix.as_bytes()).map_err(&io)?;
        out.write_all(b"\"resources\":{").map_err(&io)?;
        let mut codecs: Vec<_> = self.registry.resources.iter().collect();
        codecs.sort_by_key(|(_, c)| c.key);
        for (n, (id, codec)) in codecs.into_iter().enumerate() {
            if n > 0 {
                out.write_all(b",").map_err(&io)?;
            }
            serde_json::to_writer(&mut *out, codec.key).map_err(registry::json_error)?;
            out.write_all(b":").map_err(&io)?;
            (codec.encode)(&self.data[id], out)?;
        }
        out.write_all(b"}").map_err(&io)?;
        out.write_all(suffix.as_bytes()).map_err(io)
    }
}
impl Builder {
    pub fn load(self, source: &str, workers: usize) -> Result<Host> {
        let save: Save<&serde_json::value::RawValue> =
            serde_json::from_str(source).map_err(registry::json_error)?;
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
        // Resource decoding has no registry access. Bind action payloads once
        // after all owners and resources exist, before exposing the loaded host.
        let stored = host
            .data
            .get_mut(&TypeId::of::<Actions>())
            .expect("loaded actions");
        let actions = Arc::get_mut(stored)
            .expect("fresh resource")
            .downcast_mut::<Actions>()
            .expect("action type");
        let ids: Vec<_> = actions.0.keys().copied().collect();
        for id in ids {
            let action = actions.0.get_mut(&id).expect("loaded action");
            action.cancel.bind(&host.registry)?;
        }
        for mut job in save.jobs {
            require(
                job.at >= save.time && job.id.0 < save.next_id && seen.insert(job.id),
                "invalid saved schedule",
            )?;
            job.request.bind(&host.registry)?;
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

use crate::*;
use game_snapshot::Decoded;
type Save = game_snapshot::Manifest<Job, Notice, Receipt>;
impl Host {
    fn validate_saved_state(&self) -> Result<()> {
        let mut seen = self.registry.resources.validate(&self.data, self.next_id)?;
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
        game_snapshot::to_string(|out| self.save_to(out))
    }
    pub fn save_to(&self, out: &mut dyn std::io::Write) -> Result<()> {
        self.validate_saved_state()?;
        let metadata = Save {
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
        };
        game_snapshot::write_with_resources(out, &metadata, |out| {
            self.registry.resources.write_entries(&self.data, out)
        })
    }
}
impl Builder {
    pub fn load(self, source: &str, workers: usize) -> Result<Host> {
        let codecs = self.registry.resources.values().map(|c| (c.key, c.decode));
        let save = game_snapshot::from_str(source, codecs).map_err(registry::json_error)?;
        self.restore(save, workers)
    }
    pub fn load_from(self, source: impl std::io::Read, workers: usize) -> Result<Host> {
        let codecs = self.registry.resources.values().map(|c| (c.key, c.decode));
        let save = game_snapshot::from_reader(source, codecs).map_err(registry::json_error)?;
        self.restore(save, workers)
    }
    fn restore(self, decoded: Decoded<Save, Stored>, workers: usize) -> Result<Host> {
        let game_snapshot::Decoded {
            metadata: save,
            resources,
        } = decoded;
        require(
            save.version == 1 && save.plugins == self.registry.plugins,
            "incompatible save manifest",
        )?;
        let mut host = self.build(save.seed, workers)?;
        host.data = host.registry.resources.restore(resources)?;
        let mut seen = std::collections::BTreeSet::new();
        let actions =
            registry::mutable::<Actions>(host.data.get_mut(&TypeId::of::<Actions>()).unwrap());
        for id in actions.0.keys().copied().collect::<Vec<_>>() {
            let action = actions.0.get_mut(&id).expect("loaded action");
            action.cancel.bind(&host.registry)?;
        }
        for mut job in save.jobs {
            let valid = job.id.0 > 0 && job.id.0 < save.next_id && seen.insert(job.id);
            require(valid && job.at >= save.time, "invalid saved schedule")?;
            job.request.bind(&host.registry)?;
            let key = (job.at, job.priority, job.id);
            host.job_keys.insert(job.id, key);
            host.jobs.insert(key, job);
        }
        game_contracts::validate_notices(&save.notices, save.time, save.next_id, save.next_notice)?;
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

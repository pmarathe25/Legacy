use crate::*;
use serde::de::DeserializeOwned;
use std::any::Any;
pub type Checkpoint = Box<dyn Any + Send + Sync>;
/// Mutable fields must clone independently; resource state cannot share mutable aliases.
pub trait Resource: Clone + Default + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    /// Snapshot metadata or begin a row journal. These storage hooks must not run gameplay.
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new(self.clone())
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        *self = *checkpoint
            .downcast::<Self>()
            .expect("resource checkpoint type");
    }
    /// Infallible storage housekeeping; keep nested undo until the outer commit.
    fn commit(&mut self, _checkpoint: Checkpoint) {}
    fn validate_checkpoint(&self, checkpoint: &dyn Any) -> Result<()> {
        self.validate_changes(
            checkpoint
                .downcast_ref::<Self>()
                .expect("resource checkpoint type"),
        )
    }
    fn allocated_ids(&self) -> Vec<u64> {
        vec![]
    }
    fn validate_changes(&self, _previous: &Self) -> Result<()> {
        self.validate()
    }
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}
/// Requests clone independent immutable values; shared mutable aliases are unsupported.
pub trait Request: Clone + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    type Response: Serialize + DeserializeOwned;
}
pub trait Event: Serialize + DeserializeOwned + 'static {
    const KEY: &'static str;
}
type TypedOperation<R> =
    dyn Fn(&mut Transaction<'_>, R) -> Result<<R as Request>::Response> + Send + Sync;
pub(crate) struct Typed<R: Request> {
    pub owner: usize,
    pub run: Box<TypedOperation<R>>,
}
pub(crate) type Stored = Arc<dyn Any + Send + Sync>;
type Listener = Arc<dyn Fn(&mut Transaction<'_>, &dyn Any) -> Result<()> + Send + Sync>;
fn register_listener<T: 'static>(
    listeners: &mut BTreeMap<String, Listener>,
    owner: usize,
    name: &str,
    listener: impl Fn(&mut Transaction<'_>, &T) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    if listeners.contains_key(name) {
        return Err(Error::Duplicate(name.into()));
    }
    listeners.insert(
        name.into(),
        Arc::new(move |tx, value| {
            let value = value
                .downcast_ref::<T>()
                .ok_or_else(|| Error::Rejected("listener type mismatch".into()))?;
            tx.with_owner(owner, |tx| listener(tx, value))
        }),
    );
    Ok(())
}
type Decoder = for<'a> fn(pending::Input<'a>) -> Result<Pending>;
type Guard = Box<dyn Fn(&Host, &Envelope) -> Result<()> + Send + Sync>;
type Stopper = Box<dyn Fn(&Host) -> Result<Option<String>> + Send + Sync>;
pub(crate) struct Codec {
    pub key: &'static str,
    pub owner: usize,
    pub encode: fn(&Stored, &mut dyn std::io::Write) -> Result<()>,
    pub decode: fn(&str) -> Result<Stored>,
    pub ids: fn(&Stored) -> Vec<u64>,
    pub validate: fn(&Stored) -> Result<()>,
    pub checkpoint: fn(&mut Stored) -> Checkpoint,
    pub restore: fn(&mut Stored, Checkpoint),
    pub commit: fn(&mut Stored, Checkpoint),
    pub validate_checkpoint: fn(&Stored, &dyn Any) -> Result<()>,
}
#[derive(Default)]
pub(crate) struct Registry {
    pub resources: HashMap<TypeId, Codec>,
    pub owners: Vec<Arc<str>>,
    pub typed: HashMap<TypeId, Stored>,
    pub batches: HashMap<String, usize>,
    pub request_owners: HashMap<String, usize>,
    pub listeners: HashMap<String, BTreeMap<String, Listener>>,
    pub save_checks: Vec<fn(&Host) -> Result<()>>,
    pub guards: Vec<Guard>,
    pub stops: Vec<Stopper>,
    pub plugins: BTreeMap<String, u32>,
    pub before: HashMap<String, BTreeMap<String, Listener>>,
    pub decoders: HashMap<String, Decoder>,
}
impl Registry {
    pub fn bind(&self, envelope: Envelope) -> Result<Pending> {
        self.bind_payload(&envelope.key, pending::Input::Value(envelope.payload))
    }
    pub fn bind_payload(&self, key: &str, payload: pending::Input<'_>) -> Result<Pending> {
        self.decoders
            .get(key)
            .ok_or_else(|| Error::Missing(key.into()))?(payload)
    }
}
pub struct Builder {
    pub(crate) registry: Registry,
    owner: usize,
    data: HashMap<TypeId, Stored>,
}
impl Builder {
    pub fn new() -> Self {
        let mut b = Self {
            registry: Registry::default(),
            owner: 0,
            data: HashMap::default(),
        };
        b.plugin("sim", 1).expect("fresh builder");
        b.resource::<Actions>().expect("fresh builder");
        b
    }
    pub fn plugin(&mut self, name: &str, version: u32) -> Result<()> {
        require(version > 0, "plugin version must be positive")?;
        if self.registry.plugins.contains_key(name) {
            return Err(Error::Duplicate(name.into()));
        }
        self.registry.plugins.insert(name.into(), version);
        self.owner = self.registry.owners.len();
        self.registry.owners.push(name.into());
        Ok(())
    }
    pub fn resource<T: Resource>(&mut self) -> Result<()> {
        if self.registry.resources.contains_key(&TypeId::of::<T>())
            || self.registry.resources.values().any(|c| c.key == T::KEY)
        {
            return Err(Error::Duplicate(T::KEY.into()));
        }
        self.registry.resources.insert(
            TypeId::of::<T>(),
            Codec {
                key: T::KEY,
                owner: self.owner,
                encode: |s, out| serde_json::to_writer(out, value::<T>(s)).map_err(json_error),
                decode: |v| {
                    let x: T = serde_json::from_str(v).map_err(json_error)?;
                    Ok(Arc::new(x))
                },
                ids: |s| value::<T>(s).allocated_ids(),
                validate: |s| value::<T>(s).validate(),
                checkpoint: |s| mutable::<T>(s).checkpoint(),
                restore: |s, checkpoint| mutable::<T>(s).restore(checkpoint),
                commit: |s, checkpoint| mutable::<T>(s).commit(checkpoint),
                validate_checkpoint: |s, checkpoint| value::<T>(s).validate_checkpoint(checkpoint),
            },
        );
        let initial = T::default();
        initial.validate()?;
        self.data.insert(TypeId::of::<T>(), Arc::new(initial));
        Ok(())
    }
    pub fn request<R: Request>(
        &mut self,
        handler: impl Fn(&mut Transaction<'_>, R) -> Result<R::Response> + Send + Sync + 'static,
    ) -> Result<()> {
        if self.registry.decoders.contains_key(R::KEY) {
            return Err(Error::Duplicate(R::KEY.into()));
        }
        self.registry
            .decoders
            .insert(R::KEY.into(), |value| value.decode::<R>());
        self.registry
            .request_owners
            .insert(R::KEY.into(), self.owner);
        let typed = Arc::new(Typed::<R> {
            owner: self.owner,
            run: Box::new(handler),
        });
        self.registry.typed.insert(TypeId::of::<R>(), typed);
        Ok(())
    }
    /// Opt into atomic, ordered batches of adjacent due requests.
    pub fn batch<R: Request>(&mut self, limit: usize) -> Result<()> {
        require((1..=128).contains(&limit), "batch limit must be 1..=128")?;
        require(
            self.registry.request_owners.get(R::KEY) == Some(&self.owner),
            "batch belongs to request owner",
        )?;
        self.registry.batches.insert(R::KEY.into(), limit);
        Ok(())
    }
    pub fn subscribe<E: Event>(
        &mut self,
        name: &str,
        listener: impl Fn(&mut Transaction<'_>, &E) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        register_listener(
            self.registry.listeners.entry(E::KEY.into()).or_default(),
            self.owner,
            name,
            listener,
        )
    }
    pub fn before<R: Request>(
        &mut self,
        name: &str,
        check: impl Fn(&mut Transaction<'_>, &R) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        register_listener(
            self.registry.before.entry(R::KEY.into()).or_default(),
            self.owner,
            name,
            check,
        )
    }
    pub fn validate_save(&mut self, check: fn(&Host) -> Result<()>) {
        self.registry.save_checks.push(check);
    }
    pub fn guard(
        &mut self,
        guard: impl Fn(&Host, &Envelope) -> Result<()> + Send + Sync + 'static,
    ) {
        self.registry.guards.push(Box::new(guard));
    }
    pub fn stop(&mut self, stop: impl Fn(&Host) -> Result<Option<String>> + Send + Sync + 'static) {
        self.registry.stops.push(Box::new(stop));
    }
    pub fn build(self, seed: u64, workers: usize) -> Result<Host> {
        require(workers > 0 && workers <= 64, "worker count must be 1..=64")?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|_| Error::WorkerFailure)?;
        Ok(Host {
            registry: Arc::new(self.registry),
            data: self.data,
            pool,
            time: SimTime(0),
            seed,
            revision: 0,
            next_id: 1,
            jobs: BTreeMap::new(),
            job_keys: BTreeMap::new(),
            notices: VecDeque::new(),
            next_notice: 1,
            receipts: BTreeMap::new(),
        })
    }
}
impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
pub(crate) fn json_error(e: serde_json::Error) -> Error {
    Error::InvalidSave(e.to_string())
}
pub(crate) fn mutable<T: Resource>(stored: &mut Stored) -> &mut T {
    Arc::get_mut(stored)
        .expect("exclusive transaction resource")
        .downcast_mut()
        .expect("registered resource type")
}

fn value<T: Resource>(stored: &Stored) -> &T {
    stored
        .downcast_ref::<T>()
        .expect("registered resource type")
}
pub(crate) fn read<T: Resource>(data: &HashMap<TypeId, Stored>) -> Result<&T> {
    data.get(&TypeId::of::<T>())
        .and_then(|s| s.downcast_ref())
        .ok_or_else(|| Error::Missing(T::KEY.into()))
}

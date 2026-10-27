use crate::*;
pub use game_contracts::{Checkpoint, Resource};
pub(crate) use game_contracts::{Stored, json_error, mutable, read};
use std::any::Any;
type TypedOperation<R> =
    dyn Fn(&mut Transaction<'_>, R) -> Result<<R as Request>::Response> + Send + Sync;
pub(crate) struct Typed<R: Request> {
    pub owner: usize,
    pub run: Box<TypedOperation<R>>,
}
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
    #[cfg(feature = "review-trace")]
    let label = name.to_owned();
    listeners.insert(
        name.into(),
        Arc::new(move |tx, value| {
            #[cfg(feature = "review-trace")]
            let _cause = game_contracts::causal::scope("listener", &label);
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
#[derive(Default)]
pub(crate) struct Registry {
    pub resources: game_contracts::Catalog,
    pub owners: Vec<Arc<str>>,
    pub typed: HashMap<TypeId, Stored>,
    pub batches: HashMap<String, usize>,
    pub waves: HashMap<String, crate::scheduled::Wave>,
    pub request_owners: HashMap<String, usize>,
    pub listeners: HashMap<String, BTreeMap<String, Listener>>,
    pub isolated: HashMap<String, HashMap<String, game_contracts::drafting::Isolation>>,
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
    pub(crate) owner: usize,
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
        self.registry
            .resources
            .register::<T>(self.owner, &mut self.data)
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
    /// Independent drafts must not inspect partial results of other subscribers.
    pub fn subscribe_isolated<E: Event, T: Resource>(
        &mut self,
        name: &str,
        listener: impl Fn(&mut Transaction<'_>, &E) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        self.subscribe::<E>(name, listener)?;
        self.registry
            .isolated
            .entry(E::KEY.into())
            .or_default()
            .insert(name.into(), game_contracts::drafting::Isolation::new::<T>());
        Ok(())
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
            pool: Arc::new(pool),
            parallel_listeners: false,
            drafts: Default::default(),
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

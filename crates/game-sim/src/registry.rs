use crate::*;
use serde::de::DeserializeOwned;
use std::any::Any;
/// Mutable fields must clone independently; resource state cannot share mutable aliases.
pub trait Resource: Clone + Default + Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    fn allocated_ids(&self) -> Vec<u64> {
        vec![]
    }
    /// Validate changed records against the committed resource. Full validation
    /// remains mandatory when constructing or loading a save.
    fn validate_changes(&self, _previous: &Self) -> Result<()> {
        self.validate()
    }
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}
pub trait Request: Serialize + DeserializeOwned + Send + Sync + 'static {
    const KEY: &'static str;
    type Response: Serialize + DeserializeOwned;
}
pub trait Event: Serialize + DeserializeOwned + 'static {
    const KEY: &'static str;
}
type TypedOperation<R> =
    dyn Fn(&mut Transaction<'_>, R) -> Result<<R as Request>::Response> + Send + Sync;
pub(crate) struct Typed<R: Request> {
    pub owner: String,
    pub run: Box<TypedOperation<R>>,
}
pub(crate) type Stored = Arc<dyn Any + Send + Sync>;
type Handler = Arc<dyn Fn(&mut Transaction<'_>, Value) -> Result<Value> + Send + Sync>;
type Listener = Arc<dyn Fn(&mut Transaction<'_>, Value) -> Result<()> + Send + Sync>;
type Guard = Box<dyn Fn(&Host, &Envelope) -> Result<()> + Send + Sync>;
type Stopper = Box<dyn Fn(&Host) -> Result<Option<String>> + Send + Sync>;
pub(crate) struct Codec {
    pub key: &'static str,
    pub owner: String,
    pub copy: fn(&Stored) -> Box<dyn Any + Send + Sync>,
    pub copy_borrowed: fn(&(dyn Any + Send + Sync)) -> Box<dyn Any + Send + Sync>,
    pub encode: fn(&Stored) -> Result<Box<serde_json::value::RawValue>>,
    pub decode: fn(&str) -> Result<Stored>,
    pub ids: fn(&Stored) -> Vec<u64>,
    pub validate: fn(&Stored) -> Result<()>,
    pub validate_borrowed: fn(&(dyn Any + Send + Sync), &Stored) -> Result<()>,
}
#[derive(Default)]
pub(crate) struct Registry {
    pub resources: BTreeMap<TypeId, Codec>,
    pub handlers: BTreeMap<String, Handler>,
    pub typed: BTreeMap<TypeId, Stored>,
    pub request_owners: BTreeMap<String, String>,
    pub listeners: BTreeMap<String, BTreeMap<String, Listener>>,
    pub save_checks: Vec<fn(&Host) -> Result<()>>,
    pub guards: Vec<Guard>,
    pub stops: Vec<Stopper>,
    pub plugins: BTreeMap<String, u32>,
    pub before: BTreeMap<String, BTreeMap<String, Listener>>,
    pub decoders: BTreeMap<String, fn(Value) -> Result<()>>,
}
pub struct Builder {
    pub(crate) registry: Registry,
    owner: String,
    data: BTreeMap<TypeId, Stored>,
}
impl Builder {
    pub fn new() -> Self {
        let mut b = Self {
            registry: Registry::default(),
            owner: "sim".into(),
            data: BTreeMap::new(),
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
        self.owner = name.into();
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
                owner: self.owner.clone(),
                copy: |s| Box::new(s.downcast_ref::<T>().expect("registered type").clone()),
                copy_borrowed: |s| {
                    Box::new(s.downcast_ref::<T>().expect("registered type").clone())
                },
                encode: |s| {
                    serde_json::value::to_raw_value(s.downcast_ref::<T>().expect("registered type"))
                        .map_err(json_error)
                },
                decode: |v| {
                    let x: T = serde_json::from_str(v).map_err(json_error)?;
                    x.validate()?;
                    Ok(Arc::new(x))
                },
                ids: |s| {
                    s.downcast_ref::<T>()
                        .expect("registered type")
                        .allocated_ids()
                },
                validate: |s| s.downcast_ref::<T>().expect("registered type").validate(),
                validate_borrowed: |s, old| {
                    s.downcast_ref::<T>()
                        .expect("registered type")
                        .validate_changes(old.downcast_ref::<T>().expect("registered type"))
                },
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
        if self.registry.handlers.contains_key(R::KEY) {
            return Err(Error::Duplicate(R::KEY.into()));
        }
        self.registry.decoders.insert(R::KEY.into(), |value| {
            serde_json::from_value::<R>(value).map_err(json_error)?;
            Ok(())
        });
        self.registry
            .request_owners
            .insert(R::KEY.into(), self.owner.clone());
        let typed = Arc::new(Typed::<R> {
            owner: self.owner.clone(),
            run: Box::new(handler),
        });
        self.registry.typed.insert(TypeId::of::<R>(), typed);
        self.registry.handlers.insert(
            R::KEY.into(),
            Arc::new(move |tx, value| {
                let request: R = serde_json::from_value(value).map_err(json_error)?;
                serde_json::to_value(tx.call(request)?).map_err(json_error)
            }),
        );
        Ok(())
    }
    pub fn subscribe<E: Event>(
        &mut self,
        name: &str,
        listener: impl Fn(&mut Transaction<'_>, E) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        let listeners = self.registry.listeners.entry(E::KEY.into()).or_default();
        if listeners.contains_key(name) {
            return Err(Error::Duplicate(name.into()));
        }
        let owner = self.owner.clone();
        listeners.insert(
            name.into(),
            Arc::new(move |tx, value| {
                tx.with_owner(&owner, |tx| {
                    listener(tx, serde_json::from_value(value).map_err(json_error)?)
                })
            }),
        );
        Ok(())
    }
    pub fn before<R: Request>(
        &mut self,
        name: &str,
        check: impl Fn(&mut Transaction<'_>, R) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        let checks = self.registry.before.entry(R::KEY.into()).or_default();
        if checks.contains_key(name) {
            return Err(Error::Duplicate(name.into()));
        }
        let owner = self.owner.clone();
        checks.insert(
            name.into(),
            Arc::new(move |tx, value| {
                tx.with_owner(&owner, |tx| {
                    check(tx, serde_json::from_value(value).map_err(json_error)?)
                })
            }),
        );
        Ok(())
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

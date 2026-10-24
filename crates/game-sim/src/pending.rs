//! Typed runtime requests. JSON envelopes exist only at persistence boundaries.
use crate::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
pub(crate) enum Input<'a> {
    Value(Value),
    Json(&'a str),
}
impl Input<'_> {
    pub fn decode<R: Request>(self) -> Result<Pending> {
        let request = match self {
            Self::Value(value) => serde_json::from_value(value),
            Self::Json(value) => serde_json::from_str(value),
        }
        .map_err(registry::json_error)?;
        Pending::new::<R>(request)
    }
}
trait Payload: Send + Sync + std::any::Any {
    fn payload(&self) -> &dyn erased_serde::Serialize;
    fn key(&self) -> &str;
    fn encode(&self) -> Result<Envelope> {
        Ok(Envelope {
            key: self.key().into(),
            payload: erased_serde::serialize(self.payload(), serde_json::value::Serializer)
                .map_err(registry::json_error)?,
        })
    }
    fn invoke(&self, tx: &mut Transaction<'_>) -> Result<Value>;
    fn bound(&self) -> bool {
        true
    }
    fn bind(&self, registry: &Registry) -> Result<Pending> {
        registry.bind(self.encode()?)
    }
}
impl<R: Request> Payload for R {
    fn payload(&self) -> &dyn erased_serde::Serialize {
        self
    }
    fn key(&self) -> &str {
        R::KEY
    }
    fn invoke(&self, tx: &mut Transaction<'_>) -> Result<Value> {
        serde_json::to_value(tx.call(self.clone())?).map_err(registry::json_error)
    }
}
#[derive(Deserialize)]
struct Unbound {
    key: String,
    payload: Box<serde_json::value::RawValue>,
}
impl Payload for Unbound {
    fn payload(&self) -> &dyn erased_serde::Serialize {
        &self.payload
    }
    fn key(&self) -> &str {
        &self.key
    }
    fn invoke(&self, _: &mut Transaction<'_>) -> Result<Value> {
        Err(Error::InvalidSave("unbound request".into()))
    }
    fn bound(&self) -> bool {
        false
    }
    fn bind(&self, registry: &Registry) -> Result<Pending> {
        registry.bind_payload(&self.key, Input::Json(self.payload.get()))
    }
}
/// Immutable typed work, bound on load and invoked through `Transaction::call`.
#[derive(Clone)]
pub struct Pending(Arc<dyn Payload>);
impl Pending {
    pub fn new<R: Request>(request: R) -> Result<Self> {
        // Reject non-JSON requests before committing, without retaining a JSON tree.
        serde_json::to_writer(std::io::sink(), &request).map_err(registry::json_error)?;
        Ok(Self::immediate(request))
    }
    /// Temporary typed work; retained work must pass new() to validate nested payloads.
    pub fn immediate<R: Request>(request: R) -> Self {
        Self(Arc::new(request))
    }
    /// Inspect a bound payload without serialization or executing its handler.
    pub fn request<R: Request>(&self) -> Option<&R> {
        (self.0.as_ref() as &dyn std::any::Any).downcast_ref()
    }
    pub fn key(&self) -> &str {
        self.0.key()
    }
    pub(crate) fn bound(&self) -> bool {
        self.0.bound()
    }
    pub(crate) fn bind(&mut self, registry: &Registry) -> Result<()> {
        if !self.bound() {
            *self = self.0.bind(registry)?;
        }
        Ok(())
    }
    pub(crate) fn invoke(&self, tx: &mut Transaction<'_>) -> Result<Value> {
        self.0.invoke(tx)
    }
}
impl Serialize for Pending {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            key: &'a str,
            payload: &'a dyn erased_serde::Serialize,
        }
        Wire {
            key: self.key(),
            payload: self.0.payload(),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Pending {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(Self(Arc::new(Unbound::deserialize(deserializer)?)))
    }
}
impl fmt::Debug for Pending {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.encode().fmt(f)
    }
}
impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || matches!((self.0.encode(), other.0.encode()), (Ok(a), Ok(b)) if a == b)
    }
}
impl Eq for Pending {}

//! Typed runtime requests. JSON envelopes exist only at persistence boundaries.
use crate::*;
use serde::{Deserializer, Serializer};
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
trait Payload: Send + Sync {
    fn key(&self) -> &str;
    fn encode(&self) -> Result<Envelope>;
    fn invoke(&self, tx: &mut Transaction<'_>) -> Result<Value>;
    fn bound(&self) -> bool {
        true
    }
    fn bind(&self, registry: &Registry) -> Result<Pending> {
        registry.bind(self.encode()?)
    }
}
impl<R: Request> Payload for R {
    fn key(&self) -> &str {
        R::KEY
    }
    fn encode(&self) -> Result<Envelope> {
        Ok(Envelope {
            key: R::KEY.into(),
            payload: serde_json::to_value(self).map_err(registry::json_error)?,
        })
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
    fn key(&self) -> &str {
        &self.key
    }
    fn encode(&self) -> Result<Envelope> {
        Ok(Envelope {
            key: self.key.clone(),
            payload: serde_json::from_str(self.payload.get()).map_err(registry::json_error)?,
        })
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
/// Immutable, cheaply cloned pending work. Loading binds its envelope through
/// the feature registry before exposing the host. It always uses `Transaction::call`.
#[derive(Clone)]
pub struct Pending(Arc<dyn Payload>);
impl Pending {
    pub(crate) fn new<R: Request>(request: R) -> Result<Self> {
        // Reject non-JSON requests before committing, without retaining a JSON tree.
        serde_json::to_writer(std::io::sink(), &request).map_err(registry::json_error)?;
        Ok(Self(Arc::new(request)))
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
        self.0
            .encode()
            .map_err(serde::ser::Error::custom)?
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

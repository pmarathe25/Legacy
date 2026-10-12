//! Control and pending consent belong to the invoking person's native row.
use crate::{Activation, Active, ControlState, Warning};
use game_core::{Error, PersonId};
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSession {
    pub actor: PersonId,
    pub active: Option<Active>,
    pub activation: Option<Activation>,
    pub warning: Option<Warning>,
}
impl ControlSession {
    pub fn validate(&self) -> Result<(), Error> {
        if self.actor.0 == 0
            || self.active.is_none() && self.activation.is_none() && self.warning.is_none()
            || self.active.as_ref().is_some_and(|active| {
                active.actor != self.actor || active.lives.is_empty() || active.lives.len() > 8
            })
            || self.activation.as_ref().is_some_and(|activation| {
                activation.actor != self.actor
                    || activation.lives.is_empty()
                    || activation.lives.len() > 8
            })
            || self
                .warning
                .as_ref()
                .is_some_and(|warning| warning.id.0 == 0)
        {
            return Err(Error::InvalidSave(
                "invalid invocation control session".into(),
            ));
        }
        Ok(())
    }
}
pub fn register_session(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<ControlSession>("invocation/session", ControlSession::validate)
}
pub fn session(store: &Store, actor: PersonId) -> Option<&ControlSession> {
    store.get(StableId(actor.0))
}
pub fn session_mut(store: &mut Store, actor: PersonId) -> Option<&mut ControlSession> {
    store.get_mut(StableId(actor.0))
}
pub fn sessions(store: &Store) -> impl Iterator<Item = (PersonId, &ControlSession)> {
    store
        .iter::<ControlSession>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub(crate) fn one_session(store: &Store) -> Result<Option<(PersonId, &ControlSession)>, Error> {
    let mut rows = sessions(store);
    let first = rows.next();
    if rows.next().is_some() {
        return Err(Error::InvalidConfiguration(
            "more than one invoking person".into(),
        ));
    }
    if let Some((actor, row)) = first {
        row.validate()?;
        if row.actor != actor {
            return Err(Error::InvalidConfiguration(
                "control session identity mismatch".into(),
            ));
        }
        if row.activation.is_some()
            != row
                .warning
                .as_ref()
                .is_some_and(|pending| pending.activation.is_some())
            || row.warning.as_ref().is_some_and(|pending| {
                crate::warning(store, pending.id).is_none_or(|warning| {
                    warning.actor != actor
                        || match &pending.activation {
                            Some(lives) => row.activation.as_ref().is_none_or(|activation| {
                                &activation.lives != lives || warning.lives != *lives
                            }),
                            None => row
                                .active
                                .as_ref()
                                .is_none_or(|active| warning.lives != active.lives),
                        }
                })
            })
        {
            return Err(Error::InvalidConfiguration(
                "control session warning reference mismatch".into(),
            ));
        }
    }
    Ok(first)
}
impl ControlState {
    pub fn active<'a>(&self, store: &'a Store) -> Option<&'a Active> {
        one_session(store).ok().flatten()?.1.active.as_ref()
    }
    pub fn activation<'a>(&self, store: &'a Store) -> Option<&'a Activation> {
        one_session(store).ok().flatten()?.1.activation.as_ref()
    }
    pub fn pending_consent<'a>(&self, store: &'a Store) -> Option<&'a Warning> {
        one_session(store).ok().flatten()?.1.warning.as_ref()
    }
}

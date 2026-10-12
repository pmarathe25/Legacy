//! Action payloads are opaque; characters own only the surrounding mind data.
use super::*;
use game_ecs::{Schema, StableId};

impl<A> Mind<A> {
    /// Intrinsic scalar and local row invariants. Current clocks, catalogs and
    /// executable action meaning are supplied by the composing owner.
    pub fn validate(&self) -> Result<(), Error> {
        if !self.personality.valid()
            || self.interests.values().any(|interest| *interest > 1_000)
            || self.values.values().any(|value| !(0..=1_000).contains(&value.weight))
            || self.routes.values().any(|links| links.values().any(|duration| duration.0 == 0))
            || self.sites.iter().any(|(id, site)| *id != site.site)
            || self.plan.as_ref().is_some_and(|plan| {
                plan.delivery.as_ref().is_some_and(|delivery| !delivery.valid())
                    || plan.motives.iter().any(|motive| {
                        matches!(motive, Motive::Provide { requirement, .. } if requirement.quantity.0 == 0)
                    })
            })
        {
            return Err(Error::InvalidConfiguration("invalid personal mind component".into()));
        }
        Ok(())
    }
}

pub fn register_minds<A: Component>(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Stocks>("characters/stocks", Stocks::validate)?;
    schema.register::<Mind<A>>("characters/mind", Mind::<A>::validate)
}
pub fn mind<A: Component>(store: &Store, actor: PersonId) -> Option<&Mind<A>> {
    store.get(StableId(actor.0))
}
pub fn mind_mut<A: Component>(store: &mut Store, actor: PersonId) -> Option<&mut Mind<A>> {
    store.get_mut(StableId(actor.0))
}
pub fn minds<A: Component>(store: &Store) -> impl Iterator<Item = (PersonId, &Mind<A>)> {
    store
        .iter::<Mind<A>>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_mind<A: Component>(
    store: &mut Store,
    actor: PersonId,
    row: Mind<A>,
) -> Result<(), Error> {
    row.validate()?;
    let stocks = stocks(store, actor).cloned().unwrap_or_default();
    store.insert_pair_or_spawn(StableId(actor.0), row, stocks)
}
pub fn remove_mind<A: Component>(
    store: &mut Store,
    actor: PersonId,
) -> Result<Option<Mind<A>>, Error> {
    if !store.contains(StableId(actor.0)) {
        return Ok(None);
    }
    store.remove(StableId(actor.0))
}

/// The character reacts to newly learned failures according to their own honesty.
pub fn learn_defaults<A: Component>(store: &mut Store, observer: PersonId, debtors: &[PersonId]) {
    let Some(mind) = mind_mut::<A>(store, observer) else {
        return;
    };
    for debtor in debtors {
        let loss = i32::from(mind.personality.honesty) / 100;
        let relationship = mind.relationships.entry(*debtor).or_insert(Relationship {
            trust: 0,
            affection: 0,
        });
        relationship.trust = (relationship.trust - loss).max(-1_000);
    }
}

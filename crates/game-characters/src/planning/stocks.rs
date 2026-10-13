//! Dated stock observations are a distinct canonical character component.
use super::*;
use game_ecs::{Batch, BatchKey, StableId};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stocks {
    pub known: BTreeMap<InventoryId, StockKnowledge>,
}
impl Stocks {
    pub fn learn_memory_stock(
        &mut self,
        now: SimTime,
        mut fact: StockKnowledge,
        source: KnowledgeSource,
    ) -> bool {
        if self
            .known
            .get(&fact.inventory)
            .is_some_and(|old| old.observed_at > fact.observed_at)
        {
            return false;
        }
        fact.accessible = self
            .known
            .get(&fact.inventory)
            .is_some_and(|old| old.accessible);
        fact.learned_at = now;
        fact.source = source;
        self.known.insert(fact.inventory, fact);
        true
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.known.iter().any(|(id, k)| {
            id.0 == 0
                || *id != k.inventory
                || k.area.0 == 0
                || k.observed_at > k.learned_at
                || match k.source {
                    KnowledgeSource::Observed(id) | KnowledgeSource::Reported(id) => id.0 == 0,
                    KnowledgeSource::Recorded(id) => id.0 == 0,
                }
        }) {
            Err(Error::InvalidConfiguration(
                "invalid personal stock knowledge".into(),
            ))
        } else {
            Ok(())
        }
    }
}
pub fn stocks(store: &Store, actor: PersonId) -> Option<&Stocks> {
    store.get(StableId(actor.0))
}
pub fn stocks_mut(store: &mut Store, actor: PersonId) -> Option<&mut Stocks> {
    store.get_mut(StableId(actor.0))
}
pub fn stock_rows(store: &Store) -> impl Iterator<Item = (PersonId, &Stocks)> {
    store
        .iter::<Stocks>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_stocks(store: &mut Store, actor: PersonId, row: Stocks) -> Result<(), Error> {
    store.insert_or_spawn(StableId(actor.0), row)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedStockReports<A: Component> {
    marker: std::marker::PhantomData<A>,
    actor: PersonId,
    before: Stocks,
    after: Stocks,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StocksBatchGuard<A: Component> {
    marker: std::marker::PhantomData<A>,
    key: BatchKey,
    actor: PersonId,
}
impl<A: Component> StocksBatchGuard<A> {
    pub fn validate(&self, batch: &Batch) -> Result<(), Error> {
        if self.key != batch.key()
            || batch.touches::<Mind<A>>(StableId(self.actor.0))
            || batch
                .edited_ids::<Stocks>()
                .filter(|id| id.0 == self.actor.0)
                .collect::<Vec<_>>()
                != vec![StableId(self.actor.0)]
        {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
/// Institutional reports carry stock evidence only. Unlike a social conversation
/// they do not change trust or transfer a sender's permission to use the stock.
pub fn prepare_stock_reports<A: Component>(
    store: &Store,
    actor: PersonId,
    sender: PersonId,
    now: SimTime,
    reports: &[StockKnowledge],
) -> Result<PreparedStockReports<A>, Error> {
    mind::<A>(store, actor).ok_or(Error::Unavailable)?;
    let before = stocks(store, actor).cloned().ok_or(Error::Unavailable)?;
    let mut after = before.clone();
    for fact in reports {
        if after
            .known
            .get(&fact.inventory)
            .is_none_or(|old| old.observed_at < fact.observed_at)
        {
            let mut fact = fact.clone();
            fact.accessible = after
                .known
                .get(&fact.inventory)
                .is_some_and(|old| old.accessible);
            fact.learned_at = now;
            fact.source = KnowledgeSource::Reported(sender);
            after.known.insert(fact.inventory, fact);
        }
    }
    after.validate()?;
    Ok(PreparedStockReports {
        marker: std::marker::PhantomData,
        actor,
        before,
        after,
    })
}
impl<A: Component> PreparedStockReports<A> {
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<StocksBatchGuard<A>, Error> {
        if mind::<A>(store, self.actor).is_none()
            || stocks(store, self.actor) != Some(&self.before)
            || batch.touches::<Stocks>(StableId(self.actor.0))
        {
            return Err(Error::Unavailable);
        }
        batch.insert(StableId(self.actor.0), self.after);
        let guard = StocksBatchGuard {
            marker: std::marker::PhantomData,
            key: batch.key(),
            actor: self.actor,
        };
        guard.validate(batch)?;
        Ok(guard)
    }
}

/// Receiving Characters own both the planning consequence and local stock memory.
pub fn observe_stock<A: Component>(
    store: &mut Store,
    actor: PersonId,
    now: SimTime,
    here: AreaId,
    meal_size: Quantity,
    fact: StockKnowledge,
) -> Result<(), Error> {
    let (mind, stocks) = store
        .get_pair_mut::<Mind<A>, Stocks>(StableId(actor.0))
        .ok_or(Error::Unavailable)?;
    mind.observe_stock(stocks, now, here, meal_size, fact);
    Ok(())
}
pub fn receive_social_report<A: Component>(
    store: &mut Store,
    actor: PersonId,
    sender: PersonId,
    now: SimTime,
    fact: StockKnowledge,
) -> bool {
    let Some((mind, stocks)) = store.get_pair_mut::<Mind<A>, Stocks>(StableId(actor.0)) else {
        return false;
    };
    mind.receive_report(stocks, sender, now, fact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_ecs::Schema;
    fn fixture() -> Store {
        let mut schema = Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for actor in [PersonId(1), PersonId(2)] {
            insert_mind(&mut store, actor, super::super::tests::mind()).unwrap();
        }
        store
    }
    fn fact(at: u64, accessible: bool) -> StockKnowledge {
        StockKnowledge {
            inventory: InventoryId(10),
            area: AreaId(1),
            good: Good::Food,
            quantity: Quantity(at),
            accessible,
            observed_at: SimTime(at),
            learned_at: SimTime(at),
            source: KnowledgeSource::Observed(PersonId(1)),
        }
    }
    #[test]
    fn institutional_reports_preserve_rights_dates_and_mind_without_adding_trust() {
        let mut store = fixture();
        stocks_mut(&mut store, PersonId(1))
            .unwrap()
            .known
            .insert(InventoryId(10), fact(4, true));
        let mind_before = mind::<u8>(&store, PersonId(1)).unwrap().clone();
        let reports = prepare_stock_reports::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(8),
            &[fact(3, false), fact(4, false), fact(6, false)],
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = reports.stage_into(&store, &mut batch).unwrap();
        guard.validate(&batch).unwrap();
        store.apply_batch(batch).unwrap();
        let known = &stocks(&store, PersonId(1)).unwrap().known[&InventoryId(10)];
        assert_eq!(known.observed_at, SimTime(6));
        assert_eq!(known.learned_at, SimTime(8));
        assert_eq!(known.source, KnowledgeSource::Reported(PersonId(2)));
        assert!(known.accessible);
        assert_eq!(mind::<u8>(&store, PersonId(1)), Some(&mind_before));
        let before = store.snapshot().unwrap();
        assert!(
            prepare_stock_reports::<u8>(
                &store,
                PersonId(1),
                PersonId(2),
                SimTime(8),
                &[fact(9, true)]
            )
            .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn receiving_stock_guards_bind_reads_recipient_footprint_and_batch_identity() {
        let mut store = fixture();
        let plan = prepare_stock_reports::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(3),
            &[fact(2, true)],
        )
        .unwrap();
        stocks_mut(&mut store, PersonId(1))
            .unwrap()
            .known
            .insert(InventoryId(10), fact(1, false));
        let before = store.snapshot().unwrap();
        assert!(plan.stage_into(&store, &mut Batch::new()).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        let plan = prepare_stock_reports::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(3),
            &[fact(2, true)],
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = plan.stage_into(&store, &mut batch).unwrap();
        batch.insert(StableId(2), Stocks::default());
        guard.validate(&batch).unwrap();
        assert!(guard.validate(&Batch::new()).is_err());
        batch.remove::<Mind<u8>>(StableId(1));
        assert!(guard.validate(&batch).is_err());
        let plan = prepare_stock_reports::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
            SimTime(3),
            &[fact(2, true)],
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = plan.stage_into(&store, &mut batch).unwrap();
        batch.remove::<Stocks>(StableId(1));
        assert!(guard.validate(&batch).is_err());
        assert!(store.apply_batch(batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn native_stock_schema_rejects_unknown_fields_bad_dates_and_identity() {
        let store = fixture();
        for bad in [
            r#"{"known":{"10":{"inventory":11,"area":1,"good":"Food","quantity":1,"accessible":false,"observed_at":1,"learned_at":1,"source":{"Observed":1}}}}"#,
            r#"{"known":{"10":{"inventory":10,"area":1,"good":"Food","quantity":1,"accessible":false,"observed_at":3,"learned_at":2,"source":{"Observed":1}}}}"#,
            r#"{"known":{},"shadow":{}}"#,
        ] {
            let mut snapshot = store.snapshot().unwrap();
            snapshot.columns.get_mut("characters/stocks").unwrap()[0].value =
                serde_json::value::RawValue::from_string(bad.into()).unwrap();
            let mut schema = Schema::new();
            register_minds::<u8>(&mut schema).unwrap();
            assert!(Store::restore(schema, snapshot).is_err());
        }
    }
}

/// Access reports update remembered rights only when the inventory was already
/// known. The report does not fabricate a stock, quantity, or relationship.
pub fn prepare_permission_memory<A: Component>(
    store: &Store,
    recipient: PersonId,
    inventory: InventoryId,
    allowed: bool,
    sender: PersonId,
    now: SimTime,
) -> Result<PreparedStockReports<A>, Error> {
    prepare_permission_memories::<A>(store, recipient, &[(inventory, allowed, sender)], now)
}
pub fn prepare_permission_memories<A: Component>(
    store: &Store,
    recipient: PersonId,
    reports: &[(InventoryId, bool, PersonId)],
    now: SimTime,
) -> Result<PreparedStockReports<A>, Error> {
    let before = stocks(store, recipient)
        .cloned()
        .ok_or(Error::Unavailable)?;
    let mut after = before.clone();
    for (inventory, allowed, sender) in reports {
        if let Some(fact) = after.known.get_mut(inventory) {
            fact.accessible = *allowed;
            fact.learned_at = now;
            fact.source = KnowledgeSource::Reported(*sender);
        }
    }
    after.validate()?;
    Ok(PreparedStockReports {
        marker: std::marker::PhantomData,
        actor: recipient,
        before,
        after,
    })
}

#[cfg(test)]
mod permission_tests {
    use super::*;
    #[test]
    fn permission_reports_change_only_existing_memories_and_stage_once_per_recipient() {
        let mut schema = game_ecs::Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_mind(&mut store, PersonId(1), crate::planning::tests::mind()).unwrap();
        let fact = StockKnowledge {
            inventory: InventoryId(10),
            area: AreaId(1),
            good: Good::Food,
            quantity: Quantity(1000),
            accessible: false,
            observed_at: SimTime(100),
            learned_at: SimTime(100),
            source: KnowledgeSource::Observed(PersonId(1)),
        };
        stocks_mut(&mut store, PersonId(1))
            .unwrap()
            .known
            .insert(fact.inventory, fact.clone());
        let prepared = prepare_permission_memories::<u8>(
            &store,
            PersonId(1),
            &[
                (InventoryId(10), true, PersonId(2)),
                (InventoryId(11), true, PersonId(2)),
            ],
            SimTime(200),
        )
        .unwrap();
        let mut batch = Batch::new();
        let guard = prepared.stage_into(&store, &mut batch).unwrap();
        guard.validate(&batch).unwrap();
        store.apply_batch(batch).unwrap();
        let stocks = stocks(&store, PersonId(1)).unwrap();
        assert_eq!(stocks.known.len(), 1);
        let received = &stocks.known[&InventoryId(10)];
        assert!(received.accessible);
        assert_eq!(received.quantity, fact.quantity);
        assert_eq!(received.observed_at, fact.observed_at);
        assert_eq!(received.learned_at, SimTime(200));
        assert_eq!(received.source, KnowledgeSource::Reported(PersonId(2)));
        let prepared = prepare_permission_memory::<u8>(
            &store,
            PersonId(1),
            InventoryId(10),
            false,
            PersonId(2),
            SimTime(201),
        )
        .unwrap();
        stocks_mut(&mut store, PersonId(1))
            .unwrap()
            .known
            .get_mut(&InventoryId(10))
            .unwrap()
            .quantity = Quantity(999);
        let before = store.clone();
        assert!(prepared.stage_into(&store, &mut Batch::new()).is_err());
        assert_eq!(store, before);
    }
}

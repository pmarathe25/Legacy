//! Sequential incoming cargo admission shares each connected body's capacity.
use crate::*;

pub struct IncomingCargo<'a, F> {
    store: &'a game_ecs::Store,
    facts: F,
    enabled: bool,
    received: BTreeMap<PersonId, Quantity>,
}
impl<'a, F: BodyFacts> IncomingCargo<'a, F> {
    pub fn new(store: &'a game_ecs::Store, facts: F, runtime: Option<&Runtime>) -> Self {
        Self {
            store,
            facts,
            enabled: runtime.is_some(),
            received: BTreeMap::new(),
        }
    }
    pub fn admit(&mut self, holder: Option<PersonId>, quantity: Quantity) -> bool {
        if !self.enabled || groups(self.store).next().is_none() {
            return true;
        }
        let Some(holder) = holder else {
            return true;
        };
        if !transfer_capacity(self.store, self, holder, quantity).unwrap_or(false) {
            return false;
        }
        let received = self.received.entry(holder).or_default();
        let Some(quantity) = received.0.checked_add(quantity.0) else {
            return false;
        };
        *received = Quantity(quantity);
        true
    }
}
impl<F: BodyFacts> BodyFacts for IncomingCargo<'_, F> {
    fn body(&self, actor: PersonId) -> Option<Body> {
        let mut body = self.facts.body(actor)?;
        body.cargo.0 = body
            .cargo
            .0
            .checked_add(self.received.get(&actor).map_or(0, |quantity| quantity.0))?;
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incoming_prefix_accounts_for_both_bearers_and_denial_is_passive() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        insert_group(
            &mut store,
            PersonId(3),
            CarryGroup {
                passenger: PersonId(3),
                bearers: BTreeSet::from([PersonId(1), PersonId(2)]),
                anchor: PersonId(1),
                authority: Authority::Consented,
                accepted_at: SimTime(0),
            },
        )
        .unwrap();
        struct Facts {
            reads: std::cell::RefCell<BTreeSet<PersonId>>,
        }
        impl BodyFacts for Facts {
            fn body(&self, actor: PersonId) -> Option<Body> {
                self.reads.borrow_mut().insert(actor);
                (actor.0 <= 3).then_some(Body {
                    position: Position::Area(AreaId(10)),
                    mass: Quantity(6_000),
                    cargo: Quantity(0),
                    capacity: Quantity(10_000),
                    can_hold: true,
                    can_walk: true,
                    can_bear: true,
                })
            }
        }
        let facts = Facts {
            reads: Default::default(),
        };
        let runtime = Runtime::new(Rules::default()).unwrap();
        let before = store.snapshot().unwrap();
        let mut incoming = IncomingCargo::new(&store, &facts, Some(&runtime));
        assert!(incoming.admit(Some(PersonId(1)), Quantity(10_000)));
        assert!(!incoming.admit(Some(PersonId(2)), Quantity(10_000)));
        assert!(incoming.admit(Some(PersonId(2)), Quantity(4_000)));
        assert!(!incoming.admit(Some(PersonId(1)), Quantity(1)));
        assert_eq!(
            *facts.reads.borrow(),
            BTreeSet::from([PersonId(1), PersonId(2), PersonId(3)])
        );
        assert_eq!(store.snapshot().unwrap(), before);
        facts.reads.borrow_mut().clear();
        let mut disabled = IncomingCargo::new(&store, &facts, None);
        assert!(disabled.admit(Some(PersonId(99)), Quantity(u64::MAX)));
        assert!(facts.reads.borrow().is_empty());
    }
}

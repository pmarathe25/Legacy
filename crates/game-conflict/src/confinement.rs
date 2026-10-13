//! Prepared physical confinement, shared by an initiative capture and a timed
//! restraint. Entry points supply actual body/contact/surrender facts and scene
//! identity; global ID allocation, scheduling and publication remain outside.
//! Applied rope is forced placement, not a voluntary cargo handoff: the captive
//! need not accept it, and the additional burden may exceed carrying limits.
use crate::{Captivity, Rules};
use game_cargo::{Inventory, InventoryLocation, inventory};
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfinementFacts {
    pub captor: PersonId,
    pub target: PersonId,
    pub captor_alive: bool,
    pub captor_health: u64,
    pub independent: bool,
    pub captor_confined: bool,
    pub target_alive: bool,
    pub target_health: u64,
    pub target_confined: bool,
    /// Current surrender/yield toward this captor, rather than a past promise.
    pub surrendered_to_captor: bool,
    /// Actual reach after any proposed initiative movement, or exact physical
    /// contact at a stationary area/road point. No remote reported position.
    pub contact: bool,
    /// Both bodies remain at the admission site, using transport canonical keys.
    pub admitted_site_contact: bool,
    pub captor_location: Location,
    pub target_location: Location,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfinementMode {
    Tactical {
        encounter: EncounterId,
    },
    Timed {
        completes: SimTime,
        admitted_location: Location,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfinementContext {
    pub map: CombatMapId,
    pub area: AreaId,
    pub location: Location,
    pub target_position: Point,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfinementReceipt {
    pub captor: PersonId,
    pub target: PersonId,
    pub source: InventoryId,
    pub binding: InventoryId,
    pub quantity: Quantity,
    pub mode: ConfinementMode,
    /// A tactical captive remains a passive encounter body. A timed restraint
    /// interrupts the target's ordinary activity only after successful binding.
    pub interrupt_target: bool,
}
impl ConfinementReceipt {
    pub fn transport_burden(&self) -> Option<game_transport::ConfinementBurden> {
        matches!(self.mode, ConfinementMode::Timed { .. }).then_some(
            game_transport::ConfinementBurden {
                captor: self.captor,
                target: self.target,
                quantity: self.quantity,
            },
        )
    }
}
#[derive(Clone, Debug)]
pub struct PreparedConfinement {
    facts: ConfinementFacts,
    source: Inventory,
    captivity: Captivity,
    receipt: ConfinementReceipt,
    context: ConfinementContext,
}
/// Bounded owner decision over actual facts. No recipient-consent or incoming
/// room fact is accepted here: neither controls a forced binding's feasibility.
pub fn prepare_confinement(
    rules: &Rules,
    facts: ConfinementFacts,
    source: &Inventory,
    applied_binding: InventoryId,
    context: ConfinementContext,
    mode: ConfinementMode,
    now: SimTime,
) -> Result<PreparedConfinement, Error> {
    rules.validate()?;
    let place_matches = |place: &Location| match (&context.location, place) {
        (Location::AtArea(a), Location::AtArea(b)) => a == b,
        // Contact is the transport owner's canonical exact-point fact; this
        // permits reversed representations of the same actual road position.
        (
            Location::StoppedOnRoad { from: a, to: b, .. },
            Location::StoppedOnRoad { from: c, to: d, .. },
        ) => (a == c && b == d) || (a == d && b == c),
        _ => false,
    };
    let geography_valid = match context.location {
        Location::AtArea(area) => context.area == area,
        Location::StoppedOnRoad { from, to, .. } => context.area == from || context.area == to,
        _ => false,
    };
    let mode_valid = match &mode {
        ConfinementMode::Tactical { encounter } => encounter.0 > 0,
        ConfinementMode::Timed {
            completes,
            admitted_location: _,
        } => *completes <= now && facts.admitted_site_contact,
    };
    if facts.captor.0 == 0
        || facts.target.0 == 0
        || facts.captor == facts.target
        || !facts.captor_alive
        || facts.captor_health <= rules.incapacitation
        || !facts.independent
        || facts.captor_confined
        || !facts.target_alive
        || facts.target_confined
        || !facts.contact
        || !(facts.surrendered_to_captor || facts.target_health <= rules.incapacitation)
        || !place_matches(&facts.captor_location)
        || !place_matches(&facts.target_location)
        || !geography_valid
        || !mode_valid
        || context.map.0 == 0
        || context.area.0 == 0
        || source.id.0 == 0
        || applied_binding.0 == 0
        || source.id == applied_binding
        || source.good != Good::Binding
        || source.location != InventoryLocation::Carried(facts.captor)
        || source.quantity.0 < 1000
        || source.room().is_err()
    {
        return Err(Error::Unavailable);
    }
    let quantity = Quantity(1000);
    let captivity = Captivity {
        actor: facts.target,
        captor: facts.captor,
        area: context.area,
        position: context.target_position,
        map: context.map,
        binding: applied_binding,
    };
    let receipt = ConfinementReceipt {
        captor: facts.captor,
        target: facts.target,
        source: source.id,
        binding: applied_binding,
        quantity,
        interrupt_target: matches!(mode, ConfinementMode::Timed { .. }),
        mode,
    };
    Ok(PreparedConfinement {
        facts,
        source: source.clone(),
        captivity,
        receipt,
        context,
    })
}
#[derive(Clone, Debug)]
pub struct ValidatedConfinement {
    prepared: PreparedConfinement,
    cargo: game_cargo::PreparedForcedSplit,
}
pub struct ConfinementGuard {
    reads: game_ecs::ReadGuard,
    cargo: game_ecs::ReadGuard,
    receipt: ConfinementReceipt,
    context: ConfinementContext,
}
impl ConfinementGuard {
    pub fn receipt(&self) -> &ConfinementReceipt {
        &self.receipt
    }
    pub fn matches_capture(
        &self,
        captor: PersonId,
        target: PersonId,
        encounter: EncounterId,
        context: &ConfinementContext,
    ) -> bool {
        self.receipt.captor == captor
            && self.receipt.target == target
            && self.receipt.mode == ConfinementMode::Tactical { encounter }
            && &self.context == context
    }
    pub fn validate(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.cargo.validate(store, batch)?;
        self.reads.validate(store, batch)
    }
}
impl PreparedConfinement {
    pub fn receipt(&self) -> &ConfinementReceipt {
        &self.receipt
    }
    /// All scoped guards precede allocation or debit. The coordinator retains
    /// its foreign epoch through global ID reservation and scene-map creation.
    pub fn begin(
        self,
        store: &Store,
        fresh: &ConfinementFacts,
    ) -> Result<ValidatedConfinement, Error> {
        if fresh != &self.facts
            || inventory(store, self.source.id) != Some(&self.source)
            || store.contains(StableId(self.receipt.binding.0))
            || crate::captivity(store, self.facts.target).is_some()
            || crate::captivity(store, self.facts.captor).is_some()
        {
            return Err(Error::Unavailable);
        }
        let cargo = game_cargo::prepare_forced_split(
            store,
            self.receipt.source,
            self.receipt.quantity,
            self.receipt.binding,
            InventoryLocation::Carried(self.receipt.target),
            BTreeSet::from([self.receipt.captor]),
        )?;
        let validated = ValidatedConfinement {
            prepared: self,
            cargo,
        };
        let mut batch = Batch::new();
        let guard = validated.clone().stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.validate_batch(&batch)?;
        Ok(validated)
    }
    pub fn apply(
        self,
        store: &mut Store,
        fresh: &ConfinementFacts,
    ) -> Result<ConfinementReceipt, Error> {
        self.begin(store, fresh)?.apply(store)
    }
}
impl ValidatedConfinement {
    pub fn receipt(&self) -> &ConfinementReceipt {
        &self.prepared.receipt
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<ConfinementGuard, Error> {
        let cargo = self.cargo.stage_into(store, batch)?;
        let plan = self.prepared;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        reads.expect_row::<Captivity>(StableId(plan.facts.target.0), None);
        reads.expect_row::<Captivity>(StableId(plan.facts.captor.0), None);
        reads.validate(store)?;
        if batch.touches::<Captivity>(StableId(plan.facts.target.0)) {
            return Err(Error::Unavailable);
        }
        admitted.protect::<Captivity>(StableId(plan.facts.target.0));
        batch.insert(StableId(plan.facts.target.0), plan.captivity);
        Ok(ConfinementGuard {
            reads: reads.bind(store, batch, &admitted)?,
            cargo,
            receipt: plan.receipt,
            context: plan.context,
        })
    }
    /// The binding is forced placement. No receiving consent, body-room check
    /// or voluntary Load job is involved. The real stock components remain authoritative.
    pub fn apply(self, store: &mut Store) -> Result<ConfinementReceipt, Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(guard.receipt)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn stocks(rows: impl IntoIterator<Item = Inventory>) -> Store {
        let mut schema = game_ecs::Schema::new();
        game_cargo::register_components(&mut schema).unwrap();
        crate::register_components(&mut schema).unwrap();
        schema
            .register::<String>("test/foreign", |_| Ok(()))
            .unwrap();
        let mut store = Store::with_schema(schema);
        for row in rows {
            game_cargo::insert_inventory(&mut store, row).unwrap();
        }
        store
    }
    fn rules() -> Rules {
        Rules {
            encounter_limit: 128,
            round: SimDuration(100),
            movement: 100,
            reach: 100,
            damage: 10,
            defence: 2,
            incapacitation: 10,
        }
    }
    fn facts() -> ConfinementFacts {
        ConfinementFacts {
            captor: PersonId(1),
            target: PersonId(2),
            captor_alive: true,
            captor_health: 100,
            independent: true,
            captor_confined: false,
            target_alive: true,
            target_health: 100,
            target_confined: false,
            surrendered_to_captor: true,
            contact: true,
            admitted_site_contact: true,
            captor_location: Location::AtArea(AreaId(1)),
            target_location: Location::AtArea(AreaId(1)),
        }
    }
    fn source() -> Inventory {
        Inventory {
            id: InventoryId(10),
            location: InventoryLocation::Carried(PersonId(1)),
            quantity: Quantity(2000),
            capacity: Quantity(2000),
            access: BTreeSet::new(),
            last_granted: None,
            good: Good::Binding,
            spoils_at: None,
        }
    }
    fn context() -> ConfinementContext {
        ConfinementContext {
            map: CombatMapId(20),
            area: AreaId(1),
            location: Location::AtArea(AreaId(1)),
            target_position: Point { x: 50, y: 50 },
        }
    }
    fn timed() -> ConfinementMode {
        ConfinementMode::Timed {
            completes: SimTime(100),
            admitted_location: Location::AtArea(AreaId(1)),
        }
    }
    fn prepare(
        f: ConfinementFacts,
        s: &Inventory,
        mode: ConfinementMode,
        now: u64,
    ) -> Result<PreparedConfinement, Error> {
        prepare_confinement(
            &rules(),
            f,
            s,
            InventoryId(11),
            context(),
            mode,
            SimTime(now),
        )
    }
    #[test]
    fn initiative_capture_and_completed_timed_restraint_have_identical_physical_custody_effects() {
        let mut outcomes = Vec::new();
        for mode in [
            ConfinementMode::Tactical {
                encounter: EncounterId(30),
            },
            timed(),
        ] {
            let mut stocks = stocks([source()]);
            let plan = prepare(facts(), &source(), mode, 100).unwrap();
            let receipt = plan.apply(&mut stocks, &facts()).unwrap();
            assert_eq!(
                game_cargo::inventories(&stocks)
                    .map(|(_, s)| s.quantity.0)
                    .sum::<u64>(),
                2000
            );
            assert_eq!(receipt.quantity, Quantity(1000));
            assert_eq!(
                receipt.interrupt_target,
                matches!(receipt.mode, ConfinementMode::Timed { .. })
            );
            outcomes.push(stocks);
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
    #[test]
    fn fresh_surrender_or_actual_incapacity_is_required_at_the_closing_deadline() {
        assert!(prepare(facts(), &source(), timed(), 99).is_err());
        let mut f = facts();
        f.surrendered_to_captor = false;
        assert!(prepare(f.clone(), &source(), timed(), 100).is_err());
        f.target_health = 10;
        assert!(prepare(f.clone(), &source(), timed(), 100).is_ok());
        for f in [
            ConfinementFacts {
                captor_health: 10,
                ..facts()
            },
            ConfinementFacts {
                target_alive: false,
                ..facts()
            },
            ConfinementFacts {
                contact: false,
                ..facts()
            },
            ConfinementFacts {
                target_location: Location::AtArea(AreaId(2)),
                ..facts()
            },
        ] {
            assert!(prepare(f, &source(), timed(), 100).is_err());
        }
    }
    #[test]
    fn stale_binding_facts_or_allocated_id_reject_before_any_stock_or_custody_mutation() {
        for changed in 0..3 {
            let plan = prepare(facts(), &source(), timed(), 100).unwrap();
            let mut stocks = stocks([source()]);
            let mut fresh = facts();
            match changed {
                0 => {
                    game_cargo::inventory_mut(&mut stocks, InventoryId(10))
                        .unwrap()
                        .quantity = Quantity(999)
                }
                1 => {
                    game_cargo::insert_inventory(
                        &mut stocks,
                        Inventory {
                            id: InventoryId(11),
                            ..source()
                        },
                    )
                    .unwrap();
                }
                _ => fresh.surrendered_to_captor = false,
            }
            let before = stocks.clone();
            assert_eq!(plan.apply(&mut stocks, &fresh), Err(Error::Unavailable));
            assert_eq!(stocks, before);
        }
    }
    #[test]
    fn forced_binding_requires_no_recipient_acceptance_and_can_overload_an_existing_body() {
        let mut stocks = stocks([
            source(),
            Inventory {
                id: InventoryId(12),
                location: InventoryLocation::Carried(PersonId(2)),
                quantity: Quantity(u64::MAX),
                capacity: Quantity(u64::MAX),
                access: BTreeSet::from([PersonId(2)]),
                good: Good::Timber,
                ..source()
            },
        ]);
        prepare(facts(), &source(), timed(), 100)
            .unwrap()
            .apply(&mut stocks, &facts())
            .unwrap();
        assert_eq!(
            inventory(&stocks, InventoryId(11)).unwrap().location,
            InventoryLocation::Carried(PersonId(2))
        );
        assert_eq!(
            inventory(&stocks, InventoryId(11)).unwrap().quantity,
            Quantity(1000)
        );
        assert_eq!(
            inventory(&stocks, InventoryId(11)).unwrap().access,
            BTreeSet::from([PersonId(1)])
        );
        assert_eq!(
            inventory(&stocks, InventoryId(12)).unwrap().quantity,
            Quantity(u64::MAX)
        );
    }
    #[test]
    fn timed_binding_uses_canonical_admission_site_contact_and_rejects_departure() {
        let admitted = Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(250),
            duration: SimDuration(1000),
        };
        let mut facts = facts();
        facts.captor_location = Location::StoppedOnRoad {
            from: AreaId(2),
            to: AreaId(1),
            progress: SimDuration(750),
            duration: SimDuration(1000),
        };
        facts.target_location = admitted.clone();
        let context = ConfinementContext {
            location: admitted.clone(),
            ..context()
        };
        let mode = ConfinementMode::Timed {
            completes: SimTime(100),
            admitted_location: admitted,
        };
        assert!(
            prepare_confinement(
                &rules(),
                facts.clone(),
                &source(),
                InventoryId(11),
                context.clone(),
                mode.clone(),
                SimTime(100)
            )
            .is_ok()
        );
        facts.admitted_site_contact = false;
        assert!(
            prepare_confinement(
                &rules(),
                facts,
                &source(),
                InventoryId(11),
                context,
                mode,
                SimTime(100)
            )
            .is_err()
        );
    }
    #[test]
    fn schema_rejected_binding_and_foreign_entity_id_are_inert() {
        let mut schema = game_ecs::Schema::new();
        schema
            .register::<Inventory>("cargo/inventory", |row| {
                row.validate()?;
                if row.id == InventoryId(11) {
                    return Err(Error::Unavailable);
                }
                Ok(())
            })
            .unwrap();
        let mut store = Store::with_schema(schema);
        game_cargo::insert_inventory(&mut store, source()).unwrap();
        let before = store.clone();
        assert_eq!(
            prepare(facts(), &source(), timed(), 100)
                .unwrap()
                .apply(&mut store, &facts()),
            Err(Error::Unavailable)
        );
        assert_eq!(store, before);
        assert!(crate::captivities(&store).next().is_none());
        let mut store = stocks([source()]);
        store
            .insert_or_spawn(StableId(11), "foreign".to_string())
            .unwrap();
        let before = store.clone();
        assert_eq!(
            prepare(facts(), &source(), timed(), 100)
                .unwrap()
                .apply(&mut store, &facts()),
            Err(Error::Unavailable)
        );
        assert_eq!(store, before);
        assert!(crate::captivities(&store).next().is_none());
    }
}

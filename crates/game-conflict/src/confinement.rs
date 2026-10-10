//! Prepared physical confinement, shared by an initiative capture and a timed
//! restraint. Entry points supply actual body/contact/surrender facts and scene
//! identity; global ID allocation, scheduling and publication remain outside.
//! Applied rope is forced placement, not a voluntary cargo handoff: the captive
//! need not accept it, and the additional burden may exceed carrying limits.
use crate::{Captivity, Rules};
use game_cargo::{Inventory, InventoryLocation};
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

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
#[derive(Debug)]
pub struct PreparedConfinement {
    facts: ConfinementFacts,
    source: Inventory,
    source_after: Quantity,
    applied: Inventory,
    captivity: Captivity,
    receipt: ConfinementReceipt,
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
    let applied = Inventory {
        id: applied_binding,
        location: InventoryLocation::Carried(facts.target),
        quantity,
        capacity: quantity,
        access: BTreeSet::from([facts.captor]),
        last_granted: None,
        good: Good::Binding,
        spoils_at: None,
    };
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
        source_after: Quantity(
            source
                .quantity
                .0
                .checked_sub(quantity.0)
                .ok_or(Error::Overflow)?,
        ),
        applied,
        captivity,
        receipt,
    })
}
#[derive(Debug)]
pub struct ValidatedConfinement {
    prepared: PreparedConfinement,
}
impl PreparedConfinement {
    pub fn receipt(&self) -> &ConfinementReceipt {
        &self.receipt
    }
    /// All scoped guards precede allocation or debit. The coordinator retains
    /// its foreign epoch through global ID reservation and scene-map creation.
    pub fn begin(
        self,
        stocks: &BTreeMap<InventoryId, Inventory>,
        custody: &BTreeMap<PersonId, Captivity>,
        fresh: &ConfinementFacts,
    ) -> Result<ValidatedConfinement, Error> {
        if fresh != &self.facts
            || stocks.get(&self.source.id) != Some(&self.source)
            || stocks.contains_key(&self.applied.id)
            || custody.contains_key(&self.facts.target)
            || custody.contains_key(&self.facts.captor)
        {
            return Err(Error::Unavailable);
        }
        Ok(ValidatedConfinement { prepared: self })
    }
    pub fn apply(
        self,
        stocks: &mut BTreeMap<InventoryId, Inventory>,
        custody: &mut BTreeMap<PersonId, Captivity>,
        fresh: &ConfinementFacts,
    ) -> Result<ConfinementReceipt, Error> {
        Ok(self.begin(stocks, custody, fresh)?.apply(stocks, custody))
    }
}
impl ValidatedConfinement {
    pub fn receipt(&self) -> &ConfinementReceipt {
        &self.prepared.receipt
    }
    /// The binding is forced placement. No receiving consent, body-room check
    /// or voluntary Load job is involved. The real stock map remains unique.
    pub fn apply(
        self,
        stocks: &mut BTreeMap<InventoryId, Inventory>,
        custody: &mut BTreeMap<PersonId, Captivity>,
    ) -> ConfinementReceipt {
        let plan = self.prepared;
        stocks
            .get_mut(&plan.source.id)
            .expect("guarded binding stock")
            .quantity = plan.source_after;
        stocks.insert(plan.applied.id, plan.applied);
        custody.insert(plan.facts.target, plan.captivity);
        plan.receipt
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
            let mut stocks = BTreeMap::from([(source().id, source())]);
            let mut custody = BTreeMap::new();
            let plan = prepare(facts(), &source(), mode, 100).unwrap();
            let receipt = plan.apply(&mut stocks, &mut custody, &facts()).unwrap();
            assert_eq!(stocks.values().map(|s| s.quantity.0).sum::<u64>(), 2000);
            assert_eq!(receipt.quantity, Quantity(1000));
            assert_eq!(
                receipt.interrupt_target,
                matches!(receipt.mode, ConfinementMode::Timed { .. })
            );
            outcomes.push((stocks, custody));
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
            let mut stocks = BTreeMap::from([(source().id, source())]);
            let mut custody = BTreeMap::new();
            let mut fresh = facts();
            match changed {
                0 => stocks.get_mut(&InventoryId(10)).unwrap().quantity = Quantity(999),
                1 => {
                    stocks.insert(
                        InventoryId(11),
                        Inventory {
                            id: InventoryId(11),
                            ..source()
                        },
                    );
                }
                _ => fresh.surrendered_to_captor = false,
            }
            let before = (stocks.clone(), custody.clone());
            assert_eq!(
                plan.apply(&mut stocks, &mut custody, &fresh),
                Err(Error::Unavailable)
            );
            assert_eq!((stocks, custody), before);
        }
    }
    #[test]
    fn forced_binding_requires_no_recipient_acceptance_and_can_overload_an_existing_body() {
        let mut stocks = BTreeMap::from([
            (source().id, source()),
            (
                InventoryId(12),
                Inventory {
                    id: InventoryId(12),
                    location: InventoryLocation::Carried(PersonId(2)),
                    quantity: Quantity(u64::MAX),
                    capacity: Quantity(u64::MAX),
                    access: BTreeSet::from([PersonId(2)]),
                    good: Good::Timber,
                    ..source()
                },
            ),
        ]);
        let mut custody = BTreeMap::new();
        prepare(facts(), &source(), timed(), 100)
            .unwrap()
            .apply(&mut stocks, &mut custody, &facts())
            .unwrap();
        assert_eq!(
            stocks[&InventoryId(11)].location,
            InventoryLocation::Carried(PersonId(2))
        );
        assert_eq!(stocks[&InventoryId(11)].quantity, Quantity(1000));
        assert_eq!(
            stocks[&InventoryId(11)].access,
            BTreeSet::from([PersonId(1)])
        );
        assert_eq!(stocks[&InventoryId(12)].quantity, Quantity(u64::MAX));
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
}
